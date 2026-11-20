mod common;

use paper::core::index::{Connection, Indexer};
use paper::core::search::tier0::{fts5_match_expr, tier0_search};
use paper::core::config::SearchConfig;
use paper::core::search::{rrf_fuse, HitSource, RawHit, SearchPipeline};
use std::fs;
use std::path::PathBuf;

struct Fixture {
    _dir: tempfile::TempDir,
    docs: PathBuf,
    indexer: Indexer,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let docs = dir.path().join("docs");
        fs::create_dir(&docs).unwrap();
        fs::write(
            docs.join("fridge.txt"),
            "Fridge warranty\nThe fridge warranty covers 24 months from 2025-01-15.\nIt expires on 2027-01-15.\nSerial number FR-9981.\n",
        )
        .unwrap();
        fs::write(
            docs.join("dishwasher.txt"),
            "Dishwasher manual\nThe dishwasher warranty covers parts and labour for 12 months.\n",
        )
        .unwrap();
        fs::write(
            docs.join("tax.txt"),
            "Tax return 2025\nTotal income 82000. Filing deadline 2026-04-15.\n",
        )
        .unwrap();
        let conn = Connection::open(&dir.path().join("chunks.db")).unwrap();
        let indexer = Indexer::new(conn, Some(common::hash_embedder())).unwrap();
        indexer.index_roots(&[docs.clone()]).unwrap();
        Self {
            _dir: dir,
            docs,
            indexer,
        }
    }

    fn pipeline(&self) -> SearchPipeline {
        let conn = Connection::open(&self._dir.path().join("chunks.db")).unwrap();
        SearchPipeline::new(
            conn,
            Some(common::hash_embedder()),
            None,
            SearchConfig::default(),
        )
    }
}

fn raw(id: i64) -> RawHit {
    RawHit {
        chunk_id: id,
        path: String::new(),
        page: 0,
        line_start: 0,
        line_end: 0,
        snippet: String::new(),
        score: 1.0,
        source: HitSource::Keyword,
    }
}

#[test]
fn keyword_search_finds_the_right_document() {
    let fx = Fixture::new();
    let hits = tier0_search(fx.indexer.connection(), "fridge warranty", &[], 10).unwrap();
    assert!(!hits.is_empty());
    assert!(hits[0].path.ends_with("fridge.txt"));
    assert!(hits[0].snippet.contains("warranty"));
    assert!(hits[0].line_start >= 1);
}

#[test]
fn short_queries_return_nothing_instead_of_scanning() {
    let fx = Fixture::new();
    for q in ["a", "ab", "  ", ""] {
        let hits = tier0_search(fx.indexer.connection(), q, &[], 10).unwrap();
        assert!(hits.is_empty(), "query {q:?} should not hit the index");
    }
}

#[test]
fn queries_cannot_inject_fts5_operators() {
    let fx = Fixture::new();
    assert_eq!(fts5_match_expr("fridge OR *"), "\"fridge\" OR \"*\"");
    assert_eq!(
        fts5_match_expr("when does the fridge warranty expire?"),
        "\"fridge\" OR \"warranty\" OR \"expire?\""
    );
    for q in ["fridge OR *", "\"unterminated", "NEAR(a b)", "a) AND (b"] {
        let result = tier0_search(fx.indexer.connection(), q, &[], 10);
        assert!(result.is_ok(), "query {q:?} errored: {result:?}");
    }
}

#[test]
fn root_filter_excludes_documents_outside_the_allow_list() {
    let fx = Fixture::new();
    let other = tempfile::tempdir().unwrap();
    let hits = tier0_search(
        fx.indexer.connection(),
        "warranty",
        &[other.path().to_path_buf()],
        10,
    )
    .unwrap();
    assert!(hits.is_empty());

    let hits = tier0_search(fx.indexer.connection(), "warranty", &[fx.docs.clone()], 10).unwrap();
    assert!(!hits.is_empty());
}

#[test]
fn rrf_rewards_chunks_present_in_both_lists() {
    let kw = vec![raw(1), raw(2), raw(3)];
    let sem = vec![raw(3), raw(9)];
    let fused = rrf_fuse(&kw, &sem, 60, 10);
    let top = fused[0];
    assert_eq!(top.0, 3, "chunk in both lists must rank first");
    let score_of = |id: i64| fused.iter().find(|(c, _)| *c == id).unwrap().1;
    assert!(score_of(3) > score_of(1));
    assert!(score_of(1) > score_of(9));
}

