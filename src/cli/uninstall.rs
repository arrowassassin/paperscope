//! `paper uninstall` — remove the service, the data and (where we can) the binary.
//!
//! Package-manager installs are finished by the package manager, so for brew,
//! npm and cargo we only print the command. A shell-installer binary is ours to
//! delete. Shell startup files are never edited.

use std::io::{BufRead, IsTerminal};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Args;

use crate::core::paths::Paths;
use crate::util::format_bytes;

#[derive(Args)]
pub struct UninstallArgs {
    /// Do not ask before deleting data.
    #[arg(long, short = 'y')]
    pub yes: bool,
    /// Keep the index, models, config and state; remove only the service and binary.
    #[arg(long)]
    pub keep_data: bool,
    /// Print what would be done and change nothing.
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum InstallMethod {
    Homebrew,
    Npm,
    Cargo,
    ShellInstaller,
    Unknown,
}

const BREW_PREFIXES: &[&str] = &[
    "/opt/homebrew",
    "/usr/local/Cellar",
    "/home/linuxbrew/.linuxbrew",
];

/// Pure: decide how `exe` (canonicalised) was installed. `cargo_lists_paper` is
/// whether `$CARGO_HOME/.crates2.json`/`.crates.toml` names `paperscope`.
pub fn detect_install_method(
    exe: &Path,
    cargo_home: Option<&Path>,
    home: Option<&Path>,
    cargo_lists_paper: bool,
) -> InstallMethod {
    let text = exe.to_string_lossy().replace('\\', "/");
    if text.contains("/Cellar/") || BREW_PREFIXES.iter().any(|p| exe.starts_with(p)) {
        return InstallMethod::Homebrew;
    }
    if text.contains("node_modules") {
        return InstallMethod::Npm;
    }
    let in_dir = |dir: Option<PathBuf>| dir.is_some_and(|d| exe.starts_with(d));
    let in_cargo_bin = in_dir(cargo_home.map(|c| c.join("bin")));
    if in_cargo_bin && cargo_lists_paper {
        return InstallMethod::Cargo;
    }
    let in_installer_dir = in_cargo_bin
        || in_dir(home.map(|h| h.join(".cargo/bin")))
        || in_dir(home.map(|h| h.join(".local/bin")));
    // Only binaries in the installer's own folders count: a receipt alone must
    // not let uninstall delete a dev build running from somewhere else.
    if in_installer_dir {
        return InstallMethod::ShellInstaller;
    }
    InstallMethod::Unknown
}

pub(crate) fn cargo_home(home: Option<&Path>) -> Option<PathBuf> {
    std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| home.map(|h| h.join(".cargo")))
}

pub(crate) fn cargo_lists_paper(cargo_home: &Path) -> bool {
    [".crates2.json", ".crates.toml"].iter().any(|name| {
        std::fs::read_to_string(cargo_home.join(name))
            .is_ok_and(|text| text.contains("\"paperscope "))
    })
}

pub(crate) fn receipt_path(home: Option<&Path>) -> Option<PathBuf> {
    let dir = if cfg!(windows) {
        dirs::data_local_dir()?
    } else {
        match std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
            Some(xdg) => PathBuf::from(xdg),
            None => home?.join(".config"),
        }
    };
    Some(dir.join("paperscope").join("paperscope-receipt.json"))
}

/// How the running binary was installed.
#[cfg(feature = "daemon")]
pub(crate) fn current_install_method() -> InstallMethod {
    let Some(exe) = std::env::current_exe()
        .ok()
        .and_then(|p| p.canonicalize().ok())
    else {
        return InstallMethod::Unknown;
    };
    let home = dirs::home_dir();
    let cargo = cargo_home(home.as_deref());
    detect_install_method(
        &exe,
        cargo.as_deref(),
        home.as_deref(),
        cargo.as_deref().is_some_and(cargo_lists_paper),
    )
}

/// Directories Paper owns, existing only. Never a shared parent such as
/// `~/.config` or `~/.cache` (possible if `PAPER_HOME` points at one).
fn owned_dirs(paths: &Paths) -> Vec<PathBuf> {
    let mut dirs_out = vec![paths.base_dir()];
    if let Some(config) = paths.config_path().parent() {
        dirs_out.push(config.to_path_buf());
    }
    let shared: Vec<PathBuf> = [dirs::home_dir(), dirs::cache_dir(), dirs::config_dir()]
        .into_iter()
        .flatten()
        .collect();
    let mut out: Vec<PathBuf> = Vec::new();
    for dir in dirs_out {
        let unsafe_dir =
            dir.parent().is_none() || dir.as_os_str().is_empty() || dir == Path::new(".");
        if unsafe_dir || shared.contains(&dir) || !dir.is_dir() || out.contains(&dir) {
            continue;
        }
        out.push(dir);
    }
    out
}

