# Install-time provisioning across every distribution channel (Paper / `paperscope`)

Scope note: this document treats **registration** work (service unit, MCP config, autostart entry — small, local, synchronous, milliseconds) separately from **provisioning** work (model download ~500 MB–1.7 GB, first index build — large, networked, minutes). Almost every channel error in this space comes from conflating the two. Local baseline for comparison is `src/bootstrap.rs` in this repo (first-run bootstrap from `main`, `STATE_VERSION = "1"`, `bootstrap.state` / `bootstrap.lock`, `PAPER_NO_AUTOSTART` / `PAPER_OFFLINE` / `CI` / `PAPER_HOME` / `PAPER_FORCE_BOOTSTRAP`).

---

## Q1. Which channels actually have a post-install hook, and what do their policies say?

### Takeaway

Only four of Paper's channels give a real post-install hook: **Homebrew formula** (`post_install_steps`, sandboxed, no arbitrary Ruby), **Scoop** (`post_install` PowerShell snippet), **npm** (`postinstall` JS), and **Chocolatey** (`chocolateyInstall.ps1`, not verified in this pass). `cargo install`, cargo-dist's shell/PowerShell installers, winget, GitHub Releases and Docker have **no hook at all**, and winget explicitly refuses script-based installers. Two of the four hooks (Homebrew, winget validation) are constrained enough that a 500 MB download inside them is a policy or validation failure, not just bad manners.

### Feasibility matrix

