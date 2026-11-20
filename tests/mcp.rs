use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};

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
                "clientInfo": {"name": "paper-test", "version": "0.0.0"}
            }),
        );
        client.notify("notifications/initialized", json!({}));
        client
    }

    fn notify(&mut self, method: &str, params: Value) {
        let msg = json!({"jsonrpc": "2.0", "method": method, "params": params});
        writeln!(self.stdin, "{msg}").unwrap();
        self.stdin.flush().unwrap();
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let value = self.send(method, params);
        assert!(
            value.get("error").is_none(),
            "{method} failed: {}",
            value["error"]
        );
        value["result"].clone()
    }

    /// Full JSON-RPC response, including any protocol-level error.
    fn send(&mut self, method: &str, params: Value) -> Value {
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
                return value;
            }
        }
    }

    fn call_tool(&mut self, name: &str, arguments: Value) -> Value {
        let result = self.request("tools/call", json!({"name": name, "arguments": arguments}));
        let text = result["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .to_string();
        serde_json::from_str(&text).unwrap_or_else(|_| json!({ "raw": text }))
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn fixture() -> (tempfile::TempDir, PathBuf) {
    let home = tempfile::tempdir().unwrap();
    let docs = home.path().join("docs");
    std::fs::create_dir(&docs).unwrap();
    std::fs::write(
        docs.join("fridge.txt"),
        "Fridge warranty\nThe fridge warranty covers 24 months.\nIt expires on 2027-01-15.\n",
    )
    .unwrap();
    std::fs::write(
        docs.join("tax.txt"),
        "Tax return 2025\nDeadline 2026-04-15.\n",
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_paper"))
        .args(["index", "--root", docs.to_str().unwrap()])
        .env("PAPER_HOME", home.path())
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    (home, docs)
}

#[test]
fn mcp_exposes_exactly_the_four_document_tools() {
    let (home, docs) = fixture();
    let mut client = McpClient::spawn(home.path(), &docs);
    let tools = client.request("tools/list", json!({}));
    let mut names: Vec<String> = tools["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    names.sort();
    assert_eq!(
        names,
        vec![
            "answer_documents",
            "list_sources",
            "read_source",
            "search_documents"
        ]
    );
}

#[test]
fn mcp_search_read_and_answer_round_trip() {
    let (home, docs) = fixture();
    let mut client = McpClient::spawn(home.path(), &docs);

    let hits = client.call_tool("search_documents", json!({"query": "fridge warranty"}));
    let hits = hits.as_array().unwrap();
    assert!(!hits.is_empty());
    let source_id = hits[0]["source_id"].as_i64().unwrap();

    let source = client.call_tool("read_source", json!({"source_id": source_id, "context": 0}));
    let content = source["chunks"][0]["content"].as_str().unwrap();
    assert!(content.contains("2027-01-15"), "{content}");

    let answer = client.call_tool(
        "answer_documents",
        json!({"question": "when does the fridge warranty expire?"}),
    );
    assert_eq!(answer["not_found"], false);
    assert!(answer["text"].as_str().unwrap().contains("2027-01-15"));
    assert!(answer["citations"][0]["reference"]
        .as_str()
        .unwrap()
        .contains("fridge.txt"));

    let sources = client.call_tool("list_sources", json!({}));
    assert_eq!(sources.as_array().unwrap().len(), 2);
}

#[test]
fn mcp_read_source_rejects_unknown_ids_and_bad_queries() {
    let (home, docs) = fixture();
    let mut client = McpClient::spawn(home.path(), &docs);

    let result = client.send(
        "tools/call",
        json!({"name": "read_source", "arguments": {"source_id": 999999}}),
    );
    assert_eq!(result["error"]["code"], -32602);
    assert!(result["error"]["message"]
        .as_str()
        .unwrap()
        .contains("unknown source_id"));

    let short = client.send(
        "tools/call",
        json!({"name": "search_documents", "arguments": {"query": "ab"}}),
    );
    assert!(short["error"]["message"]
        .as_str()
        .unwrap()
        .contains("at least 3 characters"));
}

#[test]
fn mcp_results_are_scoped_to_the_server_roots() {
    let (home, _docs) = fixture();
    let empty = home.path().join("empty");
    std::fs::create_dir(&empty).unwrap();
    let mut client = McpClient::spawn(home.path(), &empty);
    let sources = client.call_tool("list_sources", json!({}));
    assert!(sources.as_array().unwrap().is_empty());
}
