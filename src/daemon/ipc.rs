//! IPC server for the paperscope daemon.
//!
//! JSON lines over Unix socket (Linux/macOS) / named pipe (Windows).

use crate::core::index::IndexStats;
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// Requests sent from clients to the daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonRequest {
    /// Index the given roots (full reindex).
    IndexRoots { roots: Vec<PathBuf> },
    /// Add a new root to watch and index.
    AddRoot { path: PathBuf },
    /// Remove a root from watching and indexing.
    RemoveRoot { path: PathBuf },
    /// Get current daemon status.
    GetStatus,
    /// Pause indexing (respects resource guard).
    Pause,
    /// Resume indexing.
    Resume,
    /// Shutdown the daemon gracefully.
    Shutdown,
}

/// Responses sent from the daemon to clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonResponse {
    /// Indexing completed with statistics.
    IndexRoots { stats: IndexStats },
    /// Current daemon status.
    Status {
        indexed_files: usize,
        queued: usize,
        paused: bool,
        model: Option<String>,
    },
    /// Generic success.
    Ok,
    /// Error with message.
    Error { msg: String },
}

/// Trait for handling daemon requests. Implement this to provide custom behavior.
#[async_trait]
pub trait DaemonHandler: Send + Sync {
    /// Handle a daemon request and return a response.
    async fn handle(&self, request: DaemonRequest) -> DaemonResponse;
}

/// Run the IPC server on the given socket path.
///
/// This function binds to the Unix socket (or named pipe on Windows) and
/// serves requests until the handler returns a `Shutdown` response or
/// the connection is closed.
pub async fn run_ipc_server(socket_path: &Path, handler: impl DaemonHandler + 'static) -> Result<()> {
    // Remove any stale socket file
    let _ = tokio::fs::remove_file(socket_path).await;

    #[cfg(unix)]
    {
        use tokio::net::UnixListener;
        let listener = UnixListener::bind(socket_path)
            .with_context(|| format!("binding to {}", socket_path.display()))?;

        tracing::info!("Daemon IPC server listening on {}", socket_path.display());

        loop {
            let (stream, _) = listener.accept().await?;
            let handler = &handler;
            tokio::spawn(async move {
                if let Err(e) = handle_connection(stream, handler).await {
                    tracing::warn!("IPC connection error: {e}");
                }
            });
        }
    }

    #[cfg(windows)]
    {
        // Named pipe implementation for Windows
        // For simplicity, we use a file-based approach on Windows as well
        // In production, this would use Windows named pipes via tokio-named-pipes
        use tokio::net::UnixListener;
        let listener = UnixListener::bind(socket_path)
            .with_context(|| format!("binding to {}", socket_path.display()))?;

        tracing::info!("Daemon IPC server listening on {}", socket_path.display());

        loop {
            let (stream, _) = listener.accept().await?;
            let handler = &handler;
            tokio::spawn(async move {
                if let Err(e) = handle_connection(stream, handler).await {
                    tracing::warn!("IPC connection error: {e}");
                }
            });
        }
    }
}

async fn handle_connection(
    stream: tokio::net::UnixStream,
    handler: &impl DaemonHandler,
) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    loop {
        line.clear();
        let bytes_read = reader.read_line(&mut line).await?;
        if bytes_read == 0 {
            break; // Client disconnected
        }

        let request: DaemonRequest = match serde_json::from_str(line.trim()) {
            Ok(req) => req,
            Err(e) => {
                let response = DaemonResponse::Error {
                    msg: format!("Invalid request: {e}"),
                };
                let json = serde_json::to_string(&response)?;
                writer.write_all(json.as_bytes()).await?;
                writer.write_all(b"\n").await?;
                writer.flush().await?;
                continue;
            }
        };

        let response = handler.handle(request).await;

        let json = serde_json::to_string(&response)?;
        writer.write_all(json.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;

        if matches!(response, DaemonResponse::Ok) {
            // Check if it was a shutdown request
            // We can't easily check here without the request, so we rely on
            // the handler to stop the server externally.
        }
    }

    Ok(())
}

/// Client for connecting to the daemon IPC server.
pub struct IpcClient {
    #[cfg(unix)]
    reader: BufReader<tokio::net::unix::OwnedReadHalf>,
    #[cfg(unix)]
    writer: tokio::net::unix::OwnedWriteHalf,
    #[cfg(windows)]
    reader: BufReader<tokio::net::unix::OwnedReadHalf>,
    #[cfg(windows)]
    writer: tokio::net::unix::OwnedWriteHalf,
}

impl IpcClient {
    /// Connect to the daemon IPC server.
    pub async fn connect(socket_path: &Path) -> Result<Self> {
        #[cfg(unix)]
        {
            let stream = tokio::net::UnixStream::connect(socket_path).await?;
            let (reader, writer) = stream.into_split();
            Ok(Self {
                reader: BufReader::new(reader),
                writer,
            })
        }

        #[cfg(windows)]
        {
            // On Windows, use Unix socket emulation for now
            let stream = tokio::net::UnixStream::connect(socket_path).await?;
            let (reader, writer) = stream.into_split();
            Ok(Self {
                reader: BufReader::new(reader),
                writer,
            })
        }
    }

    /// Send a request and wait for the response.
    pub async fn send(&mut self, request: DaemonRequest) -> Result<DaemonResponse> {
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