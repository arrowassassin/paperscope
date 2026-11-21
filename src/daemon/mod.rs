//! Daemon core: IPC server, scheduler, watcher, resource guard, and auto-discovery.

pub mod auto_discover;
pub mod ipc;
pub mod resource_guard;
pub mod scheduler;
pub mod watcher;

pub use ipc::DaemonHandler;

use crate::cli::Workspace;
use crate::core::index::{init_db, Connection, Indexer};
use crate::core::search::SharedEmbedder;
use crate::daemon::auto_discover::discover_default_roots;
use crate::daemon::ipc::{run_ipc_server, DaemonRequest, DaemonResponse};
use crate::daemon::resource_guard::{PauseReason, ResourceGuard, ResourceGuardConfig};
use crate::daemon::scheduler::RootAwareScheduler;
use crate::daemon::watcher::Watcher;
use anyhow::Result;
use clap::Args;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::signal;
use tokio::sync::{mpsc, RwLock};

/// Arguments for the daemon subcommand.
#[derive(Args, Debug, Clone)]
pub struct DaemonArgs {
    /// Run in foreground (don't daemonize)
    #[arg(long)]
    pub foreground: bool,

    /// Socket path for IPC (default: ~/.cache/paper/daemon.sock)
    #[arg(long)]
    pub socket: Option<PathBuf>,

    /// Roots to index (default: auto-discover Documents, Downloads, Desktop)
    #[arg(long, short)]
    pub root: Vec<PathBuf>,

    /// Disable file system watcher
    #[arg(long)]
    pub no_watch: bool,

    /// Disable scheduler
    #[arg(long)]
    pub no_schedule: bool,

    /// Disable resource guard (battery/CPU monitoring)
    #[arg(long)]
    pub no_resource_guard: bool,

    /// Full reindex cron expression (default: "0 3 * * *" = daily 3 AM)
    #[arg(long)]
    pub full_cron: Option<String>,

    /// Incremental index interval in seconds (default: 300 = 5 min)
    #[arg(long)]
    pub incremental_interval: Option<u64>,

    /// Debounce time for file watcher in milliseconds (default: 250)
    #[arg(long)]
    pub debounce_ms: Option<u64>,

    /// Battery threshold percentage (default: 20)
    #[arg(long)]
    pub battery_threshold: Option<u8>,

    /// CPU threshold percentage (default: 80)
    #[arg(long)]
    pub cpu_threshold: Option<u8>,
}

/// Shared daemon state.
///
/// Deliberately does NOT hold an `Indexer` or `Connection`: rusqlite
/// connections are not `Sync`, so each indexing request opens a fresh
/// connection inside `spawn_blocking` (the same pattern the CLI uses).
struct DaemonState {
    roots: RwLock<Vec<PathBuf>>,
    paused: RwLock<bool>,
    db_path: PathBuf,
    embedder: Option<SharedEmbedder>,
}

/// Daemon request handler.
struct DaemonHandlerImpl {
    state: Arc<DaemonState>,
    scheduler: Arc<RootAwareScheduler>,
    watcher: Arc<RwLock<Option<Watcher>>>,
}

