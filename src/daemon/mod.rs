//! Daemon core: IPC server, scheduler, watcher, resource guard, and auto-discovery.

pub mod auto_discover;
pub mod ipc;
pub mod resource_guard;
pub mod scheduler;
pub mod watcher;

use crate::cli::Workspace;
use crate::core::index::{Connection, Indexer, init_db};
use crate::daemon::auto_discover::discover_default_roots;
use crate::daemon::ipc::{DaemonHandler, DaemonRequest, DaemonResponse, run_ipc_server};
use crate::daemon::resource_guard::{PauseReason, ResourceGuard};
use crate::daemon::scheduler::RootAwareScheduler;
use crate::daemon::watcher::Watcher;
use anyhow::Result;
use clap::Args;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio::signal;

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

/// Shared daemon state (does NOT contain Indexer — it's not Send/Sync).
/// Instead we create fresh connections per request.
struct DaemonState {
    roots: Arc<RwLock<Vec<PathBuf>>>,
    paused: Arc<RwLock<bool>>,
    workspace: Workspace,
    embedder: Arc<tokio::sync::RwLock<Option<std::sync::Arc<dyn crate::core::search::QueryEmbedder + Send + Sync>>>>,
}

/// Daemon request handler.
struct DaemonHandlerImpl {
    state: Arc<DaemonState>,
    scheduler: Arc<RootAwareScheduler>,
    watcher: Arc<RwLock<Option<Watcher>>>,
    resource_guard: Arc<ResourceGuard>,
    sender: mpsc::Sender<DaemonRequest>,
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

                // Check if paused
                if *self.state.paused.read().await {
                    return DaemonResponse::Error {
                        msg: "Indexing paused by resource guard".to_string(),
                    };
                }

                // Get embedder
                let embedder = self.state.embedder.read().await.clone();

                // Create fresh indexer for this request (Indexer is not Send/Sync)
                let conn = match self.state.workspace.open_index() {
                    Ok(c) => c,
                    Err(e) => {
                        return DaemonResponse::Error {
                            msg: format!("Failed to open index: {e}"),
                        };
                    }
                };
                let indexer = match Indexer::new(conn, embedder) {
                    Ok(i) => i,
                    Err(e) => {
                        return DaemonResponse::Error {
                            msg: format!("Failed to create indexer: {e}"),
                        };
                    }
                };

                // Run indexing
                match indexer.index_roots(&roots_to_index) {
                    Ok(stats) => DaemonResponse::IndexRoots { stats },
                    Err(e) => DaemonResponse::Error {
                        msg: format!("Indexing failed: {e}"),
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

                    // Add to watcher if running
                    if let Some(watcher) = self.watcher.read().await.as_mut() {
                        let _ = watcher.add_root(path).await;
                    }

                    // Update scheduler
                    self.scheduler.add_root(path).await;
                }
                DaemonResponse::Ok
            }
            DaemonRequest::RemoveRoot { path } => {
                let mut roots = self.state.roots.write().await;
                roots.retain(|r| r != &path);

                // Remove from watcher if running
                if let Some(watcher) = self.watcher.read().await.as_mut() {
                    let _ = watcher.remove_root(&path).await;
                }

                // Update scheduler
                self.scheduler.remove_root(&path).await;
                DaemonResponse::Ok
            }
            DaemonRequest::GetStatus => {
                let indexed_files = self.count_indexed_files().await;
                let queued = 0; // TODO: track queue depth
                let paused = *self.state.paused.read().await;
                let model = self.get_embedding_model().await;

                DaemonResponse::Status {
                    indexed_files,
                    queued,
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
            DaemonRequest::Shutdown => {
                DaemonResponse::Ok
            }
        }
    }
}

impl DaemonHandlerImpl {
    async fn count_indexed_files(&self) -> usize {
        let conn = match self.state.workspace.open_index() {
            Ok(c) => c,
            Err(_) => return 0,
        };
        conn.query_i64("SELECT COUNT(DISTINCT source_path) FROM chunks")
            .unwrap_or(0) as usize
    }

    async fn get_embedding_model(&self) -> Option<String> {
        let conn = match self.state.workspace.open_index() {
            Ok(c) => c,
            Err(_) => return None,
        };
        conn.stored_embedding_model().ok().flatten()
    }
}

