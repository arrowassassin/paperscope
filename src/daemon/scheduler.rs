//! Scheduler for automatic indexing.
//!
//! Full reindex daily at 3 AM, incremental every 5 minutes.

use crate::daemon::ipc::DaemonRequest;
use anyhow::{Context, Result};
use chrono::Local;
use cron::Schedule;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::time::{interval, sleep};

/// Scheduler that periodically sends `IndexRoots` requests with the current
/// set of roots. Roots can be changed at runtime without restarting.
pub struct RootAwareScheduler {
    full_cron: Schedule,
    incremental_interval: Duration,
    sender: mpsc::Sender<DaemonRequest>,
    roots: Arc<RwLock<Vec<std::path::PathBuf>>>,
    task_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
}

impl RootAwareScheduler {
    /// Create a new root-aware scheduler.
    ///
    /// - `full_cron_expr`: Cron expression for full reindex (e.g. "0 0 3 * * *" = daily 3 AM)
    /// - `incremental_interval`: Interval for incremental indexing (e.g. 5 minutes)
    /// - `sender`: Channel to send indexing requests to the daemon
    pub fn new(
        full_cron_expr: &str,
        incremental_interval: Duration,
        sender: mpsc::Sender<DaemonRequest>,
    ) -> Result<Self> {
        let full_cron = Schedule::from_str(full_cron_expr)
            .with_context(|| format!("invalid cron expression: {full_cron_expr}"))?;

        Ok(Self {
            full_cron,
            incremental_interval,
            sender,
            roots: Arc::new(RwLock::new(Vec::new())),
            task_handle: Arc::new(Mutex::new(None)),
        })
    }

    /// Set the roots to index.
    pub async fn set_roots(&self, roots: Vec<std::path::PathBuf>) {
        *self.roots.write().await = roots;
    }

    /// Add a root.
    pub async fn add_root(&self, root: std::path::PathBuf) {
        self.roots.write().await.push(root);
    }

    /// Remove a root.
    pub async fn remove_root(&self, root: &std::path::Path) {
        self.roots.write().await.retain(|r| r != root);
    }

    /// Start the scheduler. No-op if already running.
    pub async fn start(&self) {
        let mut guard = self.task_handle.lock().await;
        if guard.is_some() {
            return;
        }

        let full_cron = self.full_cron.clone();
        let incremental_interval = self.incremental_interval;
        let sender = self.sender.clone();
        let roots = self.roots.clone();

        let handle = tokio::spawn(async move {
            Self::run_scheduler(full_cron, incremental_interval, sender, roots).await;
        });

        *guard = Some(handle);
    }

    /// Stop the scheduler gracefully.
    pub async fn stop(&self) {
        let mut guard = self.task_handle.lock().await;
        if let Some(handle) = guard.take() {
            handle.abort();
            let _ = handle.await;
        }
    }

    /// Trigger an immediate full reindex with current roots.
    pub async fn trigger_full(&self) {
        let roots = self.roots.read().await.clone();
        if let Err(e) = self.sender.try_send(DaemonRequest::IndexRoots { roots }) {
            tracing::warn!("Scheduler: channel full, dropping scheduled full reindex: {e:#}");
        }
    }

    /// Trigger an immediate incremental index with current roots.
    pub async fn trigger_incremental(&self) {
        let roots = self.roots.read().await.clone();
        if let Err(e) = self.sender.try_send(DaemonRequest::IndexRoots { roots }) {
            tracing::warn!(
                "Scheduler: channel full, dropping scheduled incremental reindex: {e:#}"
            );
        }
    }

    async fn run_scheduler(
        full_cron: Schedule,
        incremental_interval: Duration,
        sender: mpsc::Sender<DaemonRequest>,
        roots: Arc<RwLock<Vec<std::path::PathBuf>>>,
    ) {
        let mut incremental_timer = interval(incremental_interval);
        incremental_timer.tick().await;

        loop {
            let now = Local::now();
            let next_full = full_cron
                .after(&now)
                .next()
                .expect("cron schedule should have next occurrence");

            let duration_until_full = (next_full - now)
                .to_std()
                .unwrap_or(Duration::from_secs(86400));

            tokio::select! {
                _ = sleep(duration_until_full) => {
                    tracing::info!("Scheduler: triggering scheduled full reindex");
                    let roots = roots.read().await.clone();
                    if sender.send(DaemonRequest::IndexRoots { roots }).await.is_err() {
                        tracing::warn!("Scheduler: channel closed, stopping");
                        break;
                    }
                }
                _ = incremental_timer.tick() => {
                    tracing::debug!("Scheduler: triggering incremental index");
                    let roots = roots.read().await.clone();
                    if sender.send(DaemonRequest::IndexRoots { roots }).await.is_err() {
                        tracing::warn!("Scheduler: channel closed, stopping");
                        break;
                    }
                }
            }
        }
    }
}
