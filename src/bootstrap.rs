//! First-run automation.
//!
//! The installer cannot set Paper up: `install.sh`, `cargo install` and the
//! Claude plugin have no common post-install hook, and an installer should not
//! start a long-lived service. So the work happens here, on the first ordinary
//! command, and only once.
//!
//! Effects, in order:
//!   1. register the background indexer as a user service (launchd/systemd), or
//!      fall back to a detached daemon for this login session;
//!   2. the daemon downloads the default embedding model in the background;
//!   3. the daemon indexes the auto-discovered roots immediately (instead of
//!      waiting for the first five-minute scheduler tick).
//!
//! Escape hatches: `PAPER_NO_AUTOSTART=1` (never), `PAPER_OFFLINE=1` (no
//! network at all), `PAPER_FORCE_BOOTSTRAP=1` (run even when `PAPER_HOME` is
//! set), and `paper daemon-control uninstall` (turn it off afterwards).
//!
//! `CI` suppresses it too: a build machine must never index its own files.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::core::paths::Paths;

/// Bumped when the state file layout changes; an older file is ignored.
const STATE_VERSION: &str = "1";
/// Do not retry a failed service install more often than this.
const RETRY_AFTER: Duration = Duration::from_secs(60 * 60);
/// A lock older than this belongs to a crashed run and may be taken over.
const LOCK_STALE: Duration = Duration::from_secs(15 * 60);

const STATE_FILE: &str = "bootstrap.state";
const LOCK_FILE: &str = "bootstrap.lock";

/// Commands that must not trigger onboarding: they either manage the daemon
/// themselves, are diagnostics, or are explicitly user-driven work.
const SKIP_COMMANDS: &[&str] = &[
    "bootstrap",
    "daemon",
    "daemon-control",
    "doctor",
    "models",
    "index",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ServiceState {
    /// Never attempted.
    Pending,
    /// Service installed, or a session daemon was started successfully.
    Installed,
    /// Every attempt failed; retried later, but not on every command.
    Unavailable,
}

#[derive(Debug)]
struct State {
    service: ServiceState,
    updated: u64,
}

impl Default for State {
    fn default() -> Self {
        Self {
            service: ServiceState::Pending,
            updated: 0,
        }
    }
}

impl State {
    fn load(path: &Path) -> Self {
        let Ok(text) = fs::read_to_string(path) else {
            return Self::default();
        };
        if !text
            .lines()
            .any(|line| line.trim() == format!("version={STATE_VERSION}"))
        {
            return Self::default();
        }
        let mut state = Self::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            match key.trim() {
                "service" => {
                    state.service = match value.trim() {
                        "installed" => ServiceState::Installed,
                        "unavailable" => ServiceState::Unavailable,
                        _ => ServiceState::Pending,
                    }
                }
                "updated" => state.updated = value.trim().parse().unwrap_or(0),
                _ => {}
            }
        }
        state
    }

    fn save(&self, path: &Path) -> io::Result<()> {
        let contents = format!(
            "version={STATE_VERSION}\nservice={}\nupdated={}\n",
            match self.service {
                ServiceState::Pending => "pending",
                ServiceState::Installed => "installed",
                ServiceState::Unavailable => "unavailable",
            },
            self.updated
        );
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, contents)?;
        fs::rename(&tmp, path)
    }

    fn should_retry(&self, now: u64) -> bool {
        match self.service {
            ServiceState::Installed => false,
            ServiceState::Pending => true,
            ServiceState::Unavailable => now.saturating_sub(self.updated) >= RETRY_AFTER.as_secs(),
        }
    }
}

/// Environment switches, read once so the decision is testable without touching
/// the process environment.
#[derive(Clone, Copy, Debug, Default)]
struct Switches {
    offline: bool,
    no_autostart: bool,
    ci: bool,
    portable: bool,
    force: bool,
}

impl Switches {
    fn from_process() -> Self {
        Self {
            offline: present("PAPER_OFFLINE"),
            no_autostart: present("PAPER_NO_AUTOSTART"),
            ci: present("CI"),
            portable: present("PAPER_HOME"),
            force: present("PAPER_FORCE_BOOTSTRAP"),
        }
    }
}

fn present(name: &str) -> bool {
    match std::env::var(name) {
        Ok(value) => !value.is_empty() && value != "0",
        Err(_) => false,
    }
}

