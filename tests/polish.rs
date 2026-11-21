//! End-to-end tests for the optional answer polishing path.
//!
//! A tiny HTTP server stands in for a local OpenAI-compatible engine, so the
//! whole path is exercised: config loading, the prompt, the grounding check
//! and the fallback to the verbatim extract.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A one-shot HTTP server that answers every request with `reply`.
struct FakeEngine {
    port: u16,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl FakeEngine {
    fn start(reply: &'static str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = std::thread::spawn(move || {
            for stream in listener.incoming().take(4) {
                let Ok(mut stream) = stream else { continue };
                let mut buffer = [0u8; 8192];
                let _ = stream.read(&mut buffer);
                let body = serde_json::json!({
                    "choices": [{"message": {"role": "assistant", "content": reply}}]
                })
                .to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            }
        });
        Self {
            port,
            handle: Some(handle),
        }
    }
}

impl Drop for FakeEngine {
    fn drop(&mut self) {
        // The listener thread ends when its process exits; nothing to join.
        let _ = self.handle.take();
    }
}

struct Fixture {
    home: tempfile::TempDir,
    #[allow(dead_code)]
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
        let out = paper(home.path(), &["index", "--root", docs.to_str().unwrap()]);
        assert!(out.status.success(), "index failed: {out:?}");
        Self { home, docs }
    }

    fn point_at(&self, port: u16) {
        std::fs::write(
            self.home.path().join("config.toml"),
            format!(
                "[local_engine]\nbase_url = \"http://127.0.0.1:{port}\"\nmodel = \"fake\"\ntimeout_secs = 10\n"
            ),
        )
        .unwrap();
    }

    fn run(&self, args: &[&str]) -> Output {
        paper(self.home.path(), args)
    }
}

fn paper(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_paper"))
        .args(args)
        .env("PAPER_HOME", home)
        .env_remove("PAPER_OFFLINE")
        .output()
        .unwrap()
}

#[test]
fn polish_uses_the_engine_and_keeps_the_citations() {
    let fx = Fixture::new();
    let engine = FakeEngine::start("The warranty covers 24 months [doc:1].");
    fx.point_at(engine.port);

    let out = fx.run(&["answer", "how long is the fridge warranty?", "--polish"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("The warranty covers 24 months"),
        "polished answer missing: {stdout}"
    );
    // The local citations survive the rewrite.
    assert!(stdout.contains("fridge.txt"), "citation missing: {stdout}");
}

#[test]
fn polish_falls_back_to_the_verbatim_extract_when_the_citation_is_wrong() {
    let fx = Fixture::new();
    let engine = FakeEngine::start("A made-up claim [doc:9].");
    fx.point_at(engine.port);

    let out = fx.run(&["answer", "how long is the fridge warranty?", "--polish"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("verbatim extract"),
        "expected a fallback notice: {stderr}"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("made-up claim"),
        "unverified prose must not be shown: {stdout}"
    );
}

#[test]
fn polish_falls_back_when_the_engine_is_unreachable() {
    let fx = Fixture::new();
    // Nothing is listening on this port.
    fx.point_at(1);

    let out = fx.run(&["answer", "how long is the fridge warranty?", "--polish"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("verbatim extract"),
        "expected a fallback notice: {stderr}"
    );
}

#[test]
fn polish_is_refused_when_the_environment_is_offline() {
    let fx = Fixture::new();
    let engine = FakeEngine::start("Should never be used [doc:1].");
    fx.point_at(engine.port);

    let out = Command::new(env!("CARGO_BIN_EXE_paper"))
        .args(["answer", "how long is the fridge warranty?", "--polish"])
        .env("PAPER_HOME", fx.home.path())
        .env("PAPER_OFFLINE", "1")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("Should never be used"),
        "offline mode must not call the engine: {stdout}"
    );
}
