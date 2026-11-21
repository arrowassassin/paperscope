//! File system watcher with debouncing.
//!
//! Uses `notify` to watch for file changes, debounces events (default 250ms),
//! and batches them into a single `IndexRoots` request.

use crate::daemon::ipc::DaemonRequest;
use anyhow::Result;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher as NotifyWatcher};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use tokio::time::interval;

/// File system watcher with debouncing.
pub struct Watcher {
    roots: Arc<RwLock<HashSet<PathBuf>>>,
    debounce_ms: u64,
    sender: mpsc::Sender<DaemonRequest>,
    /// Internal notify watcher
    notify_watcher: Option<RecommendedWatcher>,
    /// Handle to the debounce task
    debounce_handle: Option<tokio::task::JoinHandle<()>>,
}

impl Watcher {
    /// Create a new watcher.
    ///
    /// - `debounce_ms`: Debounce time in milliseconds (default: 250ms)
    /// - `sender`: Channel to send indexing requests to the daemon
    pub fn new(debounce_ms: u64, sender: mpsc::Sender<DaemonRequest>) -> Self {
        Self {
            roots: Arc::new(RwLock::new(HashSet::new())),
            debounce_ms,
            sender,
            notify_watcher: None,
            debounce_handle: None,
        }
    }

    /// Create a watcher with default settings (250ms debounce).
    pub fn default(sender: mpsc::Sender<DaemonRequest>) -> Self {
        Self::new(250, sender)
    }

    /// Start the watcher.
    pub async fn start(&mut self) -> Result<()> {
        let (event_tx, event_rx) = mpsc::channel::<Vec<PathBuf>>(100);
        let roots = self.roots.clone();
        let debounce_ms = self.debounce_ms;

        // Create the notify watcher
        let mut watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    Self::handle_event(event, roots.clone(), event_tx.clone());
                }
            },
            notify::Config::default(),
        )?;

        // Watch current roots
        let current_roots = self.roots.read().await.clone();
        for root in &current_roots {
            if root.exists() {
                watcher.watch(root, RecursiveMode::Recursive)?;
            }
        }

        self.notify_watcher = Some(watcher);

        // Start debounce task
        let sender = self.sender.clone();
        let debounce_handle = tokio::spawn(async move {
            Self::debounce_loop(event_rx, sender, Duration::from_millis(debounce_ms)).await;
        });
        self.debounce_handle = Some(debounce_handle);

        Ok(())
    }

    /// Stop the watcher gracefully.
    pub async fn stop(&mut self) {
        if let Some(handle) = self.debounce_handle.take() {
            handle.abort();
            let _ = handle.await;
        }
        if let Some(mut watcher) = self.notify_watcher.take() {
            // notify has no unwatch_all: remove each watched root explicitly.
            for root in self.roots.read().await.iter() {
                let _ = watcher.unwatch(root);
            }
        }
    }

    /// Add a root to watch.
    pub async fn add_root(&mut self, path: PathBuf) -> Result<()> {
        self.roots.write().await.insert(path.clone());

        // If watcher is already running, add watch for new root
        if let Some(watcher) = &mut self.notify_watcher {
            if path.exists() {
                watcher.watch(&path, RecursiveMode::Recursive)?;
            }
        }

        Ok(())
    }

    /// Remove a root from watching.
    pub async fn remove_root(&mut self, path: &Path) -> Result<()> {
        self.roots.write().await.remove(path);

        // If watcher is already running, remove watch
        if let Some(watcher) = &mut self.notify_watcher {
            let _ = watcher.unwatch(path);
        }

        Ok(())
    }

    /// Get current watched roots.
    pub async fn roots(&self) -> Vec<PathBuf> {
        self.roots.read().await.iter().cloned().collect()
    }

    /// Handle a notify event and send relevant paths to the event channel.
    fn handle_event(
        event: Event,
        roots: Arc<RwLock<HashSet<PathBuf>>>,
        event_tx: mpsc::Sender<Vec<PathBuf>>,
    ) {
        // Only care about create, modify, remove events
        let relevant = matches!(
            event.kind,
            EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
        );

        if !relevant {
            return;
        }

        // Filter paths to only those under watched roots
        let current_roots = match roots.try_read() {
            Ok(r) => r.clone(),
            Err(_) => return, // Skip if locked
        };

        let relevant_paths: Vec<PathBuf> = event
            .paths
            .into_iter()
            .filter(|p| current_roots.iter().any(|root| p.starts_with(root)))
            .collect();

        if !relevant_paths.is_empty() {
            // `try_send` is non-blocking because `handle_event` runs in the
            // notify callback thread. Log when the channel is full so dropped
            // events are visible instead of silent (IL).
            let count = relevant_paths.len();
            if event_tx.try_send(relevant_paths).is_err() {
                tracing::warn!(
                    "Watcher: event channel full, dropped {} events",
                    count
                );
            }
        }
    }

    /// Debounce loop: collect events for debounce_ms, then send batched IndexRoots.
    async fn debounce_loop(
        mut event_rx: mpsc::Receiver<Vec<PathBuf>>,
        sender: mpsc::Sender<DaemonRequest>,
        debounce_duration: Duration,
    ) {
        let mut pending_paths = Vec::new();
        let mut debounce_timer = interval(debounce_duration);
        debounce_timer.tick().await; // Align to interval

        loop {
            tokio::select! {
                // Collect events
                Some(paths) = event_rx.recv() => {
                    pending_paths.extend(paths);
                }
                // Debounce timer fired - send batched request
                _ = debounce_timer.tick() => {
                    if !pending_paths.is_empty() {
                        // Deduplicate paths
                        pending_paths.sort();
                        pending_paths.dedup();

                        tracing::debug!("Watcher: triggering index for {} changed paths", pending_paths.len());

                        let request = DaemonRequest::IndexRoots { roots: pending_paths.clone() };
                        if sender.send(request).await.is_err() {
                            tracing::warn!("Watcher: channel closed, stopping");
                            break;
                        }

                        pending_paths.clear();
                    }
                }
            }
        }
    }
}