| Channel | Hook? | What the hook can actually do | Policy / constraint that bites | Verdict for Paper |
|---|---|---|---|---|
| `cargo install paperscope` | **None.** A post-install hook is only an open feature request | Nothing; cargo copies the binary to `~/.cargo/bin` and exits | No hook exists — [cargo#11539](https://github.com/rust-lang/cargo/issues/11539) | First-run only. Make first run non-blocking (Q2). |
| cargo-dist shell (`curl \| sh`) | **None documented.** Installer is a "thin wrapper" that downloads and unpacks an archive | Runs as a shell script on the user's machine, so any shell command *Paper's own script* contains would run | Requires network at install time by design; installer docs describe only fetch+unpack — [dist installers](https://axodotdev.github.io/cargo-dist/book/installers/index.html) | Highest-leverage place to add `paper setup --detach` **if** Paper controls the template (Q2, Q7). |
| cargo-dist PowerShell (`irm \| iex`) | **None documented** | Same as above, in PowerShell | Same — [dist installers](https://axodotdev.github.io/cargo-dist/book/installers/index.html) | Same as shell. |
| Homebrew formula (tap) | **Yes**, but reshaped: `post_install_steps` | Only a fixed vocabulary of file/mkdir/symlink/system steps with **literal** arguments; cannot run arbitrary Ruby | "Formulae in all taps must represent post-install work with `post_install_steps`; new `post_install` methods are rejected… A `post_install_steps` block may only contain the supported step calls with literal arguments. It cannot call the wider formula DSL or arbitrary Ruby code. Homebrew executes the steps with the same post-install sandbox policy." — [Formula Cookbook](https://docs.brew.sh/Formula-Cookbook) | At most a marker file or `brew postinstall`-replayable step. **Never** the model download. |
| Homebrew cask | Cask stanzas differ (`postflight`-style), not researched here | — | "Proprietary or platform-specific binary-only software belongs in a cask"; "A core formula must not depend on a cask, proprietary software or a runtime step that automatically installs either one" — [Acceptable Formulae](https://docs.brew.sh/Acceptable-Formulae) | Paper ships a formula in its own tap (prebuilt binaries) — cask not needed; see Gaps. |
| winget | **None.** Manifest fields are installer type + silent switches | Nothing. `zip`/`portable` installers are unpacked; `.exe` installers are run with silent switches | "At this time installers must be MSIX, MSI, APPX, MSIXBundle, APPXBundle, or .exe application installers… **Script-based installers are not currently supported.**" — [winget-pkgs README](https://github.com/microsoft/winget-pkgs). Validation: "The validation infrastructure runs installers as a standard (non-elevated) user by default"; "Interactive-Only-Installer — The installer requires user interaction and cannot run silently. This is a blocking issue"; a listed common failure cause is "The application is a service or background process without a suitable executable" — [ValidationFailureGuide](https://github.com/microsoft/winget-pkgs/blob/master/doc/ValidationFailureGuide.md) | Ship a `zip`/`portable` package (no admin, no installer side effects). Provisioning must be first-run. |
| Scoop | **Yes**: `post_install` | PowerShell lines, with `$dir`, `$persist_dir`, `$version` in scope; also `pre_install`, `pre_uninstall`, `post_uninstall` | "`post_install`: A one-line string, or array of strings, of the commands to be executed after an application is installed. These can use variables like `$dir`, `$persist_dir`, and `$version`." — [Scoop wiki, App Manifests](https://github.com/ScoopInstaller/Scoop/wiki/App-Manifests). `persist` links state out of the versioned dir so upgrades do not wipe it — [Persistent data](https://github.com/ScoopInstaller/Scoop/wiki/Persistent-data) | Best Windows hook available: `post_install: ["& \"$dir\\paper.exe\" setup --detach --no-service"]` (milliseconds, no network). |
| Chocolatey | **Yes** (`chocolateyInstall.ps1`), not verified in this pass | Arbitrary PowerShell, elevated by default | — | Skip: Paper's constraint is "no admin rights", and chocolatey runs elevated. |
| npm (cargo-dist-generated package) | **Yes**: `postinstall` | A node script; the generated package already uses `postinstall`-phase machinery to fetch the binary | "These also run when you run `npm install -g <pkg-name>`" — [npm scripts](https://docs.npmjs.com/cli/v10/using-npm/scripts). Scripts "are run by passing the line as a script argument to `sh`"; "If the script exits with a code other than 0, then this will abort the process"; npm's own guidance: "If the failure is minor or only will prevent some optional features, then it's better to just print a warning and exit successfully" | Use it, but only for `setup --detach`, always `exit 0`, and never assume it ran (CI installs with scripts disabled). |
| GitHub Releases tarball | **None** — pure download | — | — | First-run. |
| Docker | **None** at pull time; only `ENTRYPOINT`/`CMD` at container start | The entrypoint *is* the daemon in most designs | — | Bake models into the image or use a volume; do not download at `docker run` without a volume. |
| Claude Code plugin / marketplace | **No scripts are run.** Registration is declarative | MCP server definition shipped in the plugin `.mcp.json`; Claude starts it per session | "Claude Code loads `.mcp.json` at the plugin root first, then each declared shape in order. A server name declared later replaces an earlier one." — [Plugin manifest reference](https://code.claude.com/docs/en/plugins/manifest-reference) | Nothing to hook. This channel *is* the "five MCP sessions at once" scenario (Q4). |
| MCP registry | Declarative metadata only | Registry records how to launch the server | — (no policy text gathered; see Gaps) | Nothing to hook. |
| Source build (`cargo build --release`) | **None** | — | — | First-run. |

### Cited Findings

- cargo-dist's installer set is exactly: shell, powershell, npm, homebrew, msi; "the term 'installer' [is used] very loosely — if it's fancier than a tarball, it's an installer to us"; the docs describe no post-install extension point. — [dist installers index](https://axodotdev.github.io/cargo-dist/book/installers/index.html)
- "Fetching installers are thin wrappers which detect the user's current platform and download and unpack the appropriate archive from a server. In exchange for requiring a well-defined Artifact URL and an internet connection at install-time…" — [dist installers index](https://axodotdev.github.io/cargo-dist/book/installers/index.html)
- cargo-dist's npm package "will fetch your prebuilt archives and install your binaries to node_modules, exposing them as commands ('bins')", using "a few module dependencies which are used to fetch your binary after the package itself is installed", locked with `npm-shrinkwrap.json`. — [dist npm installer](https://axodotdev.github.io/cargo-dist/book/installers/npm.html)
- `cargo install` has no post-install hook; the request for one is still an open issue. — [cargo#11539](https://github.com/rust-lang/cargo/issues/11539)
- Homebrew has *moved* formula post-install to a restricted, declarative step DSL and rejects new `post_install` methods in *all* taps (not just core). — [Formula Cookbook](https://docs.brew.sh/Formula-Cookbook)
- Homebrew's acceptability bar for post-install work: "Software that requires extensive manual pre-installation or post-installation steps is not suitable for `homebrew/core` unless those steps can be made reliable and safe." — [Acceptable Formulae](https://docs.brew.sh/Acceptable-Formulae)
- Homebrew has an old, still-unresolved discussion about whether `post_install` may write into `$HOME`; the answers are inconsistent ("Several discussion threads imply that the post_install action within a formula can be used to install files in a user's home directory"). — [Homebrew discussion #3676](https://github.com/orgs/Homebrew/discussions/3676)
- winget supports `EXE`, `ZIP`, `INNO`, `NULLSOFT`, `MSI`, `WIX`, `APPX`, `MSIX`, `BURN`, `PORTABLE`, `FONT` — silent switches are supplied by the manifest, and there is no post-install verb. — [Use WinGet](https://learn.microsoft.com/en-us/windows/package-manager/winget)
- winget-pkgs will not accept script-based installers at all. — [winget-pkgs README](https://github.com/microsoft/winget-pkgs)
- winget's validation pipeline runs silently, non-elevated, and treats non-silent installers as blocking failures; it also flags "service or background process" apps. — [ValidationFailureGuide](https://github.com/microsoft/winget-pkgs/blob/master/doc/ValidationFailureGuide.md)
- winget requires installer URLs to come straight from the publisher, no redirectors/CDNs: "Testing has determined a redirector has been used. This is not allowed because the Windows Package Manager policies require that the InstallerUrl comes directly from the ISV's release location." — [Submit your manifest](https://learn.microsoft.com/en-us/windows/package-manager/package/repository)
- Scoop's `post_install` runs arbitrary PowerShell lines with `$dir`/`$persist_dir` in scope; `persist` exists precisely so app state survives `scoop update`. — [App Manifests](https://github.com/ScoopInstaller/Scoop/wiki/App-Manifests), [Persistent data](https://github.com/ScoopInstaller/Scoop/wiki/Persistent-data)
- npm runs `preinstall` → `install` → `postinstall` for `npm install`, "also … when you run `npm install -g <pkg-name>`", and a non-zero exit "will abort the process". — [npm scripts](https://docs.npmjs.com/cli/v10/using-npm/scripts)
- npm@7+ runs some lifecycle scripts with output hidden by default ("these scripts run in the background. To see the output, run with: `--foreground-scripts`"), which means a silent 500 MB download would be invisible in install logs. — [npm scripts](https://docs.npmjs.com/cli/v10/using-npm/scripts)
- Claude Code plugins register MCP servers declaratively and start them when enabled; no install script runs. — [Plugin manifest reference](https://code.claude.com/docs/en/plugins/manifest-reference); corroborated by [Shiplight](https://www.shiplight.ai/blog/claude-code-plugins) ("MCP servers in `.mcp.json` at the plugin root, so an install can wire Claude to external tools without a separate registration step") and [hidekazu-konishi](https://hidekazu-konishi.com/entry/claude_code_plugins_complete_guide.html) ("they start automatically when the plugin is enabled").

### Inferences

- The claim in `src/bootstrap.rs` — "The installer cannot set Paper up: `install.sh`, `cargo install` and the Claude plugin have no common post-install hook" — is **correct about the absence of a universal hook but wrong as a design conclusion**. A hook exists on 4 of 13 channels, and every one of them can run the *same* 50 ms verb; the remaining 9 need a first-run path anyway. So the design is not "hook or no hook", it is "one idempotent verb, many triggers, one always-present fallback".
- On winget specifically, any design where the installer itself downloads models fails validation twice over: installers must be silent and non-interactive, and validation runs non-elevated in a sandbox where a multi-hundred-MB download will time out or be flagged. winget must ship a `portable`/`zip` and do nothing else.
- On Homebrew, the 2026 move to `post_install_steps` with literal arguments and no arbitrary Ruby effectively kills "run the binary during install" as a pattern for formulae. Anything beyond `mkdir_p`/`touch`/`symlink`/a sandboxed command will be rejected in audit, and network access inside the post-install sandbox cannot be relied on. The idiomatic Homebrew answer for "run this once" is `caveats`, not `post_install`.
- npm's own best-practice text ("If the failure is minor or only will prevent some optional features, then it's better to just print a warning and exit successfully") is an explicit licence — and an implicit instruction — to make `postinstall` fire-and-forget and always exit 0.
- Because `curl … | sh` feeds the *script itself* on stdin, a generated shell installer cannot safely prompt; anything interactive must read `/dev/tty`. Therefore the install-time hook must be non-interactive by construction, which also makes it safe to run from `irm | iex` and from CI.

### Gaps

- Whether Homebrew's post-install sandbox permits network access or writes to `$HOME` is not stated in the Formula Cookbook text I retrieved; discussion #3676 suggests ambiguity. No authoritative statement found.
- Homebrew **cask** post-install semantics (`postflight`/`preflight`) were not researched; cargo-dist has an open request for a macOS cask ([dist installers index](https://axodotdev.github.io/cargo-dist/book/installers/index.html) lists "macOS cask" under future installers), so Paper does not currently have one.
- Chocolatey's `chocolateyInstall.ps1` semantics and moderator policy were not verified in this pass (no primary source fetched). Treat the matrix row as "hook exists, details unverified".
- MCP registry policy (whether a registry entry may point at a server that self-provisions) was not researched — no source gathered.
- Docker: no primary source gathered on recommended model-baking vs volume patterns; the row is inference, not citation.
- Whether cargo-dist supports user-supplied installer templates ("custom installers") is **unverified**; I could not load a docs page for it. This matters for Q2/Q7.

---

## Q2. When no hook exists, what is the best pattern — installer-invoked `setup`, first-run bootstrap, or both?

### Takeaway

Both, but with a hard split of responsibilities: **`paper setup` is the only implementation**, and it is *bounded to registration + scheduling* (target: under ~150 ms, no network). Installers that can run it, run it. Everything else — including every channel that has no hook — falls through to a first-run path that only *spawns* `paper setup --detach` and returns immediately. The 500 MB problem is not solved by moving it earlier; it is solved by never doing it in the foreground of a user-facing command and never holding a lock a user command waits on.

### Cited Findings

- cargo-dist installers are network-dependent fetch+unpack wrappers, so "install" is already a network operation on those channels — adding one more local command at the end costs nothing user-visible. — [dist installers index](https://axodotdev.github.io/cargo-dist/book/installers/index.html)
- rustup is the strongest precedent for "heavy work at install time, blocking, with an opt-out": `rustup-init` takes `-y` (disable confirmation), `--default-toolchain`, `--profile minimal`, `--no-modify-path`, and `--default-toolchain none` prints "info: skipping toolchain installation". — [rustup book, other installation methods](https://rust-lang.github.io/rustup/installation/other.html), [Rust forum: silent toolchain install](https://users.rust-lang.org/t/solved-is-there-a-way-to-install-rust-toolchain-silently/25790)
- rustup later had to *retrofit* an escape hatch for surprise downloads at *usage* time (`rustup set auto-install disable`, 1.28.2+), because implicit downloads on an ordinary command broke CI — direct evidence that implicit heavy work on first use is a real hazard. — [Rust forum: toolchain list downloads toolchains](https://users.rust-lang.org/t/is-it-normal-behavior-that-the-rustup-toolchain-list-command-will-download-and-install-toolchains/130168)
- Ollama's Linux installer does registration eagerly and correctly encompasses service creation: ">>> Creating ollama systemd service… >>> Enabling and starting ollama service…"; upgrades are "run the install script again". — [CIQ docs quoting upstream installer output](https://docs.ciq.com/rlc-ai/user-guides/ollama), [Ollama Linux docs](https://docs.ollama.com/linux)
- Ollama's lifecycle is also the cautionary tale: users report "the script overwrote them everytime it updated", i.e. re-running an installer on every upgrade clobbers user configuration, and users resort to `systemctl edit` drop-ins to survive. — [ollama#16191](https://github.com/ollama/ollama/issues/16191)
- Syncthing does the opposite: the package ships a systemd user unit but the *user* must copy and `systemctl --user enable` it; boot-without-login needs `loginctl enable-linger`. — [Syncthing: Starting Syncthing Automatically](https://docs.syncthing.net/users/autostart.html)
- Tailscale shows the "service is registered by the installer" norm on Windows: "On Windows, `tailscaled` runs as a Windows service named 'Tailscale'." — [tailscaled daemon](https://tailscale.com/docs/reference/tailscaled); its MSI exposes `TS_NOLAUNCH`, `TS_INSTALLUPDATES`, `TS_CHECKUPDATES` switches — [Tailscale MSI docs](https://tailscale.com/docs/install/windows/msi), [AppDeployNews packaging notes](https://www.appdeploynews.com/app-tips/tailscale-tailscale-1-68-2)
- The decisive precedent for a `setup` verb: Scoop-installed Tailscale has **no service**, because Scoop only extracts a zip, and the fix was for the app to expose its own installer verb (`tailscale-ipn /install`; an undocumented `tailscaled` subcommand installs the service). — [Scoop Extras #13460](https://github.com/ScoopInstaller/Extras/issues/13460)
- winget cannot run a script, so "installer calls setup" is impossible there; the package must be a zip/portable — [winget-pkgs README](https://github.com/microsoft/winget-pkgs).
- npm explicitly tolerates partial failure of install scripts and runs them with hidden output by default — [npm scripts](https://docs.npmjs.com/cli/v10/using-npm/scripts). So the npm hook can never be the sole trigger.

### Inferences

- The correct split, stated as a rule: **installers may only do work that is safe to do twice, safe to skip, and safe to fail.** Service registration and marker-file creation qualify. Model download does not (it is large, networked, resumable-only-with-effort, and on winget's validator it is a timeout).
- Therefore `paper setup` should be defined as *"make the system consistent with the intent to run Paper"*, not *"download models"*. Model download belongs to the daemon it starts.
- The upgrade-clobbering failure mode in Ollama is avoidable by never rewriting files that exist: `setup` should write the unit/config only if absent, and emit a `.sample` on conflict — otherwise every `brew upgrade`/`npm -g` reinstall resets user customisation.
- Syncthing's model (ship the unit, document `systemctl --user enable`) is the right *fallback* for users who refuse autostart, and it is already Paper's `daemon-control` escape hatch.

### Gaps

- No primary source found for how **uv**, **Deno** or **ripgrep-all** handle this; their installers were not inspected in this pass. (ripgrep-all is presumed "binary only, no setup" but that is not verified.)
- No source found on how any mainstream cargo-dist project adds a post-install step to the *generated* shell installer (patching generated output vs. custom templates). This is the single most important unknown for the curl channel.

---

## Q3. Detaching a child process correctly on each OS

### Takeaway

Unix: `fork` → `setsid` → `fork` → `chdir("/")` → redirect all three stdio to a log or `/dev/null` → `exec`. The double fork is mandatory because `setsid` fails when the caller is already a process-group leader. Windows: `CreateProcess` with `DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP`, plus `CREATE_BREAKAWAY_FROM_JOB` **only if** the parent's job allows breakaway — and a scheduled-task fallback for when it doesn't, because the installer frequently runs inside another application's job object.

### Cited Findings

- `setsid()` "creates a new session if the calling process is not a process group leader"; it fails with `EPERM` when "the process group ID of any process equals the PID of the calling process" — i.e. when the caller is a group leader, which is exactly what a freshly forked child of the shell may be. — [setsid(2)](https://man7.org/linux/man-pages/man2/setsid.2.html)
- The canonical daemonisation sequence (fork, `setsid`, `chdir("/")`, redirect stdio to `/dev/null`) is codified in `daemon(3)`. — [daemon(3)](https://man7.org/linux/man-pages/man3/daemon.3.html)
- `CREATE_BREAKAWAY_FROM_JOB` (0x01000000): "The child processes of a process associated with a job are not associated with the job. If the calling process is not associated with a job, this constant has no effect. **If the calling process is associated with a job, the job must set the `JOB_OBJECT_LIMIT_BREAKAWAY_OK` limit.**" — [Process Creation Flags](https://learn.microsoft.com/en-us/windows/win32/procthread/process-creation-flags)
- `DETACHED_PROCESS` (0x00000008): "For console processes, the new process does not inherit its parent's console"; it cannot be combined with `CREATE_NEW_CONSOLE`, and `CREATE_NO_WINDOW` "is ignored … if it is used with either `CREATE_NEW_CONSOLE` or `DETACHED_PROCESS`". — same page
- `CREATE_NEW_PROCESS_GROUP` (0x00000200): "The new process is the root process of a new process group… If this flag is specified, CTRL+C signals will be disabled for all processes within the new process group." — same page
- Windows file-locking fact that makes detach-vs-upgrade hard: a running executable **can** be renamed but **cannot** be deleted or overwritten. — [Super User](https://superuser.com/questions/488127/why-can-i-rename-a-running-executable-but-not-delete-it); corroborated by [Qt forum](https://forum.qt.io/topic/102668/update-current-application-by-replace-the-exe-and-delete-old-one) ("Windows does allow renaming while the .exe is open/running … But the .exe is still running, so … remove() 'old' will always fail")

### Inferences

- A single-fork `Command::spawn` from `paper search` is not enough on Unix: the child inherits the session and the controlling terminal, so a `Ctrl-C` (SIGINT to the foreground process group) or terminal hangup reaches it, and the MCP server's stdio pipes stay open. Use the double fork.
- `CREATE_BREAKAWAY_FROM_JOB` must be **opportunistic, not assumed**: because the parent's job must have set `JOB_OBJECT_LIMIT_BREAKAWAY_OK`, a VS Code/IntelliJ-spawned installer, an MSI custom action, or a CI runner job will make `CreateProcess` fail with `ERROR_ACCESS_DENIED`. The robust sequence is: try flags → on failure, retry without breakaway → if the child then cannot outlive the parent (job has `KILL_ON_JOB_CLOSE`), fall back to registering a one-shot per-user scheduled task (`schtasks /create /tn PaperSetup /sc once … /it`) or a `Run` key entry, which the OS launches outside the job.
- Corollary: on Windows, "detached process" is not a durability guarantee at all. Durability must come from the service/startup registration (Scheduled Task or `HKCU\…\Run`), which is also what survives reboot. The detached process is only an optimisation to avoid waiting.
- Paper's daemon should be a *registered* user-scope unit (launchd `LaunchAgent`, `systemd --user`, Windows Scheduled Task at logon) rather than a bare detached process — matching Tailscale (service) and Syncthing (unit + enable), and avoiding the job-object problem entirely on the next start.

### Gaps

- Nested job objects (Windows 8+ allows a process to belong to multiple jobs) were not confirmed against a primary Microsoft source in this pass. If true, it means a child can be in the installer's job *and* its own, which changes the failure analysis but not the recommendation.
- No source gathered on whether `launchd` reaps or throttles a `LaunchAgent` that exits immediately on start (relevant for a "start then hand off" unit); needs testing, not citation.

---

## Q4. Idempotency and concurrency — marker files, locks, and five MCP sessions at once

### Takeaway

Marker files alone are not enough: they lie after a crash, after `PAPER_HOME` restore, and after partial model download. The safe design is **OS-level advisory locks per phase (`flock` / `LockFileEx`) + a phase journal with per-phase completion records + physical verification of the artefact itself**. And critically: user-facing commands (especially `paper serve`) must use **`try_lock` and never block** — they should serve keyword-only results while provisioning happens elsewhere.

### Cited Findings

- Paper's current implementation uses `File::create_new` as a mutex, a 15-minute `LOCK_STALE` takeover, a `state` file with `version=1`, and a 1-hour retry backoff after a failed service install (`src/bootstrap.rs`, `STATE_VERSION`, `RETRY_AFTER`, `LOCK_STALE`). It saves state via write-temp-then-`rename` (crash-safe replace) but the lock is a bare file whose stale takeover is a `remove_file` followed by `create_new`.
- Paper runs `maybe_autostart` before dispatch for any command outside `SKIP_COMMANDS`, and `SKIP_COMMANDS` does **not** include `serve`; so an editor spawning several MCP sessions runs `ensure_service()` (launchctl/systemctl work) in each, serialised only by that lock. — `src/bootstrap.rs`
- npm's postinstall best-practice licence ("print a warning and exit successfully") shows the ecosystem's expectation that install-time work is advisory. — [npm scripts](https://docs.npmjs.com/cli/v10/using-npm/scripts)
- Claude Code loads plugin MCP servers at session start and starts them automatically; multiple sessions mean multiple `paper serve` processes, and "a server name declared later replaces an earlier one" implies config-merge semantics rather than a single owner. — [Plugin manifest reference](https://code.claude.com/docs/en/plugins/manifest-reference), [hidekazu-konishi](https://hidekazu-konishi.com/entry/claude_code_plugins_complete_guide.html)
- Precedent for state-outside-the-install-dir: Scoop's `persist` links state to `~/scoop/persist/<app>` so `scoop update` cannot wipe it. — [Persistent data](https://github.com/ScoopInstaller/Scoop/wiki/Persistent-data)

### Inferences

- The current stale-lock takeover is racy (TOCTOU): two processes can both observe staleness, both `remove_file`, and both `create_new` succeed in sequence, producing two concurrent setup runs. `flock`/`LockFileEx` avoids this because the kernel owns the lock and releases it on process death — no staleness heuristic needed.
- Crash-safety rule: the lock must be held for the *phase*, and the phase's completion record written **before** releasing it, so a killed run is either "not started" or "known done", never "half done, believed done". A model file that exists but is truncated must be detected by digest/size check, not by the marker.
- `paper serve` must not take the setup lock at all. It should: (a) attempt `try_lock`; (b) if held, log one line and continue serving keyword-only; (c) never wait for model download. Otherwise N editor windows serialise on a network operation and every one of them appears hung. That is the concrete fix for "editor starts five MCP sessions at once".
- A per-phase lock also fixes a subtler upgrade bug: `binary_version` changes must not invalidate the `services` phase (nothing to redo) but may invalidate `integrations` (new MCP config schema). One global "done" flag cannot express that; a journal can.
- Recommend `fd-lock` or `fs2` for the cross-platform primitive rather than hand-rolled `create_new`; both map to `flock(2)`/`LockFileEx`. (Library choice is design inference; no doc fetched.)

### Gaps

- No primary source fetched for `fd-lock`/`fs2`/`rustix` API specifics; the `flock`→`LockFileEx` mapping is asserted from the crates' documented purpose, not quoted.
- No data found on real-world MCP-session concurrency counts (how many `paper serve` processes an editor actually spawns); this should be measured, not assumed.

---

## Q5. Upgrades: don't redo work; don't break the running daemon

### Takeaway

Split state by owner: **models and index live under `PAPER_HOME` keyed by pinned model revision + SHA-256, never under the binary's directory**, so a binary upgrade cannot touch them. **Registration state lives in the setup journal keyed by `binary_version`**, so an upgrade re-runs only phases whose schema changed. On Windows, the daemon holds its own image open, so the updater must rename-and-replace (allowed) and delete the leftover later (not allowed while running), or stop→replace→start; and the daemon must self-detect "on-disk binary ≠ running binary" and exit so the supervisor restarts it.

### Cited Findings

- "Windows does allow renaming while the .exe is open/running … But the .exe is still running, so … remove() 'old' will always fail" — the rename-to-delete pattern: `app.exe` → `app.exe.old`, move new binary into place, clean up on next launch. — [Qt forum](https://forum.qt.io/topic/102668/update-current-application-by-replace-the-exe-and-delete-old-one), [Super User](https://superuser.com/questions/488127/why-can-i-rename-a-running-executable-but-not-delete-it)
- The OS-provided deferral mechanism is the `PendingFileRenameOperations` registry value: "The PendingFileRenameOperations registry value indicates that a user or program has tried to rename a file that is in use. The file names are stored in the value of this entry until the system is restarted and then they are renamed." — [Transoft KB](https://helpdesk.transoftsolutions.com/hc/en-us/articles/4403039103123-Transoft-Software-General-A-Restart-from-a-Previous-Installation-is-Pending)
- Package managers own replacement on their channels: Homebrew installs into a versioned Cellar and relinks (`brew postinstall` exists to replay post-install steps), Scoop uses a `current` junction plus `persist`, and winget re-runs the installer. — [Formula Cookbook](https://docs.brew.sh/Formula-Cookbook), [Persistent data](https://github.com/ScoopInstaller/Scoop/wiki/Persistent-data)
- Re-running an installer on every upgrade is Ollama's documented update path, and it demonstrably overwrites user config. — [Ollama Linux docs](https://docs.ollama.com/linux), [ollama#16191](https://github.com/ollama/ollama/issues/16191)
- Syncthing keeps config separate from the binary (`~/.config/syncthing` by default) so a package upgrade is a no-op for state. — [Syncthing autostart docs](https://docs.syncthing.net/users/autostart.html)

### Inferences

- Concretely on Windows with a registered daemon: the upgrade path is `paper setup --upgrade` → stop the scheduled task/service (`schtasks /end`, `sc stop`) → replace binary → start → then `MoveFileEx(..., MOVEFILE_DELAY_UNTIL_REBOOT)` or leave `paper.exe.old` for next-start cleanup. Renaming a running service image and then stopping it later works, but the ordering that is *testable* and does not depend on rename semantics is stop-first.
- Version-skew guard: every daemon start should compare its own compiled version to the on-disk binary's; if they differ, exit immediately with a "superseded" status so launchd/systemd/Task Scheduler restarts the new one. Without this, a `brew upgrade` leaves the old daemon serving stale behaviour for days.
- A `services` phase must be re-verifiable, not just recorded: on start, if the unit file's `ExecStart` path no longer exists (e.g. npm prefix changed, Cellar version rotated), re-register. This is the "`brew upgrade` moved my binary" bug, which is very common for launchd agents pointing into a versioned Cellar path.
- Model directory must not embed the binary version (Paper already pins model revisions + per-file SHA-256 per `AGENTS.md`), otherwise every release re-downloads 1.7 GB — the single worst possible regression for this change.

### Gaps

- No authoritative source found describing how a *self-updating* Rust CLI (uv, Deno) sequences Windows replacement; the mechanism above is inferred from OS behaviour plus community reports. `self_update` crate documentation was not reviewed.
- Whether Homebrew's `post_install_steps` are re-run on *upgrade* (not just install) is not stated in the text I retrieved; `brew postinstall` being a manual command suggests they may only run on install.

---

## Q6. UX and escape hatches when there is no terminal

### Takeaway

Treat setup as a background service with a status surface, not a chatty installer: one line on stderr only when attached to a TTY, a rotating log file under `PAPER_HOME/logs/`, `paper setup --status`/`paper doctor` for "what is happening", and environment switches that make every phase individually skippable. In a non-TTY, non-interactive context (MCP stdio, CI, winget validator, npm with hidden output) silence is mandatory — anything written to stdout corrupts the MCP stream.

### Cited Findings

- npm hides install-script output by default since npm@7 unless `--foreground-scripts` is passed — so progress must go to a file, not stdout, to be discoverable. — [npm scripts](https://docs.npmjs.com/cli/v10/using-npm/scripts)
- npm best practice is to warn and exit 0 when only optional features fail. — same source
- winget validation runs installers non-elevated and silently, and treats interactivity as a blocking failure — so any prompt or TTY-dependent behaviour is disallowed on that channel. — [ValidationFailureGuide](https://github.com/microsoft/winget-pkgs/blob/master/doc/ValidationFailureGuide.md)
- Precedent for an explicit opt-out flag set at install time: rustup's `--no-modify-path` and `--default-toolchain none`, and later `rustup set auto-install disable` for usage-time suppression. — [rustup book](https://rust-lang.github.io/rustup/installation/other.html), [Rust forum](https://users.rust-lang.org/t/is-it-normal-behavior-that-the-rustup-toolchain-list-command-will-download-and-install-toolchains/130168)
- Precedent for user-visible service status commands: Syncthing documents `systemctl --user status`, `journalctl --user -u`; Tailscale documents `net stop/start Tailscale` and a tray icon after install. — [Syncthing](https://docs.syncthing.net/users/autostart.html), [tailscaled](https://tailscale.com/docs/reference/tailscaled)
- Paper already has the right switch vocabulary in `src/bootstrap.rs`: `PAPER_OFFLINE`, `PAPER_NO_AUTOSTART`, `PAPER_FORCE_BOOTSTRAP`, `PAPER_HOME` (portable ⇒ no services), `CI`, plus `PAPER_NO_RERANKER` / `PAPER_NO_OCR` for per-artefact opt-out.

### Inferences

- Standardise on **one** tri-state rather than N booleans: `PAPER_SETUP=auto|never|always` (default `auto`), with the existing variables retained as aliases. Today the interaction of `PAPER_OFFLINE` + `PAPER_NO_AUTOSTART` + `PAPER_HOME` + `CI` is four independent vetoes that are hard to reason about and harder to test; a single resolved enum (`Offline | Portable | Disabled | Enabled`) is testable, which is what the existing `Switches` struct is groping towards.
- Portable installs (`PAPER_HOME` set) must never register a service or a startup entry — a portable binary that installs a launchd agent is malware-shaped. Current code already does this; keep it and document it as a hard rule.
- Non-TTY rule: write to stdout only if it is a TTY; otherwise append to the log and (optionally) emit a single structured line to stderr. Never write progress to stdout when `serve` is running, because MCP is JSON-RPC on stdout.
- `paper setup --status --json` is worth having from day one: it gives the plugin, the docs, `paper doctor`, and tests a single machine-readable source of truth, and it makes "did the hook run on this channel?" answerable in bug reports.
- Detecting CI should not rely on `CI` alone (many tools set it); also treat `GITHUB_ACTIONS`, `BUILDKITE`, `TF_BUILD`, and "no TTY + no `HOME` writability" as CI-ish. (Inference; `CI` is a de-facto convention, not a standard.)

### Gaps

- No source gathered on whether the MCP registry publishes install hints or install commands (which would be a natural place to put "then run `paper setup`").
- No data on how editors surface MCP server startup failures, which determines whether a "models still downloading" state needs a tool-level message inside the MCP protocol itself.

---

## Q7. Recommended architecture (synthesis)

### Takeaway

Ship **one verb, `paper setup`**, that is (a) idempotent, (b) fast, (c) lock-guarded with kernel locks, (d) resumable via a phase journal, and (e) *scheduling-only*: it registers the user-scope service and starts the daemon; the daemon alone downloads models and builds the index. Every channel that can run a command runs `paper setup --detach`; every channel that cannot falls through to a first-run path that spawns the same thing and returns in milliseconds. The current `bootstrap.rs` becomes a thin trigger plus a non-blocking `try_lock`, and the "first ordinary command blocks" failure disappears because nothing on the foreground path ever performs network I/O or service-manager work.

### Cited Findings

- Phase separation is what the channel policies force: Homebrew's post-install is declarative and sandboxed, winget forbids scripts entirely and validates silently as a standard user. — [Formula Cookbook](https://docs.brew.sh/Formula-Cookbook), [winget-pkgs README](https://github.com/microsoft/winget-pkgs), [ValidationFailureGuide](https://github.com/microsoft/winget-pkgs/blob/master/doc/ValidationFailureGuide.md)
- Apps that need a service on channels that only extract archives must expose their own installer verb — Tailscale's `/install` under Scoop is the precedent. — [Scoop Extras #13460](https://github.com/ScoopInstaller/Extras/issues/13460)
- Detach flags and their preconditions are documented; `CREATE_BREAKAWAY_FROM_JOB` requires the parent's job to permit breakaway. — [Process Creation Flags](https://learn.microsoft.com/en-us/windows/win32/procthread/process-creation-flags)
- `setsid` requires a non-group-leader caller, hence double fork. — [setsid(2)](https://man7.org/linux/man-pages/man2/setsid.2.html)
- Windows allows rename of a running image but not deletion — the basis for rename-then-replace upgrades. — [Super User](https://superuser.com/questions/488127/why-can-i-rename-a-running-executable-but-not-delete-it)
- Install scripts that re-run on upgrade clobber user config unless they are written to be no-ops when state exists. — [ollama#16191](https://github.com/ollama/ollama/issues/16191)
- npm install scripts must warn-and-exit-0 on partial failure — [npm scripts](https://docs.npmjs.com/cli/v10/using-npm/scripts)

### Inferences — the design in detail

**1. Command surface**

```
paper setup [--detach] [--foreground] [--no-service] [--no-models]
            [--integrations=claude,cursor,...] [--status] [--json]
            [--force] [--verbose]
paper setup --post-upgrade        # internal, called by package wrappers
paper daemon-control {install,uninstall,status}
```

Exit codes stay within the project's existing contract (`0` ok, `2` error, `3` model missing); `setup` never returns non-zero merely because provisioning is still running.

**2. Phases and journal** (under `$PAPER_HOME/setup/`)

| Phase | Work | Key / invalidation | Blocking? |
|---|---|---|---|
| `dirs` | create `PAPER_HOME` tree, 0700 | idempotent, always cheap | sync, <5 ms |
| `service` | launchd `LaunchAgent` / `systemd --user` unit / Windows per-user Scheduled Task at logon | re-verify `ExecStart` path exists on every run | sync, ~20–80 ms |
| `start` | detached daemon spawn (double fork / breakaway) | pidfile + liveness probe, else no-op | sync, fire-and-forget |
| `integrations` | `claude mcp add` / editor config writes | keyed by `binary_version` + config schema version; opt-in, never guessed | sync, <50 ms |
| `models` | embedder → reranker → OCR | artefact digest + size, **not** markers alone | **daemon only** |
| `index` | first full index if `chunks` is empty | DB count + roots fingerprint | **daemon only** |

Each record is a small file `setup/<phase>.json` containing `{schema, binary_version, completed_at, artefact_digest?}`. Write temp + rename (Paper already does this for `bootstrap.state`); ignore unknown `schema` (Paper already does this for `version=1`).

**3. Trigger matrix (what each channel does)**

- **curl/PowerShell installer (cargo-dist)**: append (or have the wrapper run) `paper setup --detach --reason=install`, then print a one-line hint. If the generated template cannot be modified, document `curl … | sh && paper setup` and make `paper setup` the first line of the post-install message. *(Open question: whether cargo-dist templates are patchable — see Gaps.)*
- **Homebrew formula (tap)**: `post_install_steps { … }` limited to creating the log dir / touching a marker; **all real work via `caveats`** ("run `paper setup` once, or let the first command do it"). Do not attempt network in post-install.
- **Scoop**: `"post_install": ["& \"$dir\\paper.exe\" setup --detach --no-service"]`; `"persist": ["data"]` — but note Paper's state lives in `PAPER_HOME` (`~/.paper`), outside the Scoop app dir, so `persist` is optional; use it only for `PAPER_HOME`-in-app-dir portables.
- **winget**: `zip`/`portable` manifest, no installer switches, no setup call. First run only.
- **npm**: `postinstall` → spawn detached `setup --detach` with a 10 s watchdog, always `process.exit(0)`.
- **`cargo install`, GitHub Releases, source builds**: first run only.
- **Docker**: bake models into the image or mount `PAPER_HOME` as a volume; entrypoint runs `paper setup --foreground --no-service && paper serve`.
- **Claude plugin / MCP registry**: nothing to do; the plugin's `.mcp.json` is the registration. Ensure `paper serve` is non-blocking and never takes the setup lock.

**4. First-run path (replaces `maybe_autostart`)**

```
if resolved_switches.disable_setup or portable or ci: return
if journal.is_complete() and daemon.is_alive(): return
match SetupLock::try_acquire():          // flock / LockFileEx, non-blocking
    None  => return                      // someone else is on it; serve anyway
    Some(lock) => {
        if tty { eprintln!("paper: provisioned in background; `paper setup --status`") }
        spawn_detached_self("setup --foreground --quiet")  // returns immediately
        // never ensure_service() inline
    }
```

The `serve` path must additionally skip even the spawn when another session already owns the lock, and must never wait on model availability.

**5. Upgrades**

- `setup` compares `journal.binary_version` with `env!("CARGO_PKG_VERSION")`; on mismatch run only `service` (re-verify path) and `integrations` (if schema changed), never `models`.
- The daemon compares running vs on-disk version at start and exits if superseded, letting launchd/`systemd --user`/Task Scheduler restart it.
- Windows replacement: stop task/service → rename old image to `paper.exe.old` → move new image in → start → sweep `.old` next run (or `MOVEFILE_DELAY_UNTIL_REBOOT`).
- Model store keyed by pinned revision + SHA-256 (already the invariant in `AGENTS.md`), so binary upgrades are free.

**6. Concurrency guarantees to test**

- 5 concurrent `paper serve` + 1 `paper setup --foreground` ⇒ exactly one provisioning run, all 5 serve in <100 ms, none blocks on the lock.
- SIGKILL during `models` ⇒ next run resumes/refetches only the incomplete artefact; no `.done` marker is written for it.
- Two `setup` processes started simultaneously ⇒ second returns immediately with `already-running`.
- `PAPER_HOME` on a read-only mount ⇒ `setup` degrades to keyword-only, exit 0, one log line.
- Windows under a restrictive job (`KILL_ON_JOB_CLOSE`) ⇒ breakaway fails gracefully and the scheduled task still exists (verify by logging the fallback).

### Gaps

- The rustdoc/design for `paper setup` above assumes cargo-dist generated installers can be extended; that is unconfirmed and is the blocking prerequisite for the curl channel.
- Whether Apple's `launchd` will repeatedly restart a `LaunchAgent` that exits with status 0 (which the version-skew guard would do) needs empirical verification; it may require `KeepAlive` with `SuccessfulExit: false` tuning.
- No research done on the actual **MCP registry** (`registry.modelcontextprotocol.io`) publication format, so the row "nothing to hook" is inference from the absence of any script concept.

---

## Cross-cutting source-quality notes

- Microsoft, Homebrew, npm, Claude Code and cargo-dist statements above are from primary documentation. The Homebrew Formula Cookbook text describing `post_install_steps` is recent (it describes a migration still in progress) — if the final report is written much later, re-check that page, since `post_install` may be fully removed.
- The Windows rename-while-running fact rests on two community sources (Super User, Qt forum) plus the Transoft KB for `PendingFileRenameOperations`; no Microsoft primary source was fetched. Treat as high-confidence-but-secondary.
- winget's `ValidationFailureGuide.md` is a contributor-facing doc in the manifests repo, not formal policy text; the formal policy lives in the Windows Package Manager policies, which were not fetched.
- Ollama's systemd behaviour is quoted from third-party pages reproducing upstream installer output and from an upstream issue thread; `install.sh` itself was not read.
