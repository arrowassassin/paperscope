use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};

struct McpClient {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
    next_id: i64,
    /// The protocol version the server answered the handshake with.
    negotiated_version: String,
}

impl McpClient {
    fn spawn(home: &Path, docs: &Path) -> Self {
        Self::spawn_with_version(home, docs, "2024-11-05")
    }

    /// Spawn the server and run the `initialize` handshake with the given
    /// protocol version. The negotiated version is kept for assertions.
    fn spawn_with_version(home: &Path, docs: &Path, version: &str) -> Self {
        Self::spawn_with_env(home, docs, version, &[])
    }

    /// `spawn_with_version` with extra environment, e.g. a fake `HOME`.
    fn spawn_with_env(home: &Path, docs: &Path, version: &str, envs: &[(&str, &Path)]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_paper"))
            .args(["serve", "--mcp", "--root", docs.to_str().unwrap()])
            .env("PAPER_HOME", home)
            .envs(envs.iter().copied())
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
            negotiated_version: String::new(),
        };
        let result = client.request(
            "initialize",
            json!({
                "protocolVersion": version,
                "capabilities": {},
                "clientInfo": {"name": "paper-test", "version": "0.0.0"}
            }),
        );
        client.negotiated_version = result["protocolVersion"]
            .as_str()
            .unwrap_or_default()
            .to_string();
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
fn mcp_negotiates_the_protocol_version_of_the_client() {
    let (home, docs) = fixture();

    // An old client keeps working: the server answers the version it asked
    // for, so nothing breaks for clients that have not been updated.
    let old = McpClient::spawn_with_version(home.path(), &docs, "2024-11-05");
    assert_eq!(old.negotiated_version, "2024-11-05");

    // A current client gets the current revision, not the old one.
    let mut new = McpClient::spawn_with_version(home.path(), &docs, "2025-11-25");
    assert_eq!(new.negotiated_version, "2025-11-25");

    // The tools still work after the newer handshake.
    let hits = new.call_tool("search_documents", json!({"query": "fridge warranty"}));
    assert!(!hits.as_array().unwrap().is_empty());
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
            "add_index_root",
            "answer_documents",
            "index_documents",
            "index_status",
            "list_sources",
            "read_source",
            "remove_index_root",
            "search_documents"
        ]
    );
}

#[test]
fn mcp_tools_carry_complete_annotations() {
    let (home, docs) = fixture();
    let mut client = McpClient::spawn(home.path(), &docs);
    let tools = client.request("tools/list", json!({}));
    let tools = tools["tools"].as_array().unwrap();
    for t in tools {
        for hint in [
            "readOnlyHint",
            "destructiveHint",
            "idempotentHint",
            "openWorldHint",
        ] {
            assert!(
                t["annotations"][hint].is_boolean(),
                "{} missing {hint}",
                t["name"]
            );
        }
    }
    let get = |n: &str| tools.iter().find(|t| t["name"] == n).unwrap();
    assert_eq!(
        get("remove_index_root")["annotations"]["destructiveHint"],
        true
    );
    assert_eq!(get("search_documents")["annotations"]["readOnlyHint"], true);
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
// Unix only: these tests fake the home directory via HOME, which Windows ignores (dirs uses the Known Folder API).

/// A fake home with Paper's data in a visible subdirectory, so the hidden-path
/// and Paper-own checks are exercised separately.
#[cfg(unix)]
fn fake_home() -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().canonicalize().unwrap().join("home");
    let data = home.join("paperdata");
    let docs = home.join("docs");
    let notes = home.join(".notes");
    for dir in [&data, &docs, &notes] {
        std::fs::create_dir_all(dir).unwrap();
    }
    std::fs::write(
        data.join("config.toml"),
        "[openrouter]\napi_key = \"sk-test\"\n",
    )
    .unwrap();
    std::fs::write(
        docs.join("fridge.txt"),
        "The fridge warranty covers 24 months.\n",
    )
    .unwrap();
    std::fs::write(docs.join(".env.txt"), "TOKEN=hunter2\n").unwrap();
    std::fs::write(notes.join("secret.txt"), "password hunter2\n").unwrap();
    std::fs::write(home.join(".netrc.txt"), "machine x password hunter2\n").unwrap();
    (tmp, home, data, docs)
}

#[cfg(unix)]
fn assert_refused_to_index(client: &mut McpClient, tool: &str, path: &Path) {
    let result = client.send(
        "tools/call",
        json!({"name": tool, "arguments": {"path": path}}),
    );
    assert_eq!(result["error"]["code"], -32600, "{tool} {path:?}: {result}");
    assert!(
        result["error"]["message"]
            .as_str()
            .unwrap()
            .contains("refusing to index"),
        "{result}"
    );
}

#[cfg(unix)]
#[test]
fn mcp_refuses_to_index_hidden_paths_and_papers_own_data() {
    let (_tmp, home, data, docs) = fake_home();
    let mut client = McpClient::spawn_with_env(&data, &docs, "2024-11-05", &[("HOME", &home)]);

    let refused = [
        ("index_documents", home.join(".notes")),
        ("index_documents", home.join(".notes/secret.txt")),
        ("index_documents", home.join(".netrc.txt")),
        ("index_documents", docs.join(".env.txt")),
        ("index_documents", data.join("config.toml")),
        ("index_documents", data.clone()),
        ("add_index_root", home.join(".notes")),
        ("add_index_root", data.clone()),
    ];
    for (tool, path) in refused {
        assert_refused_to_index(&mut client, tool, &path);
    }

    // An ordinary document is still indexed.
    let ok = client.call_tool("index_documents", json!({"path": docs.join("fridge.txt")}));
    assert_eq!(ok["stats"]["indexed"], 1, "{ok}");
}

