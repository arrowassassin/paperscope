use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn paper(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_paper"))
        .args(args)
        .env("PAPER_HOME", home)
        .env_remove("PAPER_OFFLINE")
        .output()
        .unwrap()
}

struct Fixture {
    home: tempfile::TempDir,
    docs: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let home = tempfile::tempdir().unwrap();
        let docs = home.path().join("docs");
        std::fs::create_dir(&docs).unwrap();
        std::fs::write(
            docs.join("fridge.txt"),
            "Fridge warranty\nThe fridge warranty covers 24 months from 2025-01-15.\nIt expires on 2027-01-15.\n",
        )
        .unwrap();
        std::fs::write(docs.join("tax.txt"), "Tax return 2025\nDeadline 2026-04-15.\n").unwrap();
        let out = paper(
            home.path(),
            &["index", "--root", docs.to_str().unwrap()],
        );
        assert!(out.status.success(), "index failed: {:?}", out);
        Self { home, docs }
    }

    fn run(&self, args: &[&str]) -> Output {
        paper(self.home.path(), args)
    }
}

#[test]
fn index_then_search_returns_json_citations() {
    let fx = Fixture::new();
    let out = fx.run(&["search", "fridge warranty", "--json"]);
    assert_eq!(out.status.code(), Some(0), "{:?}", out);
    let hits: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let hits = hits.as_array().unwrap();
    assert!(!hits.is_empty());
    let hit = &hits[0];
    assert!(hit["path"].as_str().unwrap().ends_with("fridge.txt"));
    assert!(hit["line_start"].as_i64().unwrap() >= 1);
    assert!(hit["snippet"].as_str().unwrap().contains("warranty"));
}

#[test]
fn no_match_exits_with_code_1() {
    let fx = Fixture::new();
    let out = fx.run(&["search", "zzzunfindable", "--json"]);
    assert_eq!(out.status.code(), Some(1), "{:?}", out);
    let hits: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(hits.as_array().unwrap().len(), 0);
}

#[test]
fn short_query_is_an_error_with_code_2() {
    let fx = Fixture::new();
    let out = fx.run(&["search", "ab"]);
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn semantic_flag_without_model_exits_with_code_3() {
    let fx = Fixture::new();
    let out = fx.run(&["search", "fridge", "--semantic"]);
    assert_eq!(out.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&out.stderr).contains("paper models install"));
}

#[test]
fn answer_quotes_the_document_and_cites_it() {
    let fx = Fixture::new();
    let out = fx.run(&["answer", "when does the fridge warranty expire?"]);
    assert_eq!(out.status.code(), Some(0), "{:?}", out);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("2027-01-15"), "answer was: {stdout}");
    assert!(stdout.contains("fridge.txt"), "missing citation: {stdout}");
}

#[test]
fn search_is_limited_to_the_requested_root() {
    let fx = Fixture::new();
    let empty = fx.home.path().join("empty");
    std::fs::create_dir(&empty).unwrap();
    let out = fx.run(&[
        "search",
        "fridge",
        "--json",
        "--root",
        empty.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(1));
    let hits: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(hits.as_array().unwrap().is_empty());
}

#[test]
fn models_list_does_not_download_anything() {
    let home = tempfile::tempdir().unwrap();
    let out = paper(home.path(), &["models", "list"]);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("bge-small-en-v1.5"));
    assert!(stdout.contains("multilingual-e5-small"));
    assert!(stdout.contains("bge-reranker-base"));
    // No weights were fetched as a side effect of listing.
    assert!(!home.path().join("models").join("bge-small-en-v1.5").exists());
}

#[test]
fn doctor_reports_index_stats() {
    let fx = Fixture::new();
    let out = fx.run(&["doctor"]);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("chunks"), "{stdout}");
    assert!(stdout.contains("sqlite-vec"), "{stdout}");
    assert!(fx.docs.exists());
}

#[test]
fn reindex_is_incremental() {
    let fx = Fixture::new();
    let out = fx.run(&["index", "--root", fx.docs.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("indexed 0 file"), "{stdout}");
    assert!(stdout.contains("2 unchanged"), "{stdout}");
}