fn should_run(command: &str, switches: &Switches) -> bool {
    if switches.offline || switches.no_autostart || switches.ci {
        return false;
    }
    // `PAPER_HOME` marks tests and portable installs; never write services there.
    if switches.portable && !switches.force {
        return false;
    }
    !SKIP_COMMANDS.contains(&command)
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Guards the one-time setup against parallel invocations (an editor can start
/// several MCP sessions at once, each calling `paper serve`).
struct BootstrapLock {
    path: PathBuf,
}

impl BootstrapLock {
    fn acquire(path: &Path) -> Option<Self> {
        if fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .is_ok()
        {
            return Some(Self {
                path: path.to_path_buf(),
            });
        }
        let stale = fs::metadata(path)
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .map(|age| age > LOCK_STALE)
            .unwrap_or(true);
        if !stale {
            return None;
        }
        let _ = fs::remove_file(path);
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .ok()
            .map(|_| Self {
                path: path.to_path_buf(),
            })
    }
}

impl Drop for BootstrapLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Called from `main` before dispatching `command`. Never fails the command and
/// never blocks on the network: the daemon does the slow work.
pub async fn maybe_autostart(command: &str) {
    let switches = Switches::from_process();
    if !should_run(command, &switches) {
        return;
    }

    let paths = Paths::new();
    if paths.ensure_dirs().is_err() {
        return;
    }

    let state_path = paths.base_dir().join(STATE_FILE);
    let lock_path = paths.base_dir().join(LOCK_FILE);

    let state = State::load(&state_path);
    if !state.should_retry(now_unix()) {
        return;
    }

    let Some(_lock) = BootstrapLock::acquire(&lock_path) else {
        return;
    };

    // Another process may have finished while we waited.
    let state = State::load(&state_path);
    if !state.should_retry(now_unix()) {
        return;
    }

    eprintln!(
        "paper: first run — setting up background indexing for your documents \
         (disable with PAPER_NO_AUTOSTART=1)"
    );

    let next = match crate::cli::daemon::ensure_service() {
        Ok(()) => State {
            service: ServiceState::Installed,
            updated: now_unix(),
        },
        Err(error) => {
            tracing::warn!("could not install the Paper background service: {error:#}");
            State {
                service: ServiceState::Unavailable,
                updated: now_unix(),
            }
        }
    };
    let _ = next.save(&state_path);
}

/// Download the default embedding model if it is missing. Runs inside the
/// daemon, so the user's command has already returned. Failure is not fatal:
/// keyword search keeps working and the next daemon start tries again.
pub async fn ensure_models(workspace: &crate::cli::Workspace) {
    #[cfg(feature = "embeddings")]
    {
        let switches = Switches::from_process();
        if switches.offline || switches.no_autostart {
            return;
        }
        if workspace.embedding_model_installed() {
            return;
        }
        tracing::info!("downloading the default embedding model in the background");
        if let Err(error) = workspace
            .model_manager
            .ensure(crate::core::models::DEFAULT_EMBEDDING)
            .await
        {
            tracing::warn!(
                "embedding model download failed ({error:#}); continuing with keyword search"
            );
        }
    }
    #[cfg(not(feature = "embeddings"))]
    let _ = workspace;
}

/// True when the index holds no chunks yet, which is the moment to index
/// immediately rather than wait for the five-minute incremental tick.
pub fn index_is_empty(paths: &Paths) -> bool {
    let Ok(connection) = crate::core::index::Connection::open(&paths.db_path()) else {
        return true;
    };
    if crate::core::index::init_db(&connection).is_err() {
        return true;
    }
    connection
        .query_i64("SELECT COUNT(*) FROM chunks")
        .map(|count| count == 0)
        .unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_when_disabled() {
        let enabled = Switches::default();
        assert!(should_run("search", &enabled));

        for switches in [
            Switches {
                offline: true,
                ..enabled
            },
            Switches {
                no_autostart: true,
                ..enabled
            },
            Switches {
                ci: true,
                ..enabled
            },
        ] {
            assert!(!should_run("search", &switches));
        }
    }

    #[test]
    fn skips_portable_installs_unless_forced() {
        let portable = Switches {
            portable: true,
            ..Switches::default()
        };
        assert!(!should_run("search", &portable));
        assert!(should_run(
            "search",
            &Switches {
                force: true,
                ..portable
            }
        ));
    }

    #[test]
    fn skips_commands_that_manage_the_daemon() {
        for command in SKIP_COMMANDS {
            assert!(!should_run(command, &Switches::default()), "{command}");
        }
        for command in ["search", "answer", "serve", "aggregate"] {
            assert!(should_run(command, &Switches::default()), "{command}");
        }
    }

    #[test]
    fn state_round_trips() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(STATE_FILE);

        let mut state = State::load(&path);
        assert_eq!(state.service, ServiceState::Pending);
        assert!(state.should_retry(0));

        state.service = ServiceState::Installed;
        state.updated = 42;
        state.save(&path).expect("save");

        let loaded = State::load(&path);
        assert_eq!(loaded.service, ServiceState::Installed);
        assert_eq!(loaded.updated, 42);
        assert!(!loaded.should_retry(10_000));
    }

    #[test]
    fn failed_installs_retry_after_the_backoff() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(STATE_FILE);
        let state = State {
            service: ServiceState::Unavailable,
            updated: 1_000,
        };
        state.save(&path).expect("save");
        let loaded = State::load(&path);
        assert!(!loaded.should_retry(1_000 + RETRY_AFTER.as_secs() - 1));
        assert!(loaded.should_retry(1_000 + RETRY_AFTER.as_secs()));
    }

    #[test]
    fn unknown_state_file_is_ignored() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(STATE_FILE);
        fs::write(&path, "version=0\nservice=installed\n").expect("write");
        assert_eq!(State::load(&path).service, ServiceState::Pending);
    }

    #[test]
    fn empty_index_is_reported_as_empty() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = Paths::with_base(dir.path());
        paths.ensure_dirs().expect("dirs");
        assert!(index_is_empty(&paths));
    }
}