#[cfg(unix)]
#[test]
fn mcp_refuses_a_parent_of_papers_dir_only_when_a_walk_would_reach_it() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().canonicalize().unwrap().join("home");
    let visible = home.join("work/paperdata");
    let hidden = home.join("hidden/.paper");
    let docs = home.join("docs");
    for dir in [&visible, &hidden, &docs] {
        std::fs::create_dir_all(dir).unwrap();
    }
    std::fs::write(home.join("hidden/note.txt"), "boiler serviced annually\n").unwrap();

    // Visible Paper dir: its parent's walk would read the config.
    let mut client = McpClient::spawn_with_env(&visible, &docs, "2024-11-05", &[("HOME", &home)]);
    assert_refused_to_index(&mut client, "index_documents", &home.join("work"));
    assert_refused_to_index(&mut client, "add_index_root", &home.join("work"));
    drop(client);

    // Hidden Paper dir: a walk skips it, so indexing the parent stays allowed...
    let mut client = McpClient::spawn_with_env(&hidden, &docs, "2024-11-05", &[("HOME", &home)]);
    let ok = client.call_tool("index_documents", json!({"path": home.join("hidden")}));
    assert_eq!(ok["stats"]["indexed"], 1, "{ok}");
    // ...but a permanent, watched root over it is not (watcher events from
    // hidden subdirectories reach the indexer by file name alone).
    assert_refused_to_index(&mut client, "add_index_root", &home.join("hidden"));
}

#[cfg(unix)]
#[test]
fn mcp_remove_index_root_still_removes_a_hidden_root() {
    let (_tmp, home, data, docs) = fake_home();
    let notes = home.join(".notes");
    let mut client = McpClient::spawn_with_env(&data, &notes, "2024-11-05", &[("HOME", &home)]);
    // Removing the only root is refused, so add a second one first.
    client.call_tool("add_index_root", json!({"path": docs}));

    let removed = client.call_tool("remove_index_root", json!({"path": notes}));
    assert_eq!(removed["removed_root"], json!(notes), "{removed}");
    let status = client.call_tool("index_status", json!({}));
    assert_eq!(status["roots"], json!([docs]), "{status}");
}

/// An empty root list means "no restriction", so the last root must stay.
fn assert_refused_as_last_root(client: &mut McpClient, path: &Path) {
    let refused = client.send(
        "tools/call",
        json!({"name": "remove_index_root", "arguments": {"path": path}}),
    );
    assert!(
        refused["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("last index root"),
        "{refused}"
    );
}

#[test]
fn mcp_refuses_to_remove_the_last_root() {
    let (home, docs) = fixture();
    let docs = docs.canonicalize().unwrap();
    let mut client = McpClient::spawn(home.path(), &docs);
    assert_refused_as_last_root(&mut client, &docs);
    let status = client.call_tool("index_status", json!({}));
    assert_eq!(status["roots"], json!([docs]));
}

#[test]
fn mcp_refuses_to_empty_the_persisted_roots() {
    let (home, _docs) = fixture();
    let shared = home.path().canonicalize().unwrap().join("shared");
    // Nested under the session root only so the path vetting accepts it
    // without faking $HOME; it is still the one persisted root.
    let notes = shared.join("notes");
    std::fs::create_dir_all(&notes).unwrap();
    let config_path = home.path().join("config.toml");
    let config = format!("roots = [{:?}]\n", notes.to_str().unwrap());
    std::fs::write(&config_path, &config).unwrap();

    let mut client = McpClient::spawn(home.path(), &shared);
    assert_refused_as_last_root(&mut client, &notes);
    assert_eq!(std::fs::read_to_string(&config_path).unwrap(), config);
}

#[test]
fn mcp_reports_background_setup_only_while_it_runs() {
    let (home, docs) = fixture();
    let mut client = McpClient::spawn(home.path(), &docs);
    let status = client.call_tool("index_status", json!({}));
    assert!(status["setup_in_progress"].is_null(), "{status}");
    let hits = client.call_tool("search_documents", json!({"query": "fridge warranty"}));
    assert!(hits.is_array(), "{hits}");
    drop(client);

    // PAPER_HOME is the data dir itself (Paths::base_dir).
    let phase = "indexing your documents for the first time";
    std::fs::write(home.path().join("setup-status"), format!("{phase}\n")).unwrap();
    let mut client = McpClient::spawn(home.path(), &docs);
    let status = client.call_tool("index_status", json!({}));
    assert_eq!(status["setup_in_progress"], phase);
    let raw = client.request(
        "tools/call",
        json!({"name": "search_documents", "arguments": {"query": "fridge warranty"}}),
    );
    let hits: Value = serde_json::from_str(raw["content"][0]["text"].as_str().unwrap()).unwrap();
    assert!(hits.is_array(), "{raw}");
    let note = raw["content"][1]["text"].as_str().unwrap();
    assert!(note.contains(phase), "{raw}");
    let answer = client.call_tool("answer_documents", json!({"question": "fridge warranty"}));
    assert_eq!(answer["setup_in_progress"], phase);
}