#[test]
fn rrf_limit_is_respected_and_order_is_stable() {
    let kw: Vec<RawHit> = (0..30).map(raw).collect();
    let fused = rrf_fuse(&kw, &[], 60, 5);
    assert_eq!(fused.len(), 5);
    assert_eq!(fused[0].0, 0);
    let again = rrf_fuse(&kw, &[], 60, 5);
    assert_eq!(fused, again);
}

#[test]
fn pipeline_returns_citations_with_file_and_lines() {
    let fx = Fixture::new();
    let hits = fx.pipeline().search("fridge warranty", &[]).unwrap();
    assert!(!hits.is_empty());
    let hit = &hits[0];
    assert!(hit.path.ends_with("fridge.txt"));
    assert!(hit.line_start >= 1);
    assert!(hit.line_end >= hit.line_start);
    assert!(!hit.snippet.is_empty());
    assert!(!hit.why_matched.is_empty());

    // The citation must actually resolve to the quoted text.
    let source = fs::read_to_string(&hit.path).unwrap();
    let cited: String = source
        .lines()
        .skip((hit.line_start - 1) as usize)
        .take((hit.line_end - hit.line_start + 1) as usize)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(cited.contains("warranty"), "citation did not resolve: {cited}");
}

#[test]
fn pipeline_rejects_queries_under_three_characters() {
    let fx = Fixture::new();
    assert!(fx.pipeline().search("ab", &[]).is_err());
}

#[test]
fn pipeline_works_without_any_model_installed() {
    let fx = Fixture::new();
    let conn = Connection::open(&fx._dir.path().join("chunks.db")).unwrap();
    let pipeline = SearchPipeline::new(conn, None, None, SearchConfig::default());
    let hits = pipeline.search("dishwasher warranty", &[]).unwrap();
    assert!(!hits.is_empty());
    // OR semantics: documents matching more terms must outrank partial matches.
    assert!(hits[0].path.ends_with("dishwasher.txt"), "{hits:?}");
    let dishwasher = hits.iter().position(|h| h.path.ends_with("dishwasher.txt")).unwrap();
    let fridge = hits.iter().position(|h| h.path.ends_with("fridge.txt"));
    if let Some(fridge) = fridge {
        assert!(dishwasher < fridge);
    }
}

#[test]
fn strong_keyword_evidence_skips_the_embedding_model() {
    struct PanicEmbedder;
    impl paper::core::search::QueryEmbedder for PanicEmbedder {
        fn embed(&self, _: &str) -> anyhow::Result<Vec<f32>> {
            panic!("embedder must not be called when keyword hits are sufficient")
        }
    }

    let fx = Fixture::new();
    let conn = Connection::open(&fx._dir.path().join("chunks.db")).unwrap();
    let config = SearchConfig {
        keyword_sufficiency_threshold: 1,
        ..SearchConfig::default()
    };
    let pipeline = SearchPipeline::new(conn, Some(std::sync::Arc::new(PanicEmbedder)), None, config);
    let hits = pipeline.search("fridge warranty", &[]).unwrap();
    assert!(!hits.is_empty());
}

#[test]
fn exact_identifier_match_is_high_confidence() {
    let fx = Fixture::new();
    let hits = fx.pipeline().search("FR-9981", &[]).unwrap();
    assert_eq!(hits.len(), 1);
    assert!(matches!(hits[0].confidence, paper::core::search::Confidence::High));
}

#[test]
fn partial_keyword_match_is_not_reported_as_certain() {
    let fx = Fixture::new();
    // "serial" only exists in the fridge document, so the dishwasher hit is a
    // partial OR match and must not claim high confidence.
    let hits = fx.pipeline().search("warranty serial", &[]).unwrap();
    for hit in &hits {
        if hit.path.ends_with("dishwasher.txt") {
            assert!(
                !matches!(hit.confidence, paper::core::search::Confidence::High),
                "partial match claimed high confidence: {hit:?}"
            );
        }
    }
    let fridge = hits.iter().find(|h| h.path.ends_with("fridge.txt"));
    if let Some(fridge) = fridge {
        assert!(matches!(fridge.confidence, paper::core::search::Confidence::High));
    }
}

#[test]
fn hits_carry_full_content_not_just_a_truncated_snippet() {
    let fx = Fixture::new();
    let hits = fx.pipeline().search("fridge warranty", &[]).unwrap();
    let hit = &hits[0];
    assert!(hit.content.contains("Serial number FR-9981"));
    assert!(hit.snippet.len() <= hit.content.len());
}

#[test]
fn no_match_returns_an_empty_result_not_an_error() {
    let fx = Fixture::new();
    let hits = fx.pipeline().search("zzzunfindable", &[]).unwrap();
    assert!(hits.is_empty());
}
