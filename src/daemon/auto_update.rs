//! Daily auto-update to the version the signed remote config approves, through
//! the install method that owns the binary. Only the daemon runs this.

use crate::cli::uninstall::{current_install_method, InstallMethod};
use crate::cli::Workspace;
use crate::core::effective::load_live_remote;
use crate::core::remote_config::RemoteUpdate;
use crate::core::update_state::{
    auto_update_enabled, read_update_state, version_newer, write_update_state, UpdateState,
};
use crate::daemon::resource_guard::ResourceGuard;
use anyhow::{bail, Context, Result};
use chrono::NaiveDate;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::Duration;

const REPO: &str = "kintsugi-tools/paperscope";
const CURRENT: &str = env!("CARGO_PKG_VERSION");
const SHADOW_HINT: &str =
    "restarted on an older binary; another copy may shadow it (see paper doctor)";
const SHORT: Duration = Duration::from_secs(10 * 60);
const CARGO_TIMEOUT: Duration = Duration::from_secs(60 * 60);

#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    Nothing(&'static str),
    Install { version: String },
}

/// Pure decision. Offline, then opted out, then not newer; a failed or
/// unavailable result for the same approved version today waits until tomorrow.
pub fn decide(
    current: &str,
    approved: Option<&RemoteUpdate>,
    enabled: bool,
    offline: bool,
    last: Option<&UpdateState>,
    today: NaiveDate,
) -> Action {
    if offline {
        return Action::Nothing("offline");
    }
    if !enabled {
        return Action::Nothing("opted_out");
    }
    let Some(approved) = approved.filter(|a| version_newer(&a.version, current)) else {
        return Action::Nothing("up_to_date");
    };
    if let Some(last) = last.filter(|l| l.approved.as_deref() == Some(approved.version.as_str())) {
        let same_day = chrono::DateTime::parse_from_rfc3339(&last.at)
            .is_ok_and(|at| at.with_timezone(&chrono::Local).date_naive() == today);
        match (same_day, last.outcome.as_str()) {
            (true, "failed") => return Action::Nothing("failed"),
            (true, "unavailable") => return Action::Nothing("unavailable"),
            _ => {}
        }
    }
    Action::Install {
        version: approved.version.clone(),
    }
}

fn method_name(method: &InstallMethod) -> &'static str {
    match method {
        InstallMethod::Homebrew => "brew",
        InstallMethod::Npm => "npm",
        InstallMethod::Cargo => "cargo",
        InstallMethod::ShellInstaller => "shell",
        InstallMethod::Unknown => "unknown",
    }
}

pub fn manual_command(method: &InstallMethod, version: &str) -> String {
    match method {
        InstallMethod::Homebrew => "brew upgrade kintsugi-tools/tap/paper".into(),
        InstallMethod::Npm => format!("npm install -g paperscope@{version}"),
        InstallMethod::Cargo => format!("cargo install paperscope --locked --version {version}"),
        InstallMethod::ShellInstaller => format!(
            "curl -fsSL https://github.com/{REPO}/releases/download/v{version}/paperscope-installer.sh | sh"
        ),
        InstallMethod::Unknown => "paper update".into(),
    }
}

/// Runs a program with null stdin; `env` pairs are added to the child.
#[async_trait::async_trait]
pub trait Runner: Send + Sync {
    async fn run(
        &self,
        program: &str,
        args: &[String],
        env: &[(&str, &str)],
        timeout: Duration,
    ) -> Result<Output>;
}

pub struct TokioRunner;

#[async_trait::async_trait]
impl Runner for TokioRunner {
    async fn run(
        &self,
        program: &str,
        args: &[String],
        env: &[(&str, &str)],
        timeout: Duration,
    ) -> Result<Output> {
        let child = tokio::process::Command::new(program)
            .args(args)
            .envs(env.iter().copied())
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("start {program}"))?;
        tokio::time::timeout(timeout, child.wait_with_output())
            .await
            .map_err(|_| anyhow::anyhow!("{program} timed out after {}s", timeout.as_secs()))?
            .with_context(|| format!("run {program}"))
    }
}

