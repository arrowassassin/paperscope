//! Shared auto-update contract: opt-out check, version order, `update.state`.

use super::config::Config;
use super::remote_config::write_atomic;
use serde::{Deserialize, Serialize};
use std::io;
use std::path::Path;

const STATE_FILE: &str = "update.state";

/// False when `PAPER_NO_AUTO_UPDATE` is set (non-empty, not "0") or `auto_update = false`.
pub fn auto_update_enabled(config: &Config) -> bool {
    let env_off = std::env::var("PAPER_NO_AUTO_UPDATE").is_ok_and(|v| !v.is_empty() && v != "0");
    !env_off && config.auto_update != Some(false)
}

/// Numeric comparison of `major.minor.patch`; a pre-release suffix sorts low.
pub fn version_newer(candidate: &str, current: &str) -> bool {
    let parse = |value: &str| -> (u64, u64, u64, bool) {
        let (core, pre) = match value.split_once('-') {
            Some((core, _)) => (core, true),
            None => (value, false),
        };
        let mut parts = core.split('.').map(|part| part.parse::<u64>().unwrap_or(0));
        (
            parts.next().unwrap_or(0),
            parts.next().unwrap_or(0),
            parts.next().unwrap_or(0),
            pre,
        )
    };
    let (a, b) = (parse(candidate), parse(current));
    (a.0, a.1, a.2) > (b.0, b.1, b.2) || ((a.0, a.1, a.2) == (b.0, b.1, b.2) && b.3 && !a.3)
}

/// Last auto-update outcome, written by the daemon and read by doctor, CLI and MCP.
/// `outcome` is "updated" | "up_to_date" | "unavailable" | "failed" | "opted_out" | "offline".
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UpdateState {
    pub outcome: String,
    pub approved: Option<String>,
    pub method: String,
    #[serde(default)]
    pub manual_command: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
    /// RFC 3339 timestamp.
    pub at: String,
}

/// Best effort: missing or garbled means `None`.
pub fn read_update_state(dir: &Path) -> Option<UpdateState> {
    toml::from_str(&std::fs::read_to_string(dir.join(STATE_FILE)).ok()?).ok()
}

pub fn write_update_state(dir: &Path, s: &UpdateState) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let body = toml::to_string(s).map_err(io::Error::other)?;
    write_atomic(dir, STATE_FILE, body.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_newer_orders_semver() {
        assert!(version_newer("0.2.10", "0.2.9"));
        assert!(version_newer("0.3.0", "0.2.99"));
        assert!(!version_newer("0.2.7", "0.2.7"));
        assert!(!version_newer("0.2.6", "0.2.7"));
        assert!(version_newer("0.2.7", "0.2.7-beta.1"));
    }

    // One test: it mutates a process-wide env var.
    #[test]
    fn auto_update_enabled_respects_env_and_config() {
        let on = Config::default();
        let off = Config {
            auto_update: Some(false),
            ..Config::default()
        };
        let explicit = Config {
            auto_update: Some(true),
            ..Config::default()
        };
        std::env::remove_var("PAPER_NO_AUTO_UPDATE");
        assert!(auto_update_enabled(&on));
        assert!(auto_update_enabled(&explicit));
        assert!(!auto_update_enabled(&off));
        for (v, enabled) in [("1", false), ("yes", false), ("0", true), ("", true)] {
            std::env::set_var("PAPER_NO_AUTO_UPDATE", v);
            assert_eq!(auto_update_enabled(&on), enabled, "env={v:?}");
        }
        std::env::remove_var("PAPER_NO_AUTO_UPDATE");
    }

    #[test]
    fn update_state_round_trips_and_garbage_is_none() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_update_state(dir.path()), None);
        let st = UpdateState {
            outcome: "failed".into(),
            approved: Some("0.4.2".into()),
            method: "npm".into(),
            manual_command: Some("npm install -g paperscope@0.4.2".into()),
            detail: Some("EACCES".into()),
            at: "2026-11-26T10:00:00+00:00".into(),
        };
        write_update_state(dir.path(), &st).unwrap();
        assert_eq!(read_update_state(dir.path()), Some(st));
        std::fs::write(dir.path().join(STATE_FILE), "not [ toml").unwrap();
        assert_eq!(read_update_state(dir.path()), None);
    }
}
