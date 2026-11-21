//! Integration tests for the daemon IPC protocol.

use paper::daemon::ipc::{run_ipc_server, DaemonRequest, DaemonResponse};
use paper::daemon::DaemonHandler;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::tempdir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

struct TestHandler;

#[async_trait::async_trait]
impl DaemonHandler for TestHandler {
    async fn handle(&self, request: DaemonRequest) -> DaemonResponse {
        match request {
            DaemonRequest::IndexRoots { roots: _ } => DaemonResponse::IndexRoots {
                stats: paper::core::index::IndexStats::default(),
            },
            DaemonRequest::GetStatus => DaemonResponse::Status {
                indexed_files: 0,
                queued: 0,
                paused: false,
                model: None,
            },
            DaemonRequest::Pause => DaemonResponse::Ok,
            DaemonRequest::Resume => DaemonResponse::Ok,
            DaemonRequest::Shutdown => DaemonResponse::Ok,
            DaemonRequest::AddRoot { path: _ } => DaemonResponse::Ok,
            DaemonRequest::RemoveRoot { path: _ } => DaemonResponse::Ok,
        }
    }
}

async fn setup_ipc() -> (tokio::task::JoinHandle<()>, IpcClient) {
    let dir = tempdir().unwrap();
    let socket_path = dir.path().join("paper.sock");
    let socket_path_clone = socket_path.clone();

    let handler = Arc::new(TestHandler);
    let server_handle = tokio::spawn(async move {
        let _ = run_ipc_server(&socket_path_clone, handler).await;
    });

    // Wait for server to be ready
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let stream = UnixStream::connect(&socket_path).await.unwrap();
    let client = IpcClient::new(stream);

    (server_handle, client)
}

struct IpcClient {
    reader: BufReader<tokio::net::unix::OwnedReadHalf>,
    writer: tokio::net::unix::OwnedWriteHalf,
}

impl IpcClient {
    fn new(stream: UnixStream) -> Self {
        let (reader, writer) = stream.into_split();
        Self {
            reader: BufReader::new(reader),
            writer,
        }
    }

    async fn send(&mut self, request: DaemonRequest) -> anyhow::Result<DaemonResponse> {
        let json = serde_json::to_string(&request)?;
        self.writer.write_all(json.as_bytes()).await?;
        self.writer.write_all(b"\n").await?;
        self.writer.flush().await?;

        let mut line = String::new();
        self.reader.read_line(&mut line).await?;
        let response: DaemonResponse = serde_json::from_str(line.trim())?;
        Ok(response)
    }
}

#[tokio::test]
async fn ipc_request_index_roots_returns_ok() {
    let (_server, mut client) = setup_ipc().await;
    let resp = client
        .send(DaemonRequest::IndexRoots { roots: vec![] })
        .await
        .unwrap();
    assert!(matches!(resp, DaemonResponse::IndexRoots { stats: _ }));
}

#[tokio::test]
async fn ipc_request_get_status_returns_ok() {
    let (_server, mut client) = setup_ipc().await;
    let resp = client.send(DaemonRequest::GetStatus).await.unwrap();
    assert!(matches!(resp, DaemonResponse::Status { .. }));
}

#[tokio::test]
async fn ipc_request_pause_resume_returns_ok() {
    let (_server, mut client) = setup_ipc().await;
    let resp = client.send(DaemonRequest::Pause).await.unwrap();
    assert!(matches!(resp, DaemonResponse::Ok));
    let resp = client.send(DaemonRequest::Resume).await.unwrap();
    assert!(matches!(resp, DaemonResponse::Ok));
}

#[tokio::test]
async fn ipc_request_add_remove_root_returns_ok() {
    let (_server, mut client) = setup_ipc().await;
    let resp = client
        .send(DaemonRequest::AddRoot {
            path: PathBuf::from("/tmp/test"),
        })
        .await
        .unwrap();
    assert!(matches!(resp, DaemonResponse::Ok));

    let resp = client
        .send(DaemonRequest::RemoveRoot {
            path: PathBuf::from("/tmp/test"),
        })
        .await
        .unwrap();
    assert!(matches!(resp, DaemonResponse::Ok));
}