/// Not an error: nothing can be installed right now, retry tomorrow.
#[derive(Debug)]
struct Unavailable(String);
impl std::fmt::Display for Unavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Unavailable {}

fn truncate(s: &str) -> String {
    s.trim().chars().take(300).collect()
}

fn output_text(out: &Output) -> String {
    let err = String::from_utf8_lossy(&out.stderr);
    let text = if err.trim().is_empty() {
        String::from_utf8_lossy(&out.stdout)
    } else {
        err
    };
    truncate(&text)
}

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

/// Is `version` published to this method's source yet?
async fn available(
    method: &InstallMethod,
    version: &str,
    runner: &dyn Runner,
    client: &reqwest::Client,
) -> Result<bool> {
    match method {
        InstallMethod::Homebrew => {
            let out = runner
                .run(
                    "brew",
                    &args(&["info", "--json=v2", "kintsugi-tools/tap/paper"]),
                    &[],
                    SHORT,
                )
                .await?;
            if !out.status.success() {
                bail!("brew info failed: {}", output_text(&out));
            }
            let json: serde_json::Value = serde_json::from_slice(&out.stdout)?;
            Ok(json["formulae"][0]["versions"]["stable"].as_str() == Some(version))
        }
        InstallMethod::Npm => {
            let out = runner
                .run(
                    "npm",
                    &args(&["view", &format!("paperscope@{version}"), "version"]),
                    &[],
                    SHORT,
                )
                .await?;
            // npm view exits non-zero (E404) for an unpublished version.
            Ok(out.status.success() && !out.stdout.trim_ascii().is_empty())
        }
        InstallMethod::Cargo => {
            let body = client
                .get("https://index.crates.io/pa/pe/paperscope")
                .send()
                .await?
                .error_for_status()?
                .text()
                .await?;
            Ok(body.contains(&format!("\"vers\":\"{version}\"")))
        }
        InstallMethod::ShellInstaller => {
            let url = format!("https://github.com/{REPO}/releases/download/v{version}/sha256.sum");
            Ok(client.head(url).send().await?.status() == reqwest::StatusCode::OK)
        }
        InstallMethod::Unknown => Err(Unavailable("install method not recognised".into()).into()),
    }
}

/// Poll the resource guard every `every` for up to `max`.
async fn wait_idle(guard: &ResourceGuard, every: Duration, max: Duration) -> bool {
    let start = std::time::Instant::now();
    loop {
        if !guard.check_now().await {
            return true;
        }
        if start.elapsed() >= max {
            return false;
        }
        tokio::time::sleep(every).await;
    }
}

async fn run_checked(
    runner: &dyn Runner,
    program: &str,
    argv: &[String],
    env: &[(&str, &str)],
    timeout: Duration,
) -> Result<()> {
    let out = runner.run(program, argv, env, timeout).await?;
    if !out.status.success() {
        bail!(
            "{program} exited with {}: {}",
            out.status,
            output_text(&out)
        );
    }
    Ok(())
}

async fn install(
    method: &InstallMethod,
    version: &str,
    exe: &Path,
    runner: &dyn Runner,
    client: &reqwest::Client,
    guard: &ResourceGuard,
) -> Result<()> {
    match method {
        InstallMethod::Homebrew => {
            run_checked(
                runner,
                "brew",
                &args(&["upgrade", "kintsugi-tools/tap/paper"]),
                &[],
                SHORT,
            )
            .await
        }
        InstallMethod::Npm => {
            let spec = format!("paperscope@{version}");
            run_checked(runner, "npm", &args(&["install", "-g", &spec]), &[], SHORT).await
        }
        InstallMethod::Cargo => {
            let have = runner
                .run("which", &args(&["cargo"]), &[], SHORT)
                .await
                .is_ok_and(|o| o.status.success());
            if !have {
                return Err(Unavailable("Rust toolchain not found".into()).into());
            }
            if !wait_idle(
                guard,
                Duration::from_secs(300),
                Duration::from_secs(6 * 3600),
            )
            .await
            {
                return Err(Unavailable("machine never idle".into()).into());
            }
            let mut argv = args(&["install", "paperscope", "--locked", "--version", version]);
            let program = if cfg!(unix) {
                argv.splice(0..0, args(&["-n", "19", "cargo"]));
                "nice"
            } else {
                "cargo"
            };
            run_checked(runner, program, &argv, &[], CARGO_TIMEOUT).await
        }
        InstallMethod::ShellInstaller => {
            if cfg!(windows) {
                bail!("shell-installer updates are not automated on Windows");
            }
            install_shell(version, exe, runner, client).await
        }
        InstallMethod::Unknown => Err(Unavailable("install method not recognised".into()).into()),
    }
}

