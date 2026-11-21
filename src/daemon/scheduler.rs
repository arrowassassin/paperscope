//! Scheduler for automatic indexing.
//!
//! Full reindex daily at 3 AM, incremental every 5 minutes.

use crate::daemon::ipc::DaemonRequest;
use anyhow::Result;
use chrono::Local;
use cron::Schedule;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{interval, sleep};

/// Scheduler for periodic indexing tasks.
pub struct Scheduler {
    full_cron: Schedule,
    incremental_interval: Duration,
    sender: mpsc::Sender<DaemonRequest>,
    /// Handle to the scheduler task for graceful shutdown.
    task_handle: Option<tokio::task::JoinHandle<()>>,
}

impl Scheduler {
    /// Create a new scheduler.
    ///
    /// - `full_cron_expr`: Cron expression for full reindex (default: "0 3 * * *" = daily 3 AM)
    /// - `incremental_interval`: Interval for incremental indexing (default: 5 minutes)
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
            task_handle: None,
        })
    }

    /// Create a scheduler with default settings (3 AM full, 5 min incremental).
    pub fn default(sender: mpsc::Sender<DaemonRequest>) -> Result<Self> {
        Self::new("0 3 * * *", Duration::from_secs(5 * 60), sender)
    }

    /// Start the scheduler.
    pub fn start(&mut self) {
        let full_cron = self.full_cron;
        let incremental_interval = self.incremental_interval;
        let sender = self.sender.clone();

        let handle = tokio::spawn(async move {
            Self::run_scheduler(full_cron, incremental_interval, sender).await;
        });

        self.task_handle = Some(handle);
    }

    /// Stop the scheduler gracefully.
    pub async fn stop(&mut self) {
        if let Some(handle) = self.task_handle.take() {
            handle.abort();
            let _ = handle.await;
        }
    }

    /// Trigger an immediate full reindex.
    pub fn trigger_full(&self) {
        let _ = self.sender.try_send(DaemonRequest::IndexRoots { roots: vec![] });
    }

    /// Trigger an immediate incremental index.
    pub fn trigger_incremental(&self) {
        // For now, incremental is the same as full reindex with current roots.
        // In a more sophisticated implementation, this would only index changed files.
        let _ = self.sender.try_send(DaemonRequest::IndexRoots { roots: vec![] });
    }

    async fn run_scheduler(
        full_cron: Schedule,
        incremental_interval: Duration,
        sender: mpsc::Sender<DaemonRequest>,
    ) {
        let mut incremental_timer = interval(incremental_interval);

        // Align incremental timer to the next interval boundary
        incremental_timer.tick().await;

        loop {
            // Calculate time until next full reindex
            let now = Local::now();
            let next_full = full_cron
                .after(&now)
                .next()
                .expect("cron schedule should have next occurrence");

            let duration_until_full = (next_full - now).to_std().unwrap_or(Duration::from_secs(86400));

            tokio::select! {
                // Full reindex at scheduled time
                _ = sleep(duration_until_full) => {
                    tracing::info!("Scheduler: triggering scheduled full reindex");
                    if sender.send(DaemonRequest::IndexRoots { roots: vec![] }).await.is_err() {
                        tracing::warn!("Scheduler: channel closed, stopping");
                        break;
                    }
                }
                // Incremental reindex at regular interval
                _ = incremental_timer.tick() => {
                    tracing::debug!("Scheduler: triggering incremental index");
                    if sender.send(DaemonRequest::IndexRoots { roots: vec![] }).await.is_err() {
                        tracing::warn!("Scheduler: channel closed, stopping");
                        break;
                    }
                }
            }
        }
    }
}

/// Wrapper for running the scheduler with a specific set of roots.
///
/// This variant accepts roots to index, allowing more control over what gets indexed.
#[derive(Clone)]
pub struct RootAwareScheduler {
    full_cron: Schedule,
    incremental_interval: Duration,
    sender: mpsc::Sender<DaemonRequest>,
    roots: Arc<tokio::sync::RwLock<Vec<std::path::PathBuf>>>,
    task_handle: Option<tokio::task::JoinHandle<()>>,
}

impl RootAwareScheduler {
    /// Create a new root-aware scheduler.
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
            roots: Arc::new(tokio::sync::RwLock::new(Vec::new())),
            task_handle: None,
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

    /// Start the scheduler.
    pub fn start(&mut self) {
        let full_cron = self.full_cron;
        let incremental_interval = self.incremental_interval;
        let sender = self.sender.clone();
        let roots = self.roots.clone();

        let handle = tokio::spawn(async move {
            Self::run_scheduler(full_cron, incremental_interval, sender, roots).await;
        });

        self.task_handle = Some(handle);
    }

    /// Stop the scheduler gracefully.
    pub async fn stop(&mut self) {
        if let Some(handle) = self.task_handle.take() {
            handle.abort();
            let _ = handle.await;
        }
    }

    /// Trigger an immediate full reindex with current roots.
    pub async fn trigger_full(&self) {
        let roots = self.roots.read().await.clone();
        let _ = self.sender.try_send(DaemonRequest::IndexRoots { roots });
    }

    /// Trigger an immediate incremental index with current roots.
    pub async fn trigger_incremental(&self) {
        let roots = self.roots.read().await.clone();
        let _ = self.sender.try_send(DaemonRequest::IndexRoots { roots });
    }

    async fn run_scheduler(
        full_cron: Schedule,
        incremental_interval: Duration,
        sender: mpsc::Sender<DaemonRequest>,
        roots: Arc<tokio::sync::RwLock<Vec<std::path::PathBuf>>>,
    ) {
        let mut incremental_timer = interval(incremental_interval);
        incremental_timer.tick().await;

        loop {
            let now = Local::now();
            let next_full = full_cron
                .after(&now)
                .next()
                .expect("cron schedule should have next occurrence");

            let duration_until_full = (next_full - now).to_std().unwrap_or(Duration::from_secs(86400));

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