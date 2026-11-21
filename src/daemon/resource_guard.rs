//! Resource guard for pausing indexing based on system conditions.
//!
//! Pauses when:
//! - Battery < 20% and discharging (on laptops)
//! - CPU > 80% sustained over several consecutive checks
//! - Metered network connection (currently a conservative no-op; there is no
//!   reliable cross-platform API without extra dependencies)
//!
//! CPU usage comes from `sysinfo`. Battery is read directly from the OS:
//! Linux `/sys/class/power_supply/BAT*`, macOS `pmset -g batt`. Platforms
//! without battery information simply never pause for low battery.

use std::sync::Arc;
use std::time::Duration;
use sysinfo::System;
use tokio::sync::{mpsc, RwLock};
use tokio::time::interval;

/// Reason for pausing indexing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PauseReason {
    LowBattery,
    HighCpu,
    MeteredNetwork,
    Resumed,
}

/// Resource guard configuration.
#[derive(Debug, Clone)]
pub struct ResourceGuardConfig {
    /// Battery percentage threshold (pause below this)
    pub battery_threshold: u8,
    /// CPU usage threshold (pause above this)
    pub cpu_threshold: u8,
    /// Check interval
    pub check_interval: Duration,
    /// Number of consecutive checks above threshold before pausing
    pub cpu_consecutive_checks: u32,
}

impl Default for ResourceGuardConfig {
    fn default() -> Self {
        Self {
            battery_threshold: 20,
            cpu_threshold: 80,
            check_interval: Duration::from_secs(30),
            cpu_consecutive_checks: 3,
        }
    }
}

/// Battery state snapshot: percentage (0-100) and whether charging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BatteryStatus {
    percent: u8,
    charging: bool,
}

/// Read battery status from the OS, if a battery exists.
fn battery_status() -> Option<BatteryStatus> {
    #[cfg(target_os = "linux")]
    {
        let base = std::path::Path::new("/sys/class/power_supply");
        let entries = std::fs::read_dir(base).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            let is_battery = path
                .file_name()
                .map(|n| n.to_string_lossy().starts_with("BAT"))
                .unwrap_or(false);
            if !is_battery {
                continue;
            }
            let percent: u8 = std::fs::read_to_string(path.join("capacity"))
                .ok()?
                .trim()
                .parse()
                .ok()?;
            let status = std::fs::read_to_string(path.join("status")).unwrap_or_default();
            let charging = status.trim() == "Charging" || status.trim() == "Full";
            return Some(BatteryStatus { percent, charging });
        }
        None
    }

    #[cfg(target_os = "macos")]
    {
        // `pmset -g batt` output looks like:
        //  -InternalBattery-0 (id=...)	100%; charged; 0:00 remaining
        //  -InternalBattery-0 (id=...)	85%; discharging; 4:12 remaining
        let output = std::process::Command::new("pmset")
            .args(["-g", "batt"])
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines() {
            let Some(percent_part) = line.split('%').next() else {
                continue;
            };
            let Some(digits) = percent_part
                .rsplit(|c: char| !c.is_ascii_digit())
                .find(|s| !s.is_empty())
            else {
                continue;
            };
            let Ok(percent) = digits.parse::<u8>() else {
                continue;
            };
            let charging = line.contains("charging") && !line.contains("discharging")
                || line.contains("charged");
            return Some(BatteryStatus { percent, charging });
        }
        None
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        None
    }
}

/// Resource guard for monitoring system resources.
pub struct ResourceGuard {
    /// Configuration (public so callers can tune thresholds before `with_config`).
    pub config: ResourceGuardConfig,
    system: Arc<RwLock<System>>,
    /// Track consecutive high CPU readings
    high_cpu_count: Arc<RwLock<u32>>,
    /// Current pause state
    paused: Arc<RwLock<Option<PauseReason>>>,
}

impl ResourceGuard {
    /// Create a new resource guard with default configuration.
    pub fn new() -> Self {
        Self::with_config(ResourceGuardConfig::default())
    }

