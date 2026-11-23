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
//!   2. the daemon downloads the default models in the background (the
//!      embedding model first, then the optional reranker);
//!   3. the daemon indexes the auto-discovered roots immediately (instead of
//!      waiting for the first five-minute scheduler tick).
//!
//! Escape hatches: `PAPER_NO_AUTOSTART=1` (never), `PAPER_OFFLINE=1` (no
//! network at all), `PAPER_FORCE_BOOTSTRAP=1` (run even when `PAPER_HOME` is
//! set), and `paper daemon-control uninstall` (turn it off afterwards).
//!
//! `CI` suppresses it too: a build machine must never index its own files.

use std::fs::{self, File, TryLockError};
use std::io;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::core::paths::Paths;

/// Bumped when the state file layout changes; an older file is ignored.
const STATE_VERSION: &str = "1";
/// Do not retry a failed service install more often than this.
const RETRY_AFTER: Duration = Duration::from_secs(60 * 60);

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
    // `serve` is the MCP server an editor starts, often several at once. It
    // must answer the handshake and serve keyword results immediately; it must
    // never install a service or take the setup lock. Without this, five MCP
    // sessions opening together serialised behind one lock and each tried to
    // provision — the slowest possible answer to a first `initialize`.
    "serve",
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
///
/// This is a kernel advisory lock (`flock` / `LockFileEx`, through
/// [`File::try_lock`]), not a "does the file exist?" test. The difference is
/// the whole point: a file-based lock has to guess whether its owner is still
/// alive, and the guess is racy — two processes can both read the same
/// 15-minute-old file, both decide it is stale, both delete it, and both
/// "acquire" it. A kernel lock is released when the holder's descriptor closes,
/// which the OS does even when the holder is killed, so no staleness heuristic
/// (and no reliance on an mtime a clock change or a restore can move) remains.
struct BootstrapLock {
    file: File,
}

impl BootstrapLock {
    fn acquire(path: &Path) -> Option<Self> {
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .ok()?;
        match file.try_lock() {
            Ok(()) => Some(Self { file }),
            // `WouldBlock` is the ordinary "someone else is setting up" answer.
            // Any other error means this filesystem cannot lock at all; that
            // must never be read as "the lock is mine".
            Err(TryLockError::WouldBlock | TryLockError::Error(_)) => None,
        }
    }
}

impl Drop for BootstrapLock {
    fn drop(&mut self) {
        // Dropping the handle releases the lock; unlocking first makes that
        // explicit and keeps the release observable in this function.
        let _ = self.file.unlock();
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

/// Download the default models if they are missing. Runs inside the daemon, so
/// the user's command has already returned. Failure is not fatal: keyword
/// search keeps working and the next daemon start tries again.
///
/// The embedding model comes first because indexing needs it. The reranker is
/// best-effort and second: it only improves ordering, so a failed or skipped
/// download costs accuracy, never functionality. `PAPER_NO_RERANKER=1` keeps
/// the ~1.1 GB reranker off disk entirely (small disk, metered connection).
pub async fn ensure_models(workspace: &crate::cli::Workspace) {
    let switches = Switches::from_process();
    if switches.offline || switches.no_autostart {
        return;
    }
    #[cfg(feature = "embeddings")]
    {
        if !workspace.embedding_model_installed() {
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
        if std::env::var_os("PAPER_NO_RERANKER").is_none()
            && !workspace
                .model_manager
                .is_installed(crate::core::models::OPTIONAL_RERANKER)
        {
            tracing::info!("downloading the optional reranker in the background");
            if let Err(error) = workspace
                .model_manager
                .ensure(crate::core::models::OPTIONAL_RERANKER)
                .await
            {
                tracing::warn!("reranker download failed ({error:#}); results stay dense+keyword");
            }
        }
    }
    #[cfg(not(feature = "embeddings"))]
    let _ = workspace;

    // OCR weights are small (~16 MB) and independent of the embedding stack, so
    // they are fetched even in a keyword-only build. A failure only disables
    // OCR; indexing still proceeds with whatever text layers exist.
    #[cfg(feature = "ocr")]
    if std::env::var_os("PAPER_NO_OCR").is_none() && !crate::core::extract::ocr::models_present() {
        tracing::info!("downloading OCR models in the background");
        if let Err(error) = crate::core::extract::ocr::ensure_models().await {
            tracing::warn!("OCR model download failed ({error:#}); scanned pages stay unindexed");
        }
    }
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
        for command in ["search", "answer", "aggregate"] {
            assert!(should_run(command, &Switches::default()), "{command}");
        }
    }

    /// `serve` is what an editor starts — often five sessions at once — so it is
    /// the one command that must never be the thing that installs a service.
    #[test]
    fn serve_never_bootstraps() {
        assert!(!should_run("serve", &Switches::default()));
        // Not even the portable force switch, which exists for installs that
        // want onboarding: a server session is never the right place for it.
        let forced_portable = Switches {
            portable: true,
            force: true,
            ..Switches::default()
        };
        assert!(!should_run("serve", &forced_portable));
    }

    /// A lock file with nobody holding it is what a killed run leaves behind.
    /// The old `create_new` test read that state as "someone is setting up".
    #[test]
    fn a_lock_file_left_by_a_dead_run_does_not_block_setup() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(LOCK_FILE);
        fs::write(&path, b"").expect("write");
        assert!(
            BootstrapLock::acquire(&path).is_some(),
            "an unheld lock file must be acquirable"
        );
    }

    #[test]
    fn a_held_lock_is_exclusive_and_released_on_drop() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(LOCK_FILE);
        let first = BootstrapLock::acquire(&path).expect("first acquisition");
        assert!(
            BootstrapLock::acquire(&path).is_none(),
            "a second holder must be refused while the first lives"
        );
        drop(first);
        assert!(
            BootstrapLock::acquire(&path).is_some(),
            "dropping the guard must release the lock"
        );
    }

