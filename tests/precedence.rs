use chrono::NaiveDate;
use paper::core::config::{Config, SearchConfig};
use paper::core::effective::{resolve, Source};
use paper::core::models::{
    ModelKind, ModelManager, MMARCO_RERANKER, MULTILINGUAL_EMBEDDING, OPTIONAL_RERANKER,
};
use paper::core::remote_config::RemoteConfig;

const REV: &str = "0123456789abcdef0123456789abcdef01234567";
const SHA: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 25).unwrap()
}

fn remote(expires: &str) -> RemoteConfig {
    toml::from_str(&format!(
        r#"
schema = 1
version = 3
expires = "{expires}"
[defaults]
reranker = "{MMARCO_RERANKER}"
[search]
rerank_top = 30
semantic_min_score = 0.4
[[models]]
id = "remote-rerank"
kind = "reranker"
arch = "xlm-roberta-seq-cls"
repo = "BAAI/remote-rerank"
revision = "{REV}"
languages = "multilingual"
files = [ {{ name = "model.safetensors", sha256 = "{SHA}", size = 10 }} ]
"#
    ))
    .unwrap()
}

#[test]
fn local_pin_beats_remote_beats_builtin() {
    let remote = remote("2027-01-01");
    let pinned = Config {
        reranker: Some(OPTIONAL_RERANKER.into()),
        ..Default::default()
    };
    let e = resolve(&pinned, Some(&remote), today());
    assert_eq!(
        (e.reranker.as_str(), e.reranker_source),
        (OPTIONAL_RERANKER, Source::Pinned)
    );

    let e = resolve(&Config::default(), Some(&remote), today());
    assert_eq!(
        (e.reranker.as_str(), e.reranker_source),
        (MMARCO_RERANKER, Source::Remote)
    );
    // Remote sets no embedder, so the built-in one applies.
    assert_eq!(
        (e.embedder.as_str(), e.embedder_source),
        (MULTILINGUAL_EMBEDDING, Source::BuiltIn)
    );

    let e = resolve(&Config::default(), None, today());
    assert_eq!(e.reranker_source, Source::BuiltIn);
}

#[test]
fn unset_local_search_values_take_remote_then_builtin() {
    let remote = remote("2027-01-01");
    let local = Config {
        rerank_top: Some(5),
        ..Default::default()
    };
    let s = local.search(Some(&remote));
    assert_eq!(s.rerank_top, 5); // local wins
    assert_eq!(s.semantic_min_score, 0.4); // remote
    assert_eq!(s.rrf_k, SearchConfig::default().rrf_k); // built-in
    let s = Config::default().search(None);
    assert_eq!(s.rerank_top, SearchConfig::default().rerank_top);
}

#[test]
fn expired_remote_contributes_nothing() {
    let e = resolve(&Config::default(), Some(&remote("2026-11-24")), today());
    assert_eq!(e.reranker, OPTIONAL_RERANKER);
    assert_eq!(e.reranker_source, Source::BuiltIn);
}

#[test]
fn old_config_files_still_parse() {
    let c: Config = toml::from_str("rrf_k = 12\nrerank_top = 7\n").unwrap();
    assert_eq!((c.rrf_k, c.rerank_top), (Some(12), Some(7)));
    assert!(c.embedder.is_none() && c.reranker.is_none());
}

#[test]
fn saving_writes_only_fields_that_are_set() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    Config::default().save_to(&path).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    for key in [
        "embedder",
        "reranker",
        "rrf_k",
        "rerank_top",
        "keyword_sufficiency",
        "semantic_min",
    ] {
        assert!(
            !text.contains(key),
            "{key} was frozen into the file:\n{text}"
        );
    }
}

#[test]
fn remote_models_appear_in_models_list() {
    let dir = tempfile::tempdir().unwrap();
    let manager = ModelManager::with_cache_dir(dir.path().to_path_buf())
        .with_remote(Some(&remote("2027-01-01")));
    let listed = manager.list();
    let m = listed.iter().find(|m| m.id == "remote-rerank").unwrap();
    assert_eq!(m.kind, ModelKind::Reranker);
    assert!(!m.installed);
    assert!(manager.spec("remote-rerank").is_ok());
}

#[test]
fn remote_embedder_prefixes_override_the_org_guess() {
    let model = |extra: &str| -> RemoteConfig {
        toml::from_str(&format!(
            r#"
schema = 1
version = 1
expires = "2027-01-01"
[[models]]
id = "remote-emb"
kind = "embedder"
arch = "bert"
dims = 384
repo = "intfloat/remote-emb"
revision = "{REV}"
languages = "multilingual"
{extra}
files = [ {{ name = "model.safetensors", sha256 = "{SHA}", size = 10 }} ]
"#
        ))
        .unwrap()
    };
    let dir = tempfile::tempdir().unwrap();
    let spec_prefixes = |cfg: &RemoteConfig| {
        let m = ModelManager::with_cache_dir(dir.path().to_path_buf()).with_remote(Some(cfg));
        let s = m.spec("remote-emb").unwrap();
        (s.query_prefix, s.document_prefix)
    };
    assert_eq!(spec_prefixes(&model("")), ("query: ", "passage: "));
    assert_eq!(
        spec_prefixes(&model("query_prefix = \"Q> \"\ndocument_prefix = \"\"")),
        ("Q> ", "")
    );
}