    /// Create a new resource guard with custom configuration.
    pub fn with_config(config: ResourceGuardConfig) -> Self {
        let mut system = System::new();
        system.refresh_cpu_usage();

        Self {
            config,
            system: Arc::new(RwLock::new(system)),
            high_cpu_count: Arc::new(RwLock::new(0)),
            paused: Arc::new(RwLock::new(None)),
        }
    }

    /// Evaluate the current resource state and update the pause state.
    ///
    /// Returns the pause reason if indexing should pause, `None` otherwise.
    async fn evaluate(&self) -> Option<PauseReason> {
        // CPU: average across all cores. sysinfo needs two samples over time
        // for meaningful numbers; the monitoring loop provides that cadence.
        let cpu_usage = {
            let mut system = self.system.write().await;
            system.refresh_cpu_usage();
            let cpus = system.cpus();
            if cpus.is_empty() {
                0.0
            } else {
                cpus.iter().map(|c| c.cpu_usage()).sum::<f32>() / cpus.len() as f32
            }
        };

        // Battery
        if let Some(battery) = battery_status() {
            if battery.percent < self.config.battery_threshold && !battery.charging {
                *self.paused.write().await = Some(PauseReason::LowBattery);
                return Some(PauseReason::LowBattery);
            }
        }

        // CPU
        let mut high_cpu_count = self.high_cpu_count.write().await;
        if cpu_usage > self.config.cpu_threshold as f32 {
            *high_cpu_count += 1;
            if *high_cpu_count >= self.config.cpu_consecutive_checks {
                *self.paused.write().await = Some(PauseReason::HighCpu);
                return Some(PauseReason::HighCpu);
            }
        } else {
            *high_cpu_count = 0;
        }

        // Metered network: conservative no-op (see module docs).

        // Conditions are OK: clear any previous pause state.
        *self.paused.write().await = None;
        None
    }

    /// Check if indexing should be paused based on current resource state.
    pub async fn should_pause(&self) -> bool {
        self.evaluate().await.is_some()
    }

    /// Get the current pause reason, if any.
    pub async fn pause_reason(&self) -> Option<PauseReason> {
        *self.paused.read().await
    }

    /// Start monitoring resources and send pause/resume signals.
    ///
    /// Runs until the task is aborted. Sends a `PauseReason` whenever the
    /// pause state changes: the specific reason on pause, `Resumed` on resume.
    pub fn start_monitoring(&self, sender: mpsc::Sender<PauseReason>) {
        let config = self.config.clone();
        let system = self.system.clone();
        let high_cpu_count = self.high_cpu_count.clone();
        let paused = self.paused.clone();

        tokio::spawn(async move {
            let mut timer = interval(config.check_interval);
            loop {
                timer.tick().await;

                let cpu_usage = {
                    let mut system = system.write().await;
                    system.refresh_cpu_usage();
                    let cpus = system.cpus();
                    if cpus.is_empty() {
                        0.0
                    } else {
                        cpus.iter().map(|c| c.cpu_usage()).sum::<f32>() / cpus.len() as f32
                    }
                };

                let mut reason = None;

                if let Some(battery) = battery_status() {
                    if battery.percent < config.battery_threshold && !battery.charging {
                        reason = Some(PauseReason::LowBattery);
                    }
                }

                if reason.is_none() {
                    let mut count = high_cpu_count.write().await;
                    if cpu_usage > config.cpu_threshold as f32 {
                        *count += 1;
                        if *count >= config.cpu_consecutive_checks {
                            reason = Some(PauseReason::HighCpu);
                        }
                    } else {
                        *count = 0;
                    }
                }

                let mut paused = paused.write().await;
                match reason {
                    Some(r) => {
                        if *paused != Some(r) {
                            *paused = Some(r);
                            let _ = sender.send(r).await;
                        }
                    }
                    None => {
                        if paused.is_some() {
                            *paused = None;
                            let _ = sender.send(PauseReason::Resumed).await;
                        }
                    }
                }
            }
        });
    }

    /// Force a resource check and return whether paused.
    pub async fn check_now(&self) -> bool {
        self.should_pause().await
    }
}

impl Default for ResourceGuard {
    fn default() -> Self {
        Self::new()
    }
}
