use paper::core::models::{
    all_specs, spec, ModelKind, ModelManager, DEFAULT_EMBEDDING, EMBEDDING_DIMS,
    MULTILINGUAL_EMBEDDING, OPTIONAL_RERANKER,
};

#[test]
fn every_model_is_pinned_ungated_and_complete() {
    for spec in all_specs() {
        assert_eq!(spec.revision.len(), 40, "{} must pin a commit", spec.id);
        assert!(
            spec.repo.starts_with("BAAI/") || spec.repo.starts_with("intfloat/"),
            "{} must come from a first-party publisher",
            spec.id
        );
        assert!(spec.files.iter().any(|f| f.name == "model.safetensors"));
        assert!(spec.files.iter().any(|f| f.name == "tokenizer.json"));
        assert!(spec.files.iter().any(|f| f.name == "config.json"));
        for file in spec.files {
            assert_eq!(file.sha256.len(), 64, "{} must pin a SHA256", file.name);
            assert!(file.size > 0);
        }
    }
}

#[test]
fn default_install_fits_the_200mb_budget() {
    let spec = spec(DEFAULT_EMBEDDING).unwrap();
    assert_eq!(spec.kind, ModelKind::Embedding);
    assert_eq!(spec.dims, EMBEDDING_DIMS);
    assert!(
        spec.size_mb() <= 200,
        "default model is {} MB, budget is 200 MB",
        spec.size_mb()
    );
}

#[test]
fn embedders_share_one_vector_width() {
    for id in [DEFAULT_EMBEDDING, MULTILINGUAL_EMBEDDING] {
        assert_eq!(spec(id).unwrap().dims, EMBEDDING_DIMS);
    }
    assert_eq!(spec(OPTIONAL_RERANKER).unwrap().kind, ModelKind::Reranker);
}

#[test]
fn unknown_model_is_an_error_not_a_download() {
    let dir = tempfile::tempdir().unwrap();
    let manager = ModelManager::with_cache_dir(dir.path().to_path_buf());
    assert!(manager.spec("nonexistent").is_err());
    assert!(!manager.is_installed("nonexistent"));
    assert!(manager.load_embedder(OPTIONAL_RERANKER).is_err());
}

#[test]
fn missing_model_is_reported_without_network_access() {
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("PAPER_OFFLINE", "1");
    let manager = ModelManager::with_cache_dir(dir.path().to_path_buf());
    assert!(!manager.is_installed(DEFAULT_EMBEDDING));
    let result = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(manager.ensure(DEFAULT_EMBEDDING));
    std::env::remove_var("PAPER_OFFLINE");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("PAPER_OFFLINE"));
}

#[test]
fn checksum_verification_detects_corruption() {
    use paper::core::models::verify_sha256_public;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("weights.bin");
    std::fs::write(&file, b"not the real weights").unwrap();

    let digest = format!("{:x}", sha2::Sha256::digest(b"not the real weights"));
    assert!(verify_sha256_public(&file, &digest).unwrap());
    assert!(!verify_sha256_public(&file, &"0".repeat(64)).unwrap());

    // A corrupt cache must never be reported as installed.
    let manager = ModelManager::with_cache_dir(dir.path().to_path_buf());
    let model_dir = manager.model_dir(DEFAULT_EMBEDDING);
    std::fs::create_dir_all(&model_dir).unwrap();
    for f in spec(DEFAULT_EMBEDDING).unwrap().files {
        std::fs::write(model_dir.join(f.name), b"corrupt").unwrap();
    }
    assert!(!manager.is_installed(DEFAULT_EMBEDDING));
}

use sha2::Digest;

