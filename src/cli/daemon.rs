//! CLI control and OS service integration for the Paper daemon.

use crate::core::paths::Paths;
#[cfg(unix)]
use crate::daemon::ipc::IpcClient;
use crate::daemon::ipc::{DaemonRequest, DaemonResponse};
#[cfg(unix)]
use anyhow::Context;
use anyhow::{bail, Result};
use clap::{Args, Subcommand};
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::fs;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::io::Write;
use std::path::{Path, PathBuf};
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::process::Command;
#[cfg(unix)]
use std::time::Duration;

#[cfg(unix)]
const IPC_TIMEOUT: Duration = Duration::from_secs(5);
const LAUNCHD_LABEL: &str = "tools.kintsugi.paper";
#[cfg(target_os = "linux")]
const SYSTEMD_SERVICE_NAME: &str = "paper.service";

/// Control a running daemon or manage its OS service.
#[derive(Args, Debug)]
pub struct DaemonControlArgs {
    /// Socket path for IPC (default: ~/.cache/paper/daemon.sock).
    #[arg(long, global = true)]
    pub socket: Option<PathBuf>,

    #[command(subcommand)]
    pub command: DaemonControlCommand,
}

#[derive(Subcommand, Debug)]
pub enum DaemonControlCommand {
    /// Show daemon status.
    Status {
        /// Emit the status response as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Pause indexing.
    Pause,
    /// Resume indexing.
    Resume,
    /// Index roots now (no roots = the daemon's configured roots).
    Index {
        #[arg(long)]
        root: Vec<PathBuf>,
    },
    /// Add a root to watch and index.
    AddRoot { path: PathBuf },
    /// Remove a root from watching and indexing.
    RemoveRoot { path: PathBuf },
    /// Ask the daemon to shut down gracefully.
    Stop,
    /// Install and start Paper as a user service.
    Install {
        /// Overwrite an existing, different service definition.
        #[arg(long)]
        force: bool,
    },
    /// Stop and remove the Paper user service.
    Uninstall,
    /// Start the installed Paper user service.
    Start,
    /// Restart the installed Paper user service.
    Restart,
}

pub async fn run(args: DaemonControlArgs) -> Result<i32> {
    let DaemonControlArgs { socket, command } = args;
    let socket_path = socket.unwrap_or_else(|| Paths::new().base_dir().join("daemon.sock"));

    match command {
        DaemonControlCommand::Status { json } => {
            let response = send_request(&socket_path, DaemonRequest::GetStatus).await?;
            print_status(&response, json)?;
        }
        DaemonControlCommand::Pause => {
            expect_ok(send_request(&socket_path, DaemonRequest::Pause).await?)?;
            println!("Daemon indexing paused.");
        }
        DaemonControlCommand::Resume => {
            expect_ok(send_request(&socket_path, DaemonRequest::Resume).await?)?;
            println!("Daemon indexing resumed.");
        }
        DaemonControlCommand::Index { root } => {
            let response =
                send_request(&socket_path, DaemonRequest::IndexRoots { roots: root }).await?;
            match response {
                DaemonResponse::IndexRoots { stats } => {
                    println!(
                        "Indexed {} file(s) ({} chunks); {} scanned, {} unchanged, {} skipped, {} warning(s).",
                        stats.indexed,
                        stats.chunks,
                        stats.scanned,
                        stats.unchanged,
                        stats.skipped,
                        stats.warnings
                    );
                    if stats.denied > 0 || stats.backed_off > 0 {
                        println!(
                            "  {} unreadable (permission denied), {} deferred by the backoff",
                            stats.denied, stats.backed_off
                        );
                    }
                }
                DaemonResponse::Error { msg } => bail!("daemon error: {msg}"),
                other => bail!("unexpected daemon response to index request: {other:?}"),
            }
        }
        DaemonControlCommand::AddRoot { path } => {
            let path = canonical_or_original(&path);
            persist_root(Some(&path), None)?;
            expect_ok(
                send_request(&socket_path, DaemonRequest::AddRoot { path: path.clone() }).await?,
            )?;
            println!("Added indexing root: {}", path.display());
        }
        DaemonControlCommand::RemoveRoot { path } => {
            let path = canonical_or_original(&path);
            persist_root(None, Some(&path))?;
            expect_ok(
                send_request(
                    &socket_path,
                    DaemonRequest::RemoveRoot { path: path.clone() },
                )
                .await?,
            )?;
            println!("Removed indexing root: {}", path.display());
        }
        DaemonControlCommand::Stop => {
            expect_ok(send_request(&socket_path, DaemonRequest::Shutdown).await?)?;
            println!("Daemon shutdown requested.");
        }
        DaemonControlCommand::Install { force } => {
            let path = install_service(force)?;
            println!("Installed and started Paper service: {}", path.display());
        }
        DaemonControlCommand::Uninstall => {
            let path = uninstall_service()?;
            println!("Uninstalled Paper service: {}", path.display());
        }
        DaemonControlCommand::Start => {
            start_service()?;
            println!("Started Paper service.");
        }
        DaemonControlCommand::Restart => {
            restart_service()?;
            println!("Restarted Paper service.");
        }
    }

    Ok(super::EXIT_FOUND)
}

fn print_status(response: &DaemonResponse, json: bool) -> Result<()> {
    match response {
        DaemonResponse::Status {
            indexed_files,
            queued,
            paused,
            model,
        } => {
            if json {
                println!("{}", serde_json::to_string(response)?);
            } else {
                println!("Indexed files: {indexed_files}");
                println!("Queued: {queued}");
                println!("Paused: {paused}");
                println!("Model: {}", model.as_deref().unwrap_or("none"));
            }
            Ok(())
        }
        DaemonResponse::Error { msg } => bail!("daemon error: {msg}"),
        other => bail!("unexpected daemon response to status request: {other:?}"),
    }
}

fn expect_ok(response: DaemonResponse) -> Result<()> {
    match response {
        DaemonResponse::Ok => Ok(()),
        DaemonResponse::Error { msg } => bail!("daemon error: {msg}"),
        other => bail!("unexpected daemon response: {other:?}"),
    }
}

#[cfg(unix)]
async fn send_request(socket_path: &Path, request: DaemonRequest) -> Result<DaemonResponse> {
    let operation = async {
        let mut client = IpcClient::connect(socket_path).await.with_context(|| {
            format!(
                "could not connect to paper daemon at {}; start it with `paper daemon --foreground` or run `paper daemon-control install`",
                socket_path.display()
            )
        })?;
        client.send(request).await.with_context(|| {
            format!(
                "communicating with paper daemon at {}",
                socket_path.display()
            )
        })
    };

    tokio::time::timeout(IPC_TIMEOUT, operation)
        .await
        .map_err(|_| {
            anyhow::anyhow!(
                "paper daemon at {} did not respond within {} seconds",
                socket_path.display(),
                IPC_TIMEOUT.as_secs()
            )
        })?
}

#[cfg(not(unix))]
async fn send_request(_socket_path: &Path, _request: DaemonRequest) -> Result<DaemonResponse> {
    bail!("daemon IPC is not yet supported on Windows")
}

/// Render a systemd user unit without touching the host service manager.
pub fn render_systemd_unit(exe_path: &Path) -> String {
    let exe = systemd_argument(&exe_path.to_string_lossy());
    format!(
        "# Managed by `paper daemon-control`.\n\
         [Unit]\n\
         Description=Paper local document index daemon\n\n\
         [Service]\n\
         Type=simple\n\
         ExecStart={exe} daemon --foreground\n\
         Restart=on-failure\n\n\
         [Install]\n\
         WantedBy=default.target\n"
    )
}

fn systemd_argument(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('%', "%%");
    format!("\"{escaped}\"")
}

/// Render a launchd user agent without touching the host service manager.
pub fn render_launchd_plist(exe_path: &Path, cache_dir: &Path) -> String {
    let exe = xml_escape(&exe_path.to_string_lossy());
    let stdout = xml_escape(&format!(
        "{}/daemon.stdout.log",
        cache_dir.to_string_lossy().replace('\\', "/")
    ));
    let stderr = xml_escape(&format!(
        "{}/daemon.stderr.log",
        cache_dir.to_string_lossy().replace('\\', "/")
    ));
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<!-- Managed by `paper daemon-control`. -->
<plist version="1.0">
  <dict>
    <key>Label</key>
    <string>{LAUNCHD_LABEL}</string>
    <key>ProgramArguments</key>
    <array>
      <string>{exe}</string>
      <string>daemon</string>
      <string>--foreground</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <false/>
    <key>StandardOutPath</key>
    <string>{stdout}</string>
    <key>StandardErrorPath</key>
    <string>{stderr}</string>
  </dict>
</plist>
"#
    )
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn install_service_file(path: &Path, contents: &str, force: bool) -> Result<()> {
    if !force && path.exists() {
        let existing = fs::read_to_string(path)
            .with_context(|| format!("reading existing service file {}", path.display()))?;
        if existing != contents {
            bail!(
                "service file {} already exists with different contents; use `paper daemon-control install --force` to replace it",
                path.display()
            );
        }
    }
    atomic_write(path, contents)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn atomic_write(path: &Path, contents: &str) -> Result<()> {
    let parent = path
        .parent()
        .with_context(|| format!("service path has no parent: {}", path.display()))?;
    fs::create_dir_all(parent)
        .with_context(|| format!("creating service directory {}", parent.display()))?;
    let file_name = path
        .file_name()
        .with_context(|| format!("service path has no file name: {}", path.display()))?;
    let temp_path = parent.join(format!(
        ".{}.{}.tmp",
        file_name.to_string_lossy(),
        std::process::id()
    ));

    let result = (|| -> Result<()> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
            .with_context(|| format!("creating temporary service file {}", temp_path.display()))?;
        file.write_all(contents.as_bytes())
            .with_context(|| format!("writing temporary service file {}", temp_path.display()))?;
        file.sync_all()
            .with_context(|| format!("syncing temporary service file {}", temp_path.display()))?;
        fs::rename(&temp_path, path).with_context(|| {
            format!(
                "renaming temporary service file {} to {}",
                temp_path.display(),
                path.display()
            )
        })?;
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn remove_service_file(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => {
            Err(error).with_context(|| format!("removing service file {}", path.display()))
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn require_service_installed(path: &Path) -> Result<()> {
    if !path.is_file() {
        bail!("Paper service is not installed; run `paper daemon-control install` first");
    }
    Ok(())
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn run_checked(command: &mut Command) -> Result<()> {
    let display = format!("{command:?}");
    let output = command
        .output()
        .with_context(|| format!("running {display}"))?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let details = if stderr.trim().is_empty() {
        stdout.trim()
    } else {
        stderr.trim()
    };
    bail!("{display} failed with {}: {details}", output.status)
}

#[cfg(target_os = "linux")]
fn systemd_unit_path() -> Result<PathBuf> {
    let config = dirs::config_dir().context("could not determine the user config directory")?;
    Ok(config.join("systemd/user").join(SYSTEMD_SERVICE_NAME))
}

#[cfg(target_os = "linux")]
fn install_service(force: bool) -> Result<PathBuf> {
    let path = systemd_unit_path()?;
    let exe = std::env::current_exe().context("locating the current paper executable")?;
    let unit = render_systemd_unit(&exe);
    install_service_file(&path, &unit, force)?;

    run_checked(Command::new("systemctl").args(["--user", "daemon-reload"]))?;
    run_checked(Command::new("systemctl").args([
        "--user",
        "enable",
        "--now",
        SYSTEMD_SERVICE_NAME,
    ]))?;
    Ok(path)
}

#[cfg(target_os = "linux")]
fn uninstall_service() -> Result<PathBuf> {
    let path = systemd_unit_path()?;
    if let Err(error) = run_checked(Command::new("systemctl").args([
        "--user",
        "disable",
        "--now",
        SYSTEMD_SERVICE_NAME,
    ])) {
        tracing::warn!("Could not disable the Paper service cleanly: {error:#}");
    }
    remove_service_file(&path)?;
    run_checked(Command::new("systemctl").args(["--user", "daemon-reload"]))?;
    Ok(path)
}

#[cfg(target_os = "linux")]
fn start_service() -> Result<()> {
    require_service_installed(&systemd_unit_path()?)?;
    run_checked(Command::new("systemctl").args(["--user", "start", SYSTEMD_SERVICE_NAME]))
}

#[cfg(target_os = "linux")]
fn restart_service() -> Result<()> {
    require_service_installed(&systemd_unit_path()?)?;
    run_checked(Command::new("systemctl").args(["--user", "restart", SYSTEMD_SERVICE_NAME]))
}

#[cfg(target_os = "macos")]
fn launchd_plist_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("could not determine the user home directory")?;
    Ok(home
        .join("Library/LaunchAgents")
        .join(format!("{LAUNCHD_LABEL}.plist")))
}

#[cfg(target_os = "macos")]
fn current_uid() -> Result<String> {
    let output = Command::new("id")
        .arg("-u")
        .output()
        .context("running `id -u` for launchd")?;
    if !output.status.success() {
        bail!("`id -u` failed with {}", output.status);
    }
    let uid = String::from_utf8(output.stdout).context("decoding `id -u` output")?;
    let uid = uid.trim();
    if uid.is_empty() {
        bail!("`id -u` returned an empty user ID");
    }
    Ok(uid.to_string())
}

#[cfg(target_os = "macos")]
fn load_launchd_service(domain: &str, path: &Path) -> Result<()> {
    // Boot out any already-loaded copy first. `bootstrap` refuses to replace an
    // existing job, so without this a reinstall that changes the binary path
    // leaves launchd running the old, possibly deleted, executable forever
    // (spawning it fails and the job exits 78, EX_CONFIG).
    let service = format!("{domain}/{LAUNCHD_LABEL}");
    let _ = run_checked(Command::new("launchctl").args(["bootout", &service]));

    let bootstrap = run_checked(
        Command::new("launchctl")
            .args(["bootstrap", domain])
            .arg(path),
    );
    if let Err(bootstrap_error) = bootstrap {
        run_checked(Command::new("launchctl").arg("load").arg(path)).with_context(|| {
            format!("launchctl bootstrap failed ({bootstrap_error:#}); `launchctl load` fallback also failed")
        })?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn install_service(force: bool) -> Result<PathBuf> {
    let path = launchd_plist_path()?;
    let exe = std::env::current_exe().context("locating the current paper executable")?;
    let cache_dir = Paths::new().base_dir();
    fs::create_dir_all(&cache_dir)
        .with_context(|| format!("creating Paper cache directory {}", cache_dir.display()))?;
    let plist = render_launchd_plist(&exe, &cache_dir);
    install_service_file(&path, &plist, force)?;

    let domain = format!("gui/{}", current_uid()?);
    load_launchd_service(&domain, &path)?;
    Ok(path)
}

#[cfg(target_os = "macos")]
fn uninstall_service() -> Result<PathBuf> {
    let path = launchd_plist_path()?;
    let domain = format!("gui/{}", current_uid()?);
    let bootout = run_checked(
        Command::new("launchctl")
            .args(["bootout", &domain])
            .arg(&path),
    );
    if let Err(bootout_error) = bootout {
        if let Err(unload_error) = run_checked(Command::new("launchctl").arg("unload").arg(&path)) {
            tracing::warn!(
                "Could not unload the Paper launchd agent cleanly (bootout: {bootout_error:#}; unload: {unload_error:#})"
            );
        }
    }
    remove_service_file(&path)?;
    Ok(path)
}

#[cfg(target_os = "macos")]
fn start_service() -> Result<()> {
    let path = launchd_plist_path()?;
    require_service_installed(&path)?;
    let domain = format!("gui/{}", current_uid()?);
    let service = format!("{domain}/{LAUNCHD_LABEL}");
    if run_checked(Command::new("launchctl").args(["kickstart", &service])).is_err() {
        load_launchd_service(&domain, &path)?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn restart_service() -> Result<()> {
    let path = launchd_plist_path()?;
    require_service_installed(&path)?;
    let domain = format!("gui/{}", current_uid()?);
    let service = format!("{domain}/{LAUNCHD_LABEL}");
    if run_checked(Command::new("launchctl").args(["kickstart", "-k", &service])).is_err() {
        load_launchd_service(&domain, &path)?;
    }
    Ok(())
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn install_service(_force: bool) -> Result<PathBuf> {
    bail!("OS service installation is not yet supported on Windows; use Linux or macOS")
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn uninstall_service() -> Result<PathBuf> {
    bail!("OS service management is not yet supported on Windows; use Linux or macOS")
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn start_service() -> Result<()> {
    bail!("OS service management is not yet supported on Windows; use Linux or macOS")
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn restart_service() -> Result<()> {
    bail!("OS service management is not yet supported on Windows; use Linux or macOS")
}

/// Install the background service, or start it when its file already exists
/// (for example after a binary upgrade). Used by first-run automation, so it
/// must be idempotent and must not fail because the service was already
/// registered.
pub(crate) fn ensure_service() -> Result<()> {
    match install_service(false) {
        Ok(_) => Ok(()),
        Err(install_error) => start_service().map_err(|start_error| {
            anyhow::anyhow!("install failed ({install_error:#}); start failed ({start_error:#})")
        }),
    }
}

fn canonical_or_original(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// Persist a root change in the config so it survives daemon restarts and
/// reboots. Without this, `add-root` only lived until the daemon exited.
fn persist_root(add: Option<&Path>, remove: Option<&Path>) -> Result<()> {
    let mut config = crate::core::config::Config::load().unwrap_or_default();
    if let Some(add) = add {
        if !add.exists() {
            bail!("{} does not exist", add.display());
        }
        if !config.roots.iter().any(|root| root == add) {
            config.roots.push(add.to_path_buf());
        }
    }
    if let Some(remove) = remove {
        config.roots.retain(|root| root != remove);
    }
    config.save()
}
