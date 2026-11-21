//! End-to-end product tests, run against the real `paper` binary.
//!
//! These cover the cross-cutting guarantees that unit tests cannot:
//! 1. The complete product flow works fully offline (`PAPER_OFFLINE=1`).
//! 2. A running MCP server sees newly indexed documents without a restart
//!    (a separate `paper index` process writes to the same index).

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Output, Stdio};

fn paper_offline(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_paper"))
        .args(args)
        .env("PAPER_HOME", home)
        .env("PAPER_OFFLINE", "1")
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
        Self { home, docs }
    }

    fn run(&self, args: &[&str]) -> Output {
        paper_offline(self.home.path(), args)
    }
}

#[test]
fn offline_full_product_flow() {
    let fx = Fixture::new();

    // Index offline: no models installed, so this is keyword-only and must
    // not attempt any network access.
    let out = fx.run(&["index", "--root", fx.docs.to_str().unwrap()]);
    assert!(out.status.success(), "index failed: {:?}", out);

    // Search returns exact citations.
    let out = fx.run(&["search", "fridge warranty", "--json"]);
    assert_eq!(out.status.code(), Some(0), "{:?}", out);
    let hits: Value = serde_json::from_slice(&out.stdout).unwrap();
    let hits = hits.as_array().unwrap();
    assert!(!hits.is_empty());
    let hit = &hits[0];
    assert!(hit["path"].as_str().unwrap().ends_with("fridge.txt"));
    assert!(hit["line_start"].as_i64().unwrap() >= 1);
    assert!(hit["line_end"].as_i64().unwrap() >= hit["line_start"].as_i64().unwrap());

    // Answer quotes the document and cites it.
    let out = fx.run(&["answer", "when does the fridge warranty expire?"]);
    assert_eq!(out.status.code(), Some(0), "{:?}", out);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("2027-01-15"), "answer was: {stdout}");
    assert!(stdout.contains("fridge.txt"), "missing citation: {stdout}");

    // A new document is picked up by an incremental reindex and becomes
    // searchable, still fully offline.
    std::fs::write(
        fx.docs.join("tax.txt"),
        "Tax return 2025\nDeadline 2026-04-15.\n",
    )
    .unwrap();
    let out = fx.run(&["index", "--root", fx.docs.to_str().unwrap()]);
    assert!(out.status.success(), "reindex failed: {:?}", out);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("1 unchanged"), "{stdout}");

    let out = fx.run(&["search", "deadline", "--json"]);
    assert_eq!(out.status.code(), Some(0), "{:?}", out);
    let hits: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(hits.as_array().unwrap()[0]["path"]
        .as_str()
        .unwrap()
        .ends_with("tax.txt"));

    // Doctor works offline too.
    let out = fx.run(&["doctor"]);
    assert!(out.status.success(), "{:?}", out);
}

/// Minimal MCP stdio client for the freshness test.
struct McpClient {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
    next_id: i64,
}

impl McpClient {
    fn spawn(home: &Path, docs: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_paper"))
            .args(["serve", "--mcp", "--root", docs.to_str().unwrap()])
            .env("PAPER_HOME", home)
            .env("PAPER_OFFLINE", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        let mut client = Self {
            child,
            stdin,
            stdout,
            next_id: 0,
        };
        client.request(
            "initialize",
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "paper-e2e", "version": "0.0.0"}
            }),
        );
        let msg = json!({"jsonrpc": "2.0", "method": "notifications/initialized", "params": {}});
        writeln!(client.stdin, "{msg}").unwrap();
        client.stdin.flush().unwrap();
        client
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        let msg = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        writeln!(self.stdin, "{msg}").unwrap();
        self.stdin.flush().unwrap();

        let mut line = String::new();
        loop {
            line.clear();
            let n = self.stdout.read_line(&mut line).unwrap();
            assert!(n > 0, "server closed stdout while waiting for {method}");
            let value: Value = serde_json::from_str(line.trim()).unwrap();
            if value.get("id").and_then(Value::as_i64) == Some(id) {
                assert!(
                    value.get("error").is_none(),
                    "{method} failed: {}",
                    value["error"]
                );
                return value["result"].clone();
            }
        }
    }

    fn search(&mut self, query: &str) -> Value {
        let result = self.request(
            "tools/call",
            json!({"name": "search_documents", "arguments": {"query": query}}),
        );
        let text = result["content"][0]["text"].as_str().unwrap_or("");
        serde_json::from_str(text).unwrap_or_else(|_| json!({ "raw": text }))
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn mcp_server_sees_newly_indexed_documents_without_restart() {
    let fx = Fixture::new();

    // Start from an empty index.
    let out = fx.run(&["index", "--root", fx.docs.to_str().unwrap()]);
    assert!(out.status.success(), "index failed: {:?}", out);

    let mut mcp = McpClient::spawn(fx.home.path(), &fx.docs);

    // Baseline: the server finds the initial document.
    let hits = mcp.search("fridge warranty");
    let hits_json = serde_json::to_string(&hits).unwrap();
    assert!(hits_json.contains("fridge.txt"), "baseline: {hits_json}");

    // Index a NEW document from a separate process while the server runs.
    std::fs::write(
        fx.docs.join("lease.txt"),
        "Lease agreement\nThe lease renewal deadline is 2026-12-01.\n",
    )
    .unwrap();
    let out = fx.run(&["index", "--root", fx.docs.to_str().unwrap()]);
    assert!(out.status.success(), "reindex failed: {:?}", out);

    // The SAME server process must see the new chunks: no restart.
    let hits = mcp.search("lease renewal");
    let hits_json = serde_json::to_string(&hits).unwrap();
    assert!(
        hits_json.contains("lease.txt"),
        "server did not see newly indexed document without restart: {hits_json}"
    );
}