    /// Child half of [`a_lock_is_released_when_its_holder_is_killed`]: acquire,
    /// say so on stdout, then wait to be killed. Ignored by default; the parent
    /// runs it by exact name in a separate process.
    #[test]
    #[ignore = "child process helper for a_lock_is_released_when_its_holder_is_killed"]
    fn lock_child_acquires_and_waits() {
        let Ok(path) = std::env::var("PAPER_LOCK_CHILD") else {
            return;
        };
        let lock = BootstrapLock::acquire(Path::new(&path)).expect("child acquires");
        println!("acquired");
        std::thread::sleep(Duration::from_secs(60));
        drop(lock);
    }

    /// The property the staleness heuristic was reaching for, proved instead of
    /// guessed: the kernel drops the lock when the holder dies, so a crashed run
    /// never wedges the next one.
    #[test]
    fn a_lock_is_released_when_its_holder_is_killed() {
        use std::io::BufRead;

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(LOCK_FILE);
        let mut child = std::process::Command::new(std::env::current_exe().expect("exe"))
            .args([
                "--exact",
                "bootstrap::tests::lock_child_acquires_and_waits",
                "--ignored",
                "--nocapture",
            ])
            .env("PAPER_LOCK_CHILD", &path)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn helper");

        // libtest writes its own banner before the test's output, so read until
        // the marker appears or the helper exits.
        let mut collected = String::new();
        let mut reader = std::io::BufReader::new(child.stdout.take().expect("stdout"));
        let mut line = String::new();
        loop {
            line.clear();
            let read = reader.read_line(&mut line).expect("read helper output");
            if read == 0 {
                break;
            }
            collected.push_str(&line);
            if line.contains("acquired") {
                break;
            }
        }
        assert!(
            collected.contains("acquired"),
            "helper never acquired the lock: {collected:?}"
        );
        assert!(
            BootstrapLock::acquire(&path).is_none(),
            "a live holder must refuse a second acquirer"
        );

        child.kill().expect("kill helper");
        let _ = child.wait();

        // The close that releases the lock is asynchronous relative to `kill`,
        // so poll briefly rather than asserting on a race.
        let mut acquired = None;
        for _ in 0..100 {
            if let Some(lock) = BootstrapLock::acquire(&path) {
                acquired = Some(lock);
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            acquired.is_some(),
            "the lock must be free once its holder is dead"
        );
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