fn dir_size(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .map(|e| match e.metadata() {
            Ok(m) if m.is_dir() => dir_size(&e.path()),
            Ok(m) => m.len(),
            Err(_) => 0,
        })
        .sum()
}

#[derive(Debug, PartialEq, Eq)]
pub enum DataOutcome {
    Nothing,
    Kept,
    DryRun(u64),
    NeedsYes,
    Cancelled,
    Removed(u64),
}

/// The data step. `input` is only read when a prompt is needed.
pub fn remove_data(
    dirs: &[PathBuf],
    args: &UninstallArgs,
    interactive: bool,
    input: &mut dyn BufRead,
) -> Result<DataOutcome> {
    if args.keep_data {
        return Ok(DataOutcome::Kept);
    }
    if dirs.is_empty() {
        println!("No Paper data found.");
        return Ok(DataOutcome::Nothing);
    }
    let total: u64 = dirs.iter().map(|d| dir_size(d)).sum();
    println!("Paper data ({}):", format_bytes(total));
    for dir in dirs {
        println!("  {}", dir.display());
    }
    if args.dry_run {
        println!("(dry run) would delete the above");
        return Ok(DataOutcome::DryRun(total));
    }
    if !args.yes {
        if !interactive {
            println!("Not deleting: stdin is not a terminal. Re-run with --yes to delete it.");
            return Ok(DataOutcome::NeedsYes);
        }
        print!("Delete these? [y/N] ");
        std::io::Write::flush(&mut std::io::stdout()).ok();
        let mut answer = String::new();
        input.read_line(&mut answer)?;
        if !matches!(answer.trim().to_lowercase().as_str(), "y" | "yes") {
            println!("Data kept.");
            return Ok(DataOutcome::Cancelled);
        }
    }
    for dir in dirs {
        std::fs::remove_dir_all(dir).with_context(|| format!("deleting {}", dir.display()))?;
    }
    println!("Deleted Paper data.");
    Ok(DataOutcome::Removed(total))
}

fn remove_service(dry_run: bool) -> &'static str {
    #[cfg(feature = "daemon")]
    {
        if dry_run {
            println!("(dry run) would remove the background service");
            return "service (dry run)";
        }
        match crate::cli::daemon::uninstall_service() {
            Ok(path) => {
                println!("Removed background service: {}", path.display());
                "service removed"
            }
            Err(error) => {
                println!("Background service not removed ({error:#}); continuing.");
                "service not removed"
            }
        }
    }
    #[cfg(not(feature = "daemon"))]
    {
        let _ = dry_run;
        "service skipped"
    }
}

/// Binary step. Returns what is left for the user, if anything.
fn handle_binary(
    method: &InstallMethod,
    exe: &Path,
    receipt: Option<&Path>,
    dry_run: bool,
) -> Result<Option<String>> {
    match method {
        InstallMethod::Homebrew => println!("finish with: brew uninstall paper"),
        InstallMethod::Npm => println!("finish with: npm uninstall -g paperscope"),
        InstallMethod::Cargo => println!("finish with: cargo uninstall paperscope"),
        InstallMethod::Unknown => println!("delete the binary yourself: {}", exe.display()),
        InstallMethod::ShellInstaller => {
            if dry_run {
                println!("(dry run) would delete {}", exe.display());
                if let Some(r) = receipt.filter(|r| r.exists()) {
                    println!("(dry run) would delete {}", r.display());
                }
            } else {
                if let Some(r) = receipt.filter(|r| r.exists()) {
                    std::fs::remove_file(r).with_context(|| format!("deleting {}", r.display()))?;
                    println!("Deleted install receipt: {}", r.display());
                }
                if cfg!(windows) {
                    println!("delete the binary after this exits: {}", exe.display());
                } else {
                    std::fs::remove_file(exe)
                        .with_context(|| format!("deleting {}", exe.display()))?;
                    println!("Deleted binary: {}", exe.display());
                }
            }
            println!(
                "The installer may have added a PATH line to ~/.bashrc, ~/.zshrc, ~/.profile or \
                 your fish config; you can remove it. Paper does not edit those files."
            );
            if cfg!(windows) {
                return Ok(Some(format!("the binary {}", exe.display())));
            }
            return Ok(Some("any PATH line in your shell startup file".into()));
        }
    }
    Ok(Some(match method {
        InstallMethod::Homebrew => "the Homebrew package".into(),
        InstallMethod::Npm => "the npm package".into(),
        InstallMethod::Cargo => "the cargo package".into(),
        _ => format!("the binary {}", exe.display()),
    }))
}