#[async_trait::async_trait]
impl DaemonHandler for DaemonHandlerImpl {
    async fn handle(&self, request: DaemonRequest) -> DaemonResponse {
        match request {
            DaemonRequest::IndexRoots { roots } => {
                let roots_to_index = if roots.is_empty() {
                    self.state.roots.read().await.clone()
                } else {
                    roots
                };

                if roots_to_index.is_empty() {
                    return DaemonResponse::Error {
                        msg: "No roots configured for indexing".to_string(),
                    };
                }

                if *self.state.paused.read().await {
                    return DaemonResponse::Error {
                        msg: "Indexing paused by resource guard".to_string(),
                    };
                }

                let db_path = self.state.db_path.clone();
                let embedder = self.state.embedder.clone();

                // Indexing is blocking CPU/IO work and `Indexer` is not `Send`:
                // create it inside the blocking task and run it there.
                match tokio::task::spawn_blocking(
                    move || -> Result<crate::core::index::IndexStats> {
                        let conn = Connection::open(&db_path)?;
                        init_db(&conn)?;
                        let indexer = Indexer::new(conn, embedder)?;
                        indexer.index_roots(&roots_to_index)
                    },
                )
                .await
                {
                    Ok(Ok(stats)) => DaemonResponse::IndexRoots { stats },
                    Ok(Err(e)) => DaemonResponse::Error {
                        msg: format!("Indexing failed: {e}"),
                    },
                    Err(e) => DaemonResponse::Error {
                        msg: format!("Indexing task failed: {e}"),
                    },
                }
            }
            DaemonRequest::AddRoot { path } => {
                if !path.exists() {
                    return DaemonResponse::Error {
                        msg: format!("Path does not exist: {}", path.display()),
                    };
                }

                let mut roots = self.state.roots.write().await;
                if !roots.contains(&path) {
                    roots.push(path.clone());
                    drop(roots);

                    if let Some(watcher) = self.watcher.write().await.as_mut() {
                        let _ = watcher.add_root(path.clone()).await;
                    }
                    self.scheduler.add_root(path).await;
                }
                DaemonResponse::Ok
            }
            DaemonRequest::RemoveRoot { path } => {
                self.state.roots.write().await.retain(|r| r != &path);

                if let Some(watcher) = self.watcher.write().await.as_mut() {
                    let _ = watcher.remove_root(&path).await;
                }
                self.scheduler.remove_root(&path).await;
                DaemonResponse::Ok
            }
            DaemonRequest::GetStatus => {
                let indexed_files = self.count_indexed_files();
                let paused = *self.state.paused.read().await;
                let model = self.stored_embedding_model();

                DaemonResponse::Status {
                    indexed_files,
                    queued: 0,
                    paused,
                    model,
                }
            }
            DaemonRequest::Pause => {
                *self.state.paused.write().await = true;
                DaemonResponse::Ok
            }
            DaemonRequest::Resume => {
                *self.state.paused.write().await = false;
                DaemonResponse::Ok
            }
            DaemonRequest::Shutdown => DaemonResponse::Ok,
        }
    }
}

impl DaemonHandlerImpl {
    fn count_indexed_files(&self) -> usize {
        let Ok(conn) = Connection::open(&self.state.db_path) else {
            return 0;
        };
        conn.query_i64("SELECT COUNT(DISTINCT path) FROM chunks")
            .unwrap_or(0) as usize
    }

    fn stored_embedding_model(&self) -> Option<String> {
        let Ok(conn) = Connection::open(&self.state.db_path) else {
            return None;
        };
        conn.stored_embedding_model().ok().flatten()
    }
}

