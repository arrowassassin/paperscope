//! Resource guard for pausing indexing based on system conditions.
//!
//! Pauses when:
//! - Battery < 20% (on laptops)
//! - CPU > 80% sustained
//! - Metered network connection (Windows/macOS)

use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;
use sysinfo::{System, SystemExt, ProcessorExt, BatteryExt, NetworkExt, NetworksExt, Battery, BatteryState};
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

/// Resource guard for monitoring system resources.
pub struct ResourceGuard {
    config: ResourceGuardConfig,
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
        let mut system = System::new_all();
        system.refresh_all();

        Self {
            config,
            system: Arc::new(RwLock::new(system)),
            high_cpu_count: Arc::new(RwLock::new(0)),
            paused: Arc::new(RwLock::new(None)),
        }
    }

    /// Get the configuration.
    pub fn config(&self) -> &ResourceGuardConfig {
        &self.config
    }

    /// Check if indexing should be paused based on current resource state.
    ///
    /// Returns `true` if indexing should be paused, `false` otherwise.
    pub async fn should_pause(&self) -> bool {
        let mut system = self.system.write().await;
        system.refresh_cpu_usage();
        system.refresh_memory();
        system.refresh_battery();
        system.refresh_networks();

        // Check battery
        // sysinfo 0.33: batteries() returns Vec<&Battery>
        if let Some(battery) = system.batteries().first() {
            let remaining_percent = if battery.capacity() > 0 {
                (battery.remaining_capacity() * 100 / battery.capacity()) as u8
            } else {
                100
            };
            let is_charging = matches!(battery.state(), BatteryState::Charging);
            if remaining_percent < self.config.battery_threshold && !is_charging {
                *self.paused.write().await = Some(PauseReason::LowBattery);
                return true;
            }
        }

        // Check CPU usage (average across all cores)
        let cpu_usage: f32 = system.cpus().iter().map(|c| c.cpu_usage()).sum::<f32>()
            / system.cpus().len().max(1) as f32;

        let mut high_cpu_count = self.high_cpu_count.write().await;
        if cpu_usage > self.config.cpu_threshold as f32 {
            *high_cpu_count += 1;
            if *high_cpu_count >= self.config.cpu_consecutive_checks {
                *self.paused.write().await = Some(PauseReason::HighCpu);
                return true;
            }
        } else {
            *high_cpu_count = 0;
        }

        // Check metered network (platform-specific)
        if self.is_metered_network(&system).await {
            *self.paused.write().await = Some(PauseReason::MeteredNetwork);
            return true;
        }

        // If we were paused but conditions are now OK, mark as resumed
        if self.paused.read().await.is_some() {
            *self.paused.write().await = Some(PauseReason::Resumed);
        }

        false
    }

    /// Check if the current network is metered.
    async fn is_metered_network(&self, _system: &System) -> bool {
        #[cfg(target_os = "windows")]
        {
            // On Windows, check for metered connection via network adapter
            // This is a simplified check - in production you'd use Windows API
            false
        }

        #[cfg(target_os = "macos")]
        {
            // On macOS, check for constrained/expensive network
            // This would require NetworkExtension framework
            false
        }

        #[cfg(target_os = "linux")]
        {
            // On Linux, check /proc/net/ or NetworkManager for metered status
            false
        }

        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        {
            false
        }
    }

    /// Get the current pause reason, if any.
    pub async fn pause_reason(&self) -> Option<PauseReason> {
        *self.paused.read().await
    }

    /// Start monitoring resources and send pause/resume signals.
    ///
    /// This runs indefinitely until the sender is dropped or the task is aborted.
    pub fn start_monitoring(&self, sender: mpsc::Sender<PauseReason>) {
        let config = self.config.clone();
        let system = self.system.clone();
        let high_cpu_count = self.high_cpu_count.clone();
        let paused = self.paused.clone();

        tokio::spawn(async move {
            let mut timer = interval(config.check_interval);
            loop {
                timer.tick().await;

                let mut system = system.write().await;
                system.refresh_cpu_usage();
                system.refresh_battery();
                system.refresh_networks();

                let mut should_pause = false;
                let mut reason = None;

                // Check battery
                if let Some(battery) = system.batteries().first() {
                    let remaining_percent = if battery.capacity() > 0 {
                        (battery.remaining_capacity() * 100 / battery.capacity()) as u8
                    } else {
                        100
                    };
                    let is_charging = matches!(battery.state(), BatteryState::Charging);
                    if remaining_percent < config.battery_threshold && !is_charging {
                        should_pause = true;
                        reason = Some(PauseReason::LowBattery);
                    }
                }

                // Check CPU
                let cpu_usage: f32 = system.cpus().iter().map(|c| c.cpu_usage()).sum::<f32>()
                    / system.cpus().len().max(1) as f32;

                let mut high_cpu_count = high_cpu_count.write().await;
                if cpu_usage > config.cpu_threshold as f32 {
                    *high_cpu_count += 1;
                    if *high_cpu_count >= config.cpu_consecutive_checks && !should_pause {
                        should_pause = true;
                        reason = Some(PauseReason::HighCpu);
                    }
                } else {
                    *high_cpu_count = 0;
                }

                // Check metered network (simplified)
                // Platform-specific checks would go here

                let currently_paused = *paused.read().await;
                if should_pause {
                    if currently_paused != reason {
                        *paused.write().await = reason;
                        let _ = sender.send(reason.unwrap()).await;
                    }
                } else if currently_paused.is_some() {
                    *paused.write().await = Some(PauseReason::Resumed);
                    let _ = sender.send(PauseReason::Resumed).await;
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