pub async fn run(args: UninstallArgs) -> Result<i32> {
    let service = remove_service(args.dry_run);

    let dirs = owned_dirs(&Paths::new());
    let interactive = std::io::stdin().is_terminal();
    let data = remove_data(&dirs, &args, interactive, &mut std::io::stdin().lock())?;

    let exe = std::env::current_exe()
        .and_then(|p| p.canonicalize())
        .context("locating the paper binary")?;
    let home = dirs::home_dir();
    let cargo = cargo_home(home.as_deref());
    let receipt = receipt_path(home.as_deref());
    let method = detect_install_method(
        &exe,
        cargo.as_deref(),
        home.as_deref(),
        cargo.as_deref().is_some_and(cargo_lists_paper),
    );
    let left = handle_binary(&method, &exe, receipt.as_deref(), args.dry_run)?;

    let data_note = match data {
        DataOutcome::Removed(b) => format!("data deleted ({})", format_bytes(b)),
        DataOutcome::DryRun(_) => "data would be deleted".into(),
        DataOutcome::Kept => "data kept (--keep-data)".into(),
        DataOutcome::Cancelled | DataOutcome::NeedsYes => "data kept".into(),
        DataOutcome::Nothing => "no data found".into(),
    };
    let prefix = if args.dry_run {
        "Dry run, nothing changed: "
    } else {
        "Done: "
    };
    println!(
        "{prefix}{service}; {data_note}. Left for you: {}.",
        left.unwrap_or_else(|| "nothing".into())
    );
    Ok(super::EXIT_FOUND)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `_receipt` documents that a receipt never changes the verdict.
    fn detect(exe: &str, listed: bool, _receipt: bool) -> InstallMethod {
        detect_install_method(
            Path::new(exe),
            Some(Path::new("/home/u/.cargo")),
            Some(Path::new("/home/u")),
            listed,
        )
    }

    #[test]
    fn detects_homebrew() {
        assert_eq!(
            detect("/opt/homebrew/bin/paper", false, false),
            InstallMethod::Homebrew
        );
        assert_eq!(
            detect("/usr/local/Cellar/paper/0.3.1/bin/paper", false, false),
            InstallMethod::Homebrew
        );
    }

    #[test]
    fn detects_npm() {
        let exe = "/usr/lib/node_modules/paperscope/bin/paper";
        assert_eq!(detect(exe, false, true), InstallMethod::Npm);
    }

    #[test]
    fn detects_cargo_only_when_listed() {
        let exe = "/home/u/.cargo/bin/paper";
        assert_eq!(detect(exe, true, false), InstallMethod::Cargo);
        assert_eq!(detect(exe, false, false), InstallMethod::ShellInstaller);
    }

    #[test]
    fn detects_shell_installer_by_folder_never_by_receipt_alone() {
        assert_eq!(detect("/srv/paper", false, true), InstallMethod::Unknown);
        assert_eq!(
            detect("/home/u/.local/bin/paper", false, false),
            InstallMethod::ShellInstaller
        );
    }

    #[test]
    fn unknown_otherwise() {
        assert_eq!(detect("/srv/paper", false, false), InstallMethod::Unknown);
    }

    fn args(yes: bool, keep_data: bool, dry_run: bool) -> UninstallArgs {
        UninstallArgs {
            yes,
            keep_data,
            dry_run,
        }
    }

    #[test]
    fn data_step_modes() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("paper");
        std::fs::create_dir_all(dir.join("models")).unwrap();
        std::fs::write(dir.join("chunks.db"), b"abc").unwrap();
        let dirs = vec![dir.clone()];

        let r = |a: UninstallArgs, tty: bool, input: &str| {
            remove_data(&dirs, &a, tty, &mut input.as_bytes()).unwrap()
        };
        assert_eq!(r(args(true, false, true), true, ""), DataOutcome::DryRun(3));
        assert_eq!(r(args(true, true, false), true, ""), DataOutcome::Kept);
        assert_eq!(
            r(args(false, false, false), false, "y\n"),
            DataOutcome::NeedsYes
        );
        assert_eq!(
            r(args(false, false, false), true, "n\n"),
            DataOutcome::Cancelled
        );
        assert_eq!(
            r(args(false, false, false), true, "\n"),
            DataOutcome::Cancelled
        );
        assert!(dir.join("chunks.db").exists());
        assert_eq!(
            r(args(false, false, false), true, "Yes\n"),
            DataOutcome::Removed(3)
        );
        assert!(!dir.exists());
    }

    #[test]
    fn owned_dirs_skips_missing_and_shared() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(
            owned_dirs(&Paths::with_base(tmp.path())),
            vec![tmp.path().to_path_buf()]
        );
        assert!(owned_dirs(&Paths::with_base(tmp.path().join("nope"))).is_empty());
        if let Some(home) = dirs::home_dir() {
            assert!(owned_dirs(&Paths::with_base(home)).is_empty());
        }
    }
}
