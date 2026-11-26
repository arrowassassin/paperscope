//! `paper update` — move an installed binary to the latest release.
//!
//! Updating goes through the same installer a fresh install uses, so there is
//! exactly one code path that knows where files belong, how to verify the
//! download and how to touch `PATH`. `--check` is offline-safe reporting only.

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::Args;

use super::{EXIT_FOUND, EXIT_NOT_FOUND};
use crate::core::update_state::version_newer;

const REPO: &str = "kintsugi-tools/paperscope";
const API_LATEST: &str = "https://api.github.com/repos/kintsugi-tools/paperscope/releases/latest";

#[derive(Args)]
pub struct UpdateArgs {
    /// Only report whether a newer release exists; change nothing.
    #[arg(long)]
    pub check: bool,
}

pub async fn run(args: UpdateArgs) -> Result<i32> {
    let current = env!("CARGO_PKG_VERSION");
    println!("paper {current}");

    let latest = match latest_release().await {
        Ok(tag) => tag,
        Err(error) => {
            println!("could not check for updates: {error:#}");
            println!("check your connection, or reinstall from https://kintsugi.tools/paper.sh");
            return Ok(EXIT_NOT_FOUND);
        }
    };

    if !version_newer(&latest, current) {
        println!("up to date (latest release is {latest})");
        warn_if_shadowed();
        return Ok(EXIT_FOUND);
    }

    println!("newer release available: {latest}");
    if args.check {
        println!("run `paper update` to install it");
        return Ok(EXIT_FOUND);
    }

    install_latest().await?;
    println!("updated to {latest}");
    warn_if_shadowed();
    Ok(EXIT_FOUND)
}

/// Ask GitHub for the newest published release tag, without a token.
async fn latest_release() -> Result<String> {
    let client = reqwest::Client::builder()
        .user_agent(concat!("paper/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(20))
        .build()?;
    let response = client
        .get(API_LATEST)
        .send()
        .await
        .context("query GitHub releases")?
        .error_for_status()
        .context("query GitHub releases")?;
    let body: serde_json::Value = response.json().await.context("read release metadata")?;
    let tag = body
        .get("tag_name")
        .and_then(|value| value.as_str())
        .context("release metadata has no tag_name")?;
    Ok(tag.trim_start_matches('v').to_string())
}

/// Run the platform installer from the latest release.
async fn install_latest() -> Result<()> {
    #[cfg(windows)]
    let (url, program, flag) = (
        format!("https://github.com/{REPO}/releases/latest/download/paperscope-installer.ps1"),
        "powershell",
        "-File",
    );
    #[cfg(not(windows))]
    let (url, program, flag) = (
        format!("https://github.com/{REPO}/releases/latest/download/paperscope-installer.sh"),
        "sh",
        "",
    );

    let client = reqwest::Client::builder()
        .user_agent(concat!("paper/", env!("CARGO_PKG_VERSION")))
        .build()?;
    let script = client
        .get(&url)
        .send()
        .await
        .context("download installer")?
        .error_for_status()
        .context("download installer")?
        .text()
        .await
        .context("read installer")?;

    // The guard removes the directory and script when it drops, after the installer exits.
    let (_dir, path) = temp_script_path(&std::env::temp_dir())?;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .and_then(|mut file| file.write_all(script.as_bytes()))
        .with_context(|| format!("write installer to {}", path.display()))?;
    let mut command = std::process::Command::new(program);
    if !flag.is_empty() {
        command.arg(flag);
    }
    command.arg(&path);
    let status = command.status().context("run installer")?;
    if !status.success() {
        bail!("installer exited with {status}; nothing was changed");
    }
    Ok(())
}

/// Installer path inside a fresh, randomly named directory under `base`, created
/// exclusively (0700 on Unix) so no other local user can pre-create or plant it.
fn temp_script_path(base: &Path) -> Result<(tempfile::TempDir, PathBuf)> {
    let name = if cfg!(windows) {
        "paperscope-installer.ps1"
    } else {
        "paperscope-installer.sh"
    };
    std::fs::create_dir_all(base)
        .with_context(|| format!("creating temporary directory {}", base.display()))?;
    let mut builder = tempfile::Builder::new();
    builder.prefix("paper-update-");
    #[cfg(unix)]
    builder.permissions(std::os::unix::fs::PermissionsExt::from_mode(0o700));
    let dir = builder
        .tempdir_in(base)
        .with_context(|| format!("creating temporary directory in {}", base.display()))?;
    let path = dir.path().join(name);
    Ok((dir, path))
}

/// Tell the user when `paper` on `PATH` resolves to a different binary.
///
/// This is the single most confusing update symptom: Homebrew and the shell
/// installer both install a `paper`, and whichever directory comes first wins.
pub fn warn_if_shadowed() {
    let Ok(current) = std::env::current_exe() else {
        return;
    };
    let current = canonical(&current);
    let Some(path) = std::env::var_os("PATH") else {
        return;
    };
    for dir in absolute_dirs(&path) {
        let candidate = dir.join(if cfg!(windows) { "paper.exe" } else { "paper" });
        if !candidate.is_file() {
            continue;
        }
        let candidate = canonical(&candidate);
        if candidate != current {
            println!(
                "note: `paper` in {} is a different copy ({}) and comes first in PATH;",
                dir.display(),
                version_of(&candidate).unwrap_or_else(|| "version unknown".to_string())
            );
            println!(
                "      remove it or put {} earlier in PATH to run this one.",
                current.display()
            );
            return;
        }
    }
}

/// PATH entries that name a fixed directory. Empty, `.` and relative entries
/// resolve against the current directory (shells do resolve them), so a
/// `paper` there is not run by this shadow check.
fn absolute_dirs(path: &std::ffi::OsStr) -> impl Iterator<Item = PathBuf> + '_ {
    std::env::split_paths(path).filter(|dir| dir.is_absolute())
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn version_of(binary: &Path) -> Option<String> {
    let output = std::process::Command::new(binary)
        .arg("--version")
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    text.split_whitespace().last().map(str::to_string)
}

#[cfg(test)]
mod tests {
    #[test]
    fn temp_script_directory_is_fresh_and_private() {
        let root = tempfile::tempdir().expect("tempdir");
        // A missing temp root is still created, as before.
        let base = root.path().join("missing");
        let (first, path) = super::temp_script_path(&base).expect("temp path");
        let (second, _) = super::temp_script_path(&base).expect("temp path");
        assert_eq!(path.parent(), Some(first.path()));
        assert_ne!(first.path(), second.path());
        assert!(!path.exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(first.path())
                .expect("metadata")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o700);
        }
        let dir = first.path().to_path_buf();
        drop(first);
        assert!(!dir.exists());
    }

    #[test]
    fn relative_path_entries_are_ignored() {
        let abs = std::env::current_dir().expect("current dir");
        let path = std::env::join_paths(["", ".", "bin", abs.to_str().expect("utf-8"), ""])
            .expect("join PATH");
        assert_eq!(super::absolute_dirs(&path).collect::<Vec<_>>(), [abs]);
    }
}