/// Real end-to-end model check: downloads 134 MB into a temporary cache, verifies
/// every pinned hash, then checks embedding semantics. Ignored by default so a
/// normal `cargo test` never touches the network.
#[test]
#[ignore = "downloads 134 MB; run with `cargo test -- --ignored`"]
fn real_model_downloads_verifies_and_embeds() {
    let dir = tempfile::tempdir().unwrap();
    let manager = ModelManager::with_cache_dir(dir.path().to_path_buf());
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(manager.ensure(DEFAULT_EMBEDDING)).unwrap();
    assert!(manager.is_installed(DEFAULT_EMBEDDING));

    let embedder = manager.load_embedder(DEFAULT_EMBEDDING).unwrap();
    assert_eq!(embedder.dims(), EMBEDDING_DIMS);

    let query = embedder.embed_query("how long is the fridge warranty").unwrap();
    assert_eq!(query.len(), EMBEDDING_DIMS);
    let norm = query.iter().map(|x| x * x).sum::<f32>().sqrt();
    assert!((norm - 1.0).abs() < 1e-3, "embedding must be unit length");

    let related = embedder
        .embed_document("The refrigerator warranty lasts 24 months from purchase.")
        .unwrap();
    let unrelated = embedder
        .embed_document("Quarterly tax filings are due in April.")
        .unwrap();
    let dot = |a: &[f32], b: &[f32]| a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>();
    let related_score = dot(&query, &related);
    let unrelated_score = dot(&query, &unrelated);
    assert!(
        related_score > unrelated_score,
        "related ({related_score}) must outrank unrelated ({unrelated_score})"
    );
}

/// Multilingual model: same encoder code path, different vocab and prefixes.
#[test]
#[ignore = "downloads 487 MB"]
fn multilingual_model_embeds_across_languages() {
    let dir = tempfile::tempdir().unwrap();
    let manager = ModelManager::with_cache_dir(dir.path().to_path_buf());
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(manager.ensure(MULTILINGUAL_EMBEDDING))
        .unwrap();
    let embedder = manager.load_embedder(MULTILINGUAL_EMBEDDING).unwrap();

    let english = embedder.embed_query("fridge warranty duration").unwrap();
    let french = embedder
        .embed_document("La garantie du réfrigérateur est de 24 mois.")
        .unwrap();
    let unrelated = embedder
        .embed_document("Déclaration de revenus pour l'année 2025.")
        .unwrap();
    assert_eq!(english.len(), EMBEDDING_DIMS);
    let dot = |a: &[f32], b: &[f32]| a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>();
    assert!(
        dot(&english, &french) > dot(&english, &unrelated),
        "cross-lingual match failed: {} vs {}",
        dot(&english, &french),
        dot(&english, &unrelated)
    );
}

/// The opt-in cross-encoder must actually re-order candidates by relevance.
#[test]
#[ignore = "downloads 1.13 GB"]
fn reranker_scores_relevant_passage_higher() {
    use paper::core::search::{Confidence, Engine, Hit};
    let dir = tempfile::tempdir().unwrap();
    let manager = ModelManager::with_cache_dir(dir.path().to_path_buf());
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(manager.ensure(OPTIONAL_RERANKER))
        .unwrap();
    let reranker = manager.load_reranker().unwrap();

    let hit = |id: i64, snippet: &str| Hit {
        source_id: id,
        path: format!("/doc{id}.txt"),
        source_path: format!("/doc{id}.txt"),
        page: 0,
        line_start: 1,
        line_end: 2,
        anchor: None,
        snippet: snippet.to_string(),
        content: snippet.to_string(),
        score: 0.0,
        why_matched: String::new(),
        confidence: Confidence::Low,
        engine: Engine::Local,
    };
    let hits = vec![
        hit(1, "Income tax filings are due in April 2026."),
        hit(2, "The fridge warranty covers 24 months and expires on 2027-01-15."),
    ];
    let ranked = reranker.rerank("when does the fridge warranty expire?", hits).unwrap();
    assert_eq!(ranked[0].source_id, 2, "{ranked:?}");
    assert!(ranked[0].score > ranked[1].score);
    assert!(ranked[0].score > 0.5, "relevant passage should score high");
}