/// Run the daemon.
pub async fn run_daemon(args: DaemonArgs) -> Result<i32> {
    // Determine socket path
    let socket_path = args.socket.unwrap_or_else(|| {
        Workspace::new()
            .paths
            .base_dir()
            .join("daemon.sock")
    });

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

    // Initialize workspace and embedder (but NOT indexer — it's not Send/Sync)
    let workspace = Workspace::new();
    let embedder = workspace.embedder();

    // Create channels
    let (request_tx, mut request_rx) = mpsc::channel::<DaemonRequest>(100);

    // Create shared state
    let state = Arc::new(DaemonState {
        roots: Arc::new(RwLock::new(roots.clone())),
        paused: Arc::new(RwLock::new(false)),
        workspace,
        embedder: Arc::new(tokio::sync::RwLock::new(embedder.map(|e| std::sync::Arc::new(e) as std::sync::Arc<dyn crate::core::search::QueryEmbedder + Send + Sync>))),
    });

    // Create scheduler
    let full_cron = args.full_cron.unwrap_or_else(|| "0 3 * * *".to_string());
    let incremental_interval = args.incremental_interval.map(std::time::Duration::from_secs).unwrap_or_else(|| std::time::Duration::from_secs(5 * 60));
    let mut scheduler = RootAwareScheduler::new(&full_cron, incremental_interval, request_tx.clone())?;
    scheduler.set_roots(roots.clone()).await;
    let scheduler = Arc::new(scheduler);

    // Create watcher
    let debounce_ms = args.debounce_ms.unwrap_or(250);
    let mut watcher = Watcher::new(debounce_ms, request_tx.clone());
    for root in &roots {
        watcher.add_root(root.clone()).await?;
    }
    let watcher = Arc::new(RwLock::new(Some(watcher)));

    // Create resource guard
    let mut resource_config = ResourceGuard::default().config;
    if let Some(b) = args.battery_threshold {
        resource_config.battery_threshold = b;
    }
    if let Some(c) = args.cpu_threshold {
        resource_config.cpu_threshold = c;
    }
    let resource_guard = Arc::new(ResourceGuard::with_config(resource_config));

    // Create handler
    let handler = Arc::new(DaemonHandlerImpl {
        state: state.clone(),
        scheduler: scheduler.clone(),
        watcher: watcher.clone(),
        resource_guard: resource_guard.clone(),
        sender: request_tx.clone(),
    });

    // Start components
    if !args.no_schedule {
        let mut scheduler_mut = (*scheduler).clone();
        scheduler_mut.start();
    }

    if !args.no_watch {
        if let Some(w) = watcher.write().await.as_mut() {
            w.start()?;
        }
    }

    if !args.no_resource_guard {
        let (pause_tx, mut pause_rx) = mpsc::channel::<PauseReason>(10);
        resource_guard.start_monitoring(pause_tx);
        let state_paused = state.paused.clone();
        tokio::spawn(async move {
            while let Some(reason) = pause_rx.recv().await {
                match reason {
                    PauseReason::Resumed => {
                        *state_paused.write().await = false;
                        tracing::info!("Resource guard: resuming indexing");
                    }
                    _ => {
                        *state_paused.write().await = true;
                        tracing::warn!("Resource guard: pausing indexing due to {:?}", reason);
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

    // Run until shutdown signal or shutdown request
    tokio::select! {
        _ = shutdown => {
            tracing::info!("Shutting down daemon...");
        }
        _ = async {
            while let Some(request) = request_rx.recv().await {
                if matches!(request, DaemonRequest::Shutdown) {
                    break;
                }
            }
        } => {
            tracing::info!("Shutdown requested via IPC...");
        }
    }

    // Graceful shutdown
    if !args.no_schedule {
        // scheduler.stop().await; // Need mutable access
    }

    if !args.no_watch {
        if let Some(w) = watcher.write().await.as_mut() {
            w.stop().await;
        }
    }

    ipc_handle.abort();
    let _ = ipc_handle.await;

    // Clean up socket file
    let _ = tokio::fs::remove_file(&socket_path).await;

    tracing::info!("Daemon stopped");
    Ok(0)
}