/// The installer verifies the archive checksums itself.
async fn install_shell(
    version: &str,
    exe: &Path,
    runner: &dyn Runner,
    client: &reqwest::Client,
) -> Result<()> {
    let url =
        format!("https://github.com/{REPO}/releases/download/v{version}/paperscope-installer.sh");
    let script = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await
        .context("download installer")?;
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("paperscope-installer.sh");
    std::fs::write(&path, script)?;
    std::fs::copy(exe, old_path(exe)).context("keep the current binary as .old")?;
    let script_arg = [path.to_string_lossy().into_owned()];
    run_checked(
        runner,
        "sh",
        &script_arg,
        &[("PAPER_NO_AUTOSTART", "1")],
        SHORT,
    )
    .await
}

fn old_path(exe: &Path) -> PathBuf {
    let mut s = exe.as_os_str().to_owned();
    s.push(".old");
    s.into()
}

struct Ctx<'a> {
    dir: &'a Path,
    method: &'a InstallMethod,
    current: &'a str,
    exe: &'a Path,
    enabled: bool,
    offline: bool,
    today: NaiveDate,
    runner: &'a dyn Runner,
    client: &'a reqwest::Client,
    guard: &'a ResourceGuard,
    restart: &'a (dyn Fn() -> Result<()> + Sync),
}

fn record(
    ctx: &Ctx<'_>,
    outcome: &str,
    approved: Option<&str>,
    manual_command: Option<String>,
    detail: Option<String>,
) {
    let state = UpdateState {
        outcome: outcome.into(),
        approved: approved.map(String::from),
        method: method_name(ctx.method).into(),
        manual_command,
        detail,
        at: chrono::Utc::now().to_rfc3339(),
    };
    if let Err(e) = write_update_state(ctx.dir, &state) {
        tracing::warn!("could not write update.state: {e}");
    }
}

/// Record `unavailable` for an `Unavailable` error, `failed` for anything else.
fn record_error(ctx: &Ctx<'_>, version: &str, e: &anyhow::Error) {
    let manual = Some(manual_command(ctx.method, version));
    match e.downcast_ref::<Unavailable>() {
        Some(u) => record(ctx, "unavailable", Some(version), manual, Some(u.0.clone())),
        None => {
            let detail = truncate(&format!("{e:#}"));
            tracing::warn!("auto-update to {version} failed: {detail}");
            record(ctx, "failed", Some(version), manual, Some(detail));
        }
    }
}

