//! IPC server for the paperscope daemon.
//!
//! JSON lines over a Unix socket (Linux/macOS). Each line is a JSON-encoded
//! [`DaemonRequest`] or [`DaemonResponse`].

use crate::core::index::IndexStats;
#[cfg(unix)]
use anyhow::Context;
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
#[cfg(unix)]
use std::os::raw::c_int;
#[cfg(any(target_os = "linux", target_os = "android"))]
use std::os::raw::{c_uint, c_void};
#[cfg(unix)]
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::Arc;
#[cfg(unix)]
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// UID of the daemon process, used to authorize IPC peers (C3).
///
/// On Unix every connected peer must run under the same UID as the daemon;
/// otherwise the connection is dropped before any request is handled.
#[cfg(unix)]
fn current_uid() -> Option<u32> {
    unsafe { Some(getuid()) }
}

#[cfg(unix)]
unsafe extern "C" {
    fn getuid() -> u32;
}

/// Peer UID of a connected Unix stream socket.
///
/// Uses `SO_PEERCRED` on Linux/Android and `getpeereid` on BSD/macOS.
#[cfg(unix)]
fn peer_uid(stream: &tokio::net::UnixStream) -> std::io::Result<Option<u32>> {
    peer_uid_fd(stream.as_raw_fd())
}

#[cfg(unix)]
#[cfg(any(target_os = "linux", target_os = "android"))]
fn peer_uid_fd(fd: c_int) -> std::io::Result<Option<u32>> {
    const SOL_SOCKET: c_int = 1;
    const SO_PEERCRED: c_int = 17;

    #[repr(C)]
    struct UCred {
        pid: c_int,
        uid: u32,
        gid: u32,
    }

    unsafe extern "C" {
        fn getsockopt(
            sockfd: c_int,
            level: c_int,
            optname: c_int,
            optval: *mut c_void,
            optlen: *mut c_uint,
        ) -> c_int;
    }

    let mut cred = UCred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let mut len: c_uint = std::mem::size_of::<UCred>() as c_uint;
    let rc = unsafe {
        getsockopt(
            fd,
            SOL_SOCKET,
            SO_PEERCRED,
            &mut cred as *mut UCred as *mut c_void,
            &mut len,
        )
    };
    if rc == 0 {
        Ok(Some(cred.uid))
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(unix)]
#[cfg(not(any(target_os = "linux", target_os = "android")))]
fn peer_uid_fd(fd: c_int) -> std::io::Result<Option<u32>> {
    unsafe extern "C" {
        fn getpeereid(fd: c_int, uid: *mut u32, gid: *mut u32) -> c_int;
    }
    let mut uid: u32 = 0;
    let mut gid: u32 = 0;
    let rc = unsafe { getpeereid(fd, &mut uid, &mut gid) };
    if rc == 0 {
        Ok(Some(uid))
    } else {
        Err(std::io::Error::last_os_error())
    }
}

/// Requests sent from clients to the daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonRequest {
    /// Index the given roots (empty = all configured roots).
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
/// Binds to the Unix socket and serves requests until the task is aborted.
/// Each connection is handled in its own task with a shared handler.
#[cfg(unix)]
pub async fn run_ipc_server(socket_path: &Path, handler: Arc<dyn DaemonHandler>) -> Result<()> {
    use tokio::net::UnixListener;

    // Remove any stale socket file
    let _ = tokio::fs::remove_file(socket_path).await;
    if let Some(parent) = socket_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let listener = UnixListener::bind(socket_path)
        .with_context(|| format!("binding to {}", socket_path.display()))?;

    tracing::info!("Daemon IPC server listening on {}", socket_path.display());

    loop {
        let (stream, _) = listener.accept().await?;
        // C3: only accept connections from the same UID as the daemon.
        if let Some(daemon_uid) = current_uid() {
            match peer_uid(&stream) {
                Ok(Some(uid)) if uid == daemon_uid => {}
                Ok(Some(uid)) => {
                    tracing::warn!(
                        "rejecting IPC connection from peer uid {uid} (daemon uid {daemon_uid})"
                    );
                    continue;
                }
                Ok(None) | Err(_) => {
                    tracing::warn!(
                        "rejecting IPC connection: unable to determine peer credentials"
                    );
                    continue;
                }
            }
        }
        let handler = handler.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, handler).await {
                tracing::warn!("IPC connection error: {e:#}");
            }
        });
    }
}

/// Windows named-pipe support is not implemented yet.
#[cfg(windows)]
pub async fn run_ipc_server(_socket_path: &Path, _handler: Arc<dyn DaemonHandler>) -> Result<()> {
    anyhow::bail!("daemon IPC is not yet supported on Windows")
}

#[cfg(unix)]
async fn handle_connection(
    stream: tokio::net::UnixStream,
    handler: Arc<dyn DaemonHandler>,
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
    }

    Ok(())
}

/// Client for connecting to the daemon IPC server.
pub struct IpcClient {
    #[cfg(unix)]
    reader: BufReader<tokio::net::unix::OwnedReadHalf>,
    #[cfg(unix)]
    writer: tokio::net::unix::OwnedWriteHalf,
}

impl IpcClient {
    /// Connect to the daemon IPC server.
    #[cfg(unix)]
    pub async fn connect(socket_path: &Path) -> Result<Self> {
        let stream = tokio::net::UnixStream::connect(socket_path)
            .await
            .with_context(|| format!("connecting to {}", socket_path.display()))?;
        let (reader, writer) = stream.into_split();
        Ok(Self {
            reader: BufReader::new(reader),
            writer,
        })
    }

    /// Windows named-pipe support is not implemented yet.
    #[cfg(windows)]
    pub async fn connect(_socket_path: &Path) -> Result<Self> {
        anyhow::bail!("daemon IPC is not yet supported on Windows")
    }

    /// Send a request and wait for the response.
    #[cfg(unix)]
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
