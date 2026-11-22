//! `paper update` — move an installed binary to the latest release.
//!
//! Updating goes through the same installer a fresh install uses, so there is
//! exactly one code path that knows where files belong, how to verify the
//! download and how to touch `PATH`. `--check` is offline-safe reporting only.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::Args;

use super::{EXIT_FOUND, EXIT_NOT_FOUND};

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

    if !is_newer(&latest, current) {
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

/// Numeric comparison of `major.minor.patch`; a pre-release suffix sorts low.
fn is_newer(candidate: &str, current: &str) -> bool {
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

    let path = temp_script_path()?;
    std::fs::write(&path, script)
        .with_context(|| format!("write installer to {}", path.display()))?;
    let mut command = std::process::Command::new(program);
    if !flag.is_empty() {
        command.arg(flag);
    }
    command.arg(&path);
    let status = command.status().context("run installer")?;
    let _ = std::fs::remove_file(&path);
    if let Some(dir) = path.parent() {
        let _ = std::fs::remove_dir(dir);
    }
    if !status.success() {
        bail!("installer exited with {status}; nothing was changed");
    }
    Ok(())
}

fn temp_script_path() -> Result<PathBuf> {
    let name = if cfg!(windows) {
        "paperscope-installer.ps1"
    } else {
        "paperscope-installer.sh"
    };
    let dir = std::env::temp_dir().join(format!("paper-update-{}", std::process::id()));
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("creating temporary directory {}", dir.display()))?;
    Ok(dir.join(name))
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
    for dir in std::env::split_paths(&path) {
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
    use super::is_newer;

    #[test]
    fn temp_script_directory_exists() {
        let path = super::temp_script_path().expect("temp path");
        assert!(path.parent().expect("parent").is_dir());
        if let Some(dir) = path.parent() {
            let _ = std::fs::remove_dir(dir);
        }
    }

    #[test]
    fn compares_versions_numerically() {
        assert!(is_newer("0.2.10", "0.2.9"));
        assert!(is_newer("0.3.0", "0.2.99"));
        assert!(!is_newer("0.2.7", "0.2.7"));
        assert!(!is_newer("0.2.6", "0.2.7"));
        assert!(is_newer("0.2.7", "0.2.7-beta.1"));
    }
}