async fn run_with(ctx: &Ctx<'_>, approved: Option<&RemoteUpdate>) {
    let last = read_update_state(ctx.dir);
    let version = match decide(
        ctx.current,
        approved,
        ctx.enabled,
        ctx.offline,
        last.as_ref(),
        ctx.today,
    ) {
        Action::Install { version } => version,
        Action::Nothing(outcome) => {
            // A same-day failure and the "updated" notice stay as they are.
            let keep = matches!(outcome, "failed" | "unavailable")
                || (outcome == "up_to_date"
                    && last.as_ref().is_some_and(|l| l.outcome == "updated"));
            if !keep {
                record(
                    ctx,
                    outcome,
                    approved.map(|a| a.version.as_str()),
                    None,
                    None,
                );
            }
            return;
        }
    };
    match available(ctx.method, &version, ctx.runner, ctx.client).await {
        Ok(true) => {}
        Ok(false) => {
            let detail = format!("{version} is not published for this install method yet");
            let manual = Some(manual_command(ctx.method, &version));
            return record(ctx, "unavailable", Some(&version), manual, Some(detail));
        }
        Err(e) => return record_error(ctx, &version, &e),
    }
    if let Err(e) = install(
        ctx.method, &version, ctx.exe, ctx.runner, ctx.client, ctx.guard,
    )
    .await
    {
        return record_error(ctx, &version, &e);
    }
    record(
        ctx,
        "updated",
        Some(&version),
        None,
        Some("restarting".into()),
    );
    // Last action: the service manager may kill this very process.
    if let Err(e) = (ctx.restart)() {
        let e = anyhow::anyhow!("update installed but restart failed: {e:#}");
        record_error(ctx, &version, &e);
    }
}

/// Settle a "restarting" state left by the previous process.
fn check_restart(dir: &Path, current: &str, exe: &Path) {
    let Some(mut state) = read_update_state(dir)
        .filter(|s| s.outcome == "updated" && s.detail.as_deref() == Some("restarting"))
    else {
        return;
    };
    let old = old_path(exe);
    if state.approved.as_deref() == Some(current) {
        state.detail = None;
        let _ = std::fs::remove_file(&old);
    } else {
        state.outcome = "failed".into();
        state.detail = Some(SHADOW_HINT.into());
        if state.method == "shell" && old.exists() {
            let _ = std::fs::rename(&old, exe);
        }
    }
    state.at = chrono::Utc::now().to_rfc3339();
    let _ = write_update_state(dir, &state);
}

/// Once at daemon startup, before the daily loop.
pub(super) fn check_after_restart() {
    if let Ok(exe) = std::env::current_exe() {
        check_restart(&crate::core::paths::Paths::new().base_dir(), CURRENT, &exe);
    }
}

