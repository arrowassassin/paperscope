use paper::core::config::{Config, SearchConfig};
use paper::core::paths::Paths;

#[test]
fn default_config_matches_spec() {
    let config = Config::default();
    assert_eq!(config.keyword_sufficiency_threshold, 10);
    assert_eq!(config.rerank_top, 20);
    assert_eq!(config.rrf_k, 60);
    assert_eq!(config.max_chunk_tokens, 400);
    assert_eq!(config.chunk_overlap, 40);
    assert!(config.roots.is_empty());

    let search = SearchConfig::default();
    assert_eq!(search.keyword_limit, 20);
    assert_eq!(search.semantic_limit, 20);
    assert_eq!(search.fused_limit, 50);
}

#[test]
fn paths_resolve_expected_locations() {
    let paths = Paths::with_base("/tmp/paper-test");
    assert!(paths.model_cache_dir().ends_with("paper-test/models"));
    assert!(paths.db_path().ends_with("chunks.db"));
    assert!(paths.config_path().ends_with("config.toml"));
}

#[test]
fn default_paths_live_under_paper() {
    let paths = Paths::new();
    assert!(paths.model_cache_dir().ends_with("paper/models"));
    assert!(paths.db_path().ends_with("chunks.db"));
}

#[test]
fn config_round_trips_through_toml() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let root = dir.path().join("docs");
    let mut config = paper::Config {
        rrf_k: 42,
        ..Default::default()
    };
    config.roots.push(root.clone());
    std::fs::write(&path, toml::to_string_pretty(&config).unwrap()).unwrap();

    let loaded = Config::load_from(&path).unwrap();
    assert_eq!(loaded.rrf_k, 42);
    assert_eq!(loaded.roots, vec![root]);
    // Unspecified fields fall back to defaults rather than failing to parse.
    assert_eq!(loaded.search().keyword_limit, 20);
}

#[test]
fn partial_config_uses_defaults_for_missing_keys() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "rrf_k = 12\n").unwrap();
    let loaded = Config::load_from(&path).unwrap();
    assert_eq!(loaded.rrf_k, 12);
    assert_eq!(loaded.rerank_top, 20);
    assert_eq!(loaded.search().keyword_limit, 20);
}

#[test]
fn missing_config_file_is_defaults_not_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let loaded = Config::load_from(&dir.path().join("nope.toml")).unwrap();
    assert_eq!(loaded.rrf_k, 60);
}