/// Run the daemon until SIGTERM/SIGINT or an IPC `Shutdown` request.
pub async fn run_daemon(args: DaemonArgs) -> Result<i32> {
    let workspace = Workspace::new();

    // Determine socket path
    let socket_path = args
        .socket
        .unwrap_or_else(|| workspace.paths.base_dir().join("daemon.sock"));

    // Ensure parent directory exists
    if let Some(parent) = socket_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    // Determine roots to index
    let roots = if args.root.is_empty() {
        discover_default_roots()
    } else {
        args.root.clone()
    };

    if roots.is_empty() {
        tracing::warn!("No roots to index (auto-discovery found nothing and no --root provided)");
    } else {
        tracing::info!("Daemon will index: {:?}", roots);
    }

    // Load the embedder once (only if already on disk — never download here).
    let embedder = workspace.embedder();
    let db_path = workspace.paths.db_path();

    // Create channels
    let (request_tx, mut request_rx) = mpsc::channel::<DaemonRequest>(100);

    // Create shared state
    let state = Arc::new(DaemonState {
        roots: RwLock::new(roots.clone()),
        paused: RwLock::new(false),
        db_path,
        embedder,
    });

    // Create scheduler
    let full_cron = args.full_cron.unwrap_or_else(|| "0 3 * * *".to_string());
    let incremental_interval = args
        .incremental_interval
        .map(std::time::Duration::from_secs)
        .unwrap_or_else(|| std::time::Duration::from_secs(5 * 60));
    let scheduler = Arc::new(RootAwareScheduler::new(
        &full_cron,
        incremental_interval,
        request_tx.clone(),
    )?);
    scheduler.set_roots(roots.clone()).await;

    // Create watcher
    let debounce_ms = args.debounce_ms.unwrap_or(250);
    let mut watcher = Watcher::new(debounce_ms, request_tx.clone());
    for root in &roots {
        watcher.add_root(root.clone()).await?;
    }
    let watcher = Arc::new(RwLock::new(Some(watcher)));

    // Create resource guard
    let mut resource_config = ResourceGuardConfig::default();
    if let Some(b) = args.battery_threshold {
        resource_config.battery_threshold = b;
    }
    if let Some(c) = args.cpu_threshold {
        resource_config.cpu_threshold = c;
    }
    let resource_guard = Arc::new(ResourceGuard::with_config(resource_config));

    // Create handler
    let handler: Arc<dyn DaemonHandler> = Arc::new(DaemonHandlerImpl {
        state: state.clone(),
        scheduler: scheduler.clone(),
        watcher: watcher.clone(),
    });

    // Start components
    if !args.no_schedule {
        scheduler.start().await;
    }

    if !args.no_watch {
        if let Some(w) = watcher.write().await.as_mut() {
            w.start().await?;
        }
    }

    if !args.no_resource_guard {
        let (pause_tx, mut pause_rx) = mpsc::channel::<PauseReason>(10);
        resource_guard.start_monitoring(pause_tx);
        let state = state.clone();
        tokio::spawn(async move {
            while let Some(reason) = pause_rx.recv().await {
                match reason {
                    PauseReason::Resumed => {
                        *state.paused.write().await = false;
                        tracing::info!("Resource guard: resuming indexing");
                    }
                    other => {
                        *state.paused.write().await = true;
                        tracing::warn!("Resource guard: pausing indexing due to {:?}", other);
                    }
                }
            }
        });
    }

    // Start IPC server task
    let socket_path_clone = socket_path.clone();
    let handler_clone = handler.clone();
    let ipc_handle = tokio::spawn(async move {
        if let Err(e) = run_ipc_server(&socket_path_clone, handler_clone).await {
            tracing::error!("IPC server error: {e}");
        }
    });

    // Handle shutdown signals
    let shutdown = async {
        #[cfg(unix)]
        {
            let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate())?;
            let mut sigint = signal::unix::signal(signal::unix::SignalKind::interrupt())?;
            tokio::select! {
                _ = sigterm.recv() => tracing::info!("Received SIGTERM"),
                _ = sigint.recv() => tracing::info!("Received SIGINT"),
            }
        }
        #[cfg(windows)]
        {
            signal::ctrl_c().await?;
            tracing::info!("Received Ctrl+C");
        }
        Ok::<(), anyhow::Error>(())
    };

    // Dispatch loop: requests from the scheduler and watcher are handled here;
    // IPC requests are handled by the per-connection tasks in `run_ipc_server`.
    let dispatch = async {
        while let Some(request) = request_rx.recv().await {
            if matches!(request, DaemonRequest::Shutdown) {
                break;
            }
            let handler = handler.clone();
            tokio::spawn(async move {
                let _ = handler.handle(request).await;
            });
        }
    };

    // Run until shutdown signal or shutdown request
    tokio::select! {
        _ = shutdown => {
            tracing::info!("Shutting down daemon...");
        }
        _ = dispatch => {
            tracing::info!("Shutdown requested via IPC...");
        }
    }

    // Graceful shutdown
    scheduler.stop().await;

    if let Some(w) = watcher.write().await.as_mut() {
        w.stop().await;
    }

    ipc_handle.abort();
    let _ = ipc_handle.await;

    // Clean up socket file
    let _ = tokio::fs::remove_file(&socket_path).await;

    tracing::info!("Daemon stopped");
    Ok(0)
}