/// Daily entry point.
pub(super) async fn run(workspace: &Workspace) {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let today = chrono::Local::now().date_naive();
    let approved = load_live_remote(today).and_then(|r| r.update);
    let Ok(client) = reqwest::Client::builder()
        .user_agent(concat!("paper/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(30))
        .build()
    else {
        return;
    };
    let dir = workspace.paths.base_dir();
    let method = current_install_method();
    let ctx = Ctx {
        dir: &dir,
        method: &method,
        current: CURRENT,
        exe: &exe,
        enabled: auto_update_enabled(&workspace.config),
        offline: crate::bootstrap::is_offline(),
        today,
        runner: &TokioRunner,
        client: &client,
        guard: &ResourceGuard::new(),
        restart: &crate::cli::daemon::restart_service,
    };
    run_with(&ctx, approved.as_ref()).await;
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;
    use std::sync::Mutex;

    /// Replies by `(program, first arg or "*")`: (exit code, stdout, stderr).
    type Reply = (&'static str, &'static str, i32, &'static str, &'static str);
    struct Fake {
        replies: Vec<Reply>,
        calls: Mutex<Vec<String>>,
    }
    impl Fake {
        fn new(replies: Vec<Reply>) -> Self {
            Fake {
                replies,
                calls: Mutex::new(vec![]),
            }
        }
        fn count(&self) -> usize {
            self.calls.lock().unwrap().len()
        }
    }
    #[async_trait::async_trait]
    impl Runner for Fake {
        async fn run(
            &self,
            program: &str,
            argv: &[String],
            _env: &[(&str, &str)],
            _t: Duration,
        ) -> Result<Output> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("{program} {}", argv.join(" ")));
            let first = argv.first().map_or("", String::as_str);
            let (_, _, code, out, err) = self
                .replies
                .iter()
                .find(|r| r.0 == program && (r.1 == "*" || r.1 == first))
                .copied()
                .unwrap_or((program, "*", 127, "", "not scripted"));
            Ok(Output {
                status: std::process::ExitStatus::from_raw(code << 8),
                stdout: out.as_bytes().to_vec(),
                stderr: err.as_bytes().to_vec(),
            })
        }
    }

    fn today() -> NaiveDate {
        chrono::Local::now().date_naive()
    }
    fn upd(v: &str) -> RemoteUpdate {
        RemoteUpdate {
            version: v.into(),
            notes: None,
        }
    }
    fn state(outcome: &str, approved: &str, at: String) -> UpdateState {
        UpdateState {
            outcome: outcome.into(),
            approved: Some(approved.into()),
            method: "npm".into(),
            manual_command: None,
            detail: None,
            at,
        }
    }

    struct Rig {
        dir: tempfile::TempDir,
        guard: ResourceGuard,
        client: reqwest::Client,
        restarts: Mutex<u32>,
    }
    impl Rig {
        fn new() -> Self {
            Rig {
                dir: tempfile::tempdir().unwrap(),
                guard: ResourceGuard::new(),
                client: reqwest::Client::new(),
                restarts: Mutex::new(0),
            }
        }
        async fn go(&self, runner: &Fake, restart_ok: bool) {
            let exe = self.dir.path().join("paper");
            let restart = || -> Result<()> {
                *self.restarts.lock().unwrap() += 1;
                if restart_ok {
                    Ok(())
                } else {
                    bail!("no service")
                }
            };
            let ctx = Ctx {
                dir: self.dir.path(),
                method: &InstallMethod::Npm,
                current: "0.4.1",
                exe: &exe,
                enabled: true,
                offline: false,
                today: today(),
                runner,
                client: &self.client,
                guard: &self.guard,
                restart: &restart,
            };
            run_with(&ctx, Some(&upd("0.4.2"))).await;
        }
        fn st(&self) -> UpdateState {
            read_update_state(self.dir.path()).unwrap()
        }
    }

    #[test]
    fn decide_never_downgrades_or_reinstalls() {
        for approved in ["0.4.1", "0.4.0", "0.3.9"] {
            assert_eq!(
                decide("0.4.1", Some(&upd(approved)), true, false, None, today()),
                Action::Nothing("up_to_date")
            );
        }
        assert_eq!(
            decide("0.4.1", None, true, false, None, today()),
            Action::Nothing("up_to_date")
        );
        let a = upd("0.4.2");
        let install = Action::Install {
            version: "0.4.2".into(),
        };
        assert_eq!(
            decide("0.4.1", Some(&a), true, false, None, today()),
            install
        );
        // Yesterday's failure retries; today's does not; another version does.
        let now = chrono::Utc::now().to_rfc3339();
        let old = (chrono::Utc::now() - chrono::Duration::days(2)).to_rfc3339();
        let d = |s: UpdateState| decide("0.4.1", Some(&a), true, false, Some(&s), today());
        assert_eq!(d(state("failed", "0.4.2", old)), install);
        assert_eq!(
            d(state("failed", "0.4.2", now.clone())),
            Action::Nothing("failed")
        );
        assert_eq!(
            d(state("unavailable", "0.4.2", now.clone())),
            Action::Nothing("unavailable")
        );
        assert_eq!(d(state("failed", "0.4.0", now)), install);
    }

    #[test]
    fn decide_respects_offline_and_opt_out() {
        let a = upd("0.4.2");
        let d = |enabled, offline| decide("0.4.1", Some(&a), enabled, offline, None, today());
        assert_eq!(d(true, true), Action::Nothing("offline"));
        assert_eq!(d(false, false), Action::Nothing("opted_out"));
        assert_eq!(d(false, true), Action::Nothing("offline"));
    }

    #[test]
    fn manual_commands_per_method() {
        let m = |method| manual_command(&method, "1.2.3");
        assert_eq!(
            m(InstallMethod::Homebrew),
            "brew upgrade kintsugi-tools/tap/paper"
        );
        assert_eq!(m(InstallMethod::Npm), "npm install -g paperscope@1.2.3");
        assert_eq!(
            m(InstallMethod::Cargo),
            "cargo install paperscope --locked --version 1.2.3"
        );
        assert_eq!(
            m(InstallMethod::ShellInstaller),
            "curl -fsSL https://github.com/kintsugi-tools/paperscope/releases/download/v1.2.3/paperscope-installer.sh | sh"
        );
        assert_eq!(m(InstallMethod::Unknown), "paper update");
    }

    #[tokio::test]
    async fn failed_command_records_manual_command_and_does_not_retry_today() {
        let rig = Rig::new();
        let fake = Fake::new(vec![
            ("npm", "view", 0, "0.4.2\n", ""),
            ("npm", "install", 243, "", "EACCES: permission denied"),
        ]);
        rig.go(&fake, true).await;
        let st = rig.st();
        assert_eq!(st.outcome, "failed");
        assert_eq!(
            st.manual_command.as_deref(),
            Some("npm install -g paperscope@0.4.2")
        );
        assert!(st.detail.unwrap().contains("EACCES"));
        assert_eq!(*rig.restarts.lock().unwrap(), 0);
        let calls = fake.count();
        rig.go(&fake, true).await;
        assert_eq!(fake.count(), calls, "no retry today");
        assert_eq!(rig.st().outcome, "failed");
    }

    #[tokio::test]
    async fn cargo_without_toolchain_is_unavailable_not_failed() {
        let rig = Rig::new();
        let fake = Fake::new(vec![("which", "*", 1, "", "")]);
        let exe = rig.dir.path().join("paper");
        let err = install(
            &InstallMethod::Cargo,
            "0.4.2",
            &exe,
            &fake,
            &rig.client,
            &rig.guard,
        )
        .await
        .unwrap_err();
        assert_eq!(
            err.downcast_ref::<Unavailable>().unwrap().0,
            "Rust toolchain not found"
        );
        assert_eq!(fake.count(), 1, "cargo itself never ran");
    }

    #[tokio::test]
    async fn unpublished_version_is_unavailable() {
        let rig = Rig::new();
        let fake = Fake::new(vec![("npm", "view", 1, "", "E404 not found")]);
        rig.go(&fake, true).await;
        let st = rig.st();
        assert_eq!(st.outcome, "unavailable");
        assert_eq!(st.approved.as_deref(), Some("0.4.2"));
        assert_eq!(fake.count(), 1, "only the availability probe ran");
        assert_eq!(*rig.restarts.lock().unwrap(), 0);
    }

    fn npm_ok() -> Fake {
        Fake::new(vec![("npm", "*", 0, "0.4.2\n", "")])
    }

    #[tokio::test]
    async fn successful_install_records_restarting_then_updated_on_matching_start() {
        let rig = Rig::new();
        rig.go(&npm_ok(), true).await;
        let st = rig.st();
        assert_eq!(
            (st.outcome.as_str(), st.detail.as_deref()),
            ("updated", Some("restarting"))
        );
        assert_eq!(*rig.restarts.lock().unwrap(), 1);
        let exe = rig.dir.path().join("paper");
        std::fs::write(old_path(&exe), "old").unwrap();
        check_restart(rig.dir.path(), "0.4.2", &exe);
        let st = rig.st();
        assert_eq!((st.outcome.as_str(), st.detail), ("updated", None));
        assert!(!old_path(&exe).exists());
    }

    #[tokio::test]
    async fn restart_failure_records_failed_with_manual_command() {
        let rig = Rig::new();
        rig.go(&npm_ok(), false).await;
        let st = rig.st();
        assert_eq!(st.outcome, "failed");
        assert!(st.manual_command.is_some());
    }

    #[tokio::test]
    async fn post_restart_version_mismatch_is_reported_once() {
        let rig = Rig::new();
        rig.go(&npm_ok(), true).await;
        let exe = rig.dir.path().join("paper");
        check_restart(rig.dir.path(), "0.4.1", &exe);
        let st = rig.st();
        assert_eq!(st.outcome, "failed");
        assert_eq!(st.detail.as_deref(), Some(SHADOW_HINT));
        check_restart(rig.dir.path(), "0.4.1", &exe);
        assert_eq!(rig.st(), st, "second start leaves it alone");
    }
}
