# Paperscope Complete Product Launch — Handover

> Updated 2026-11-21 after the fix round. Supersedes the earlier "blocked"
> handover: **the daemon and language workstreams now compile and all tests
> pass.** Remaining work is being executed by three parallel subagents
> (Task 2 daemon CLI integration, Task 4 release scripts, Task 5 website
> completion); their reports land in
> `.superpowers/sdd/2026-11-21-complete-product-launch/task-{2,4,5}-final-report.md`.

## Project Status Summary

**Goal:** Ship paperscope v0.1.0 as a complete product:
1. Auto-indexing daemon (IPC, scheduler, watcher, resource guard) ✅ core done
2. Full language support (ICU segmenter + Lindera, optional features) ✅ done
3. Model distribution (cargo-dist extra-artifacts + first-run downloader) — downloader done, release scripts in progress
4. Website (kintsugi.tools + paperscope page) — design system + 2 pages done, completion in progress
5. Publish to crates.io + document Claude Desktop MCP config — pending release

**Branch:** `main`. Latest pushed: `e5758ae`.

## What's Done (Verified Green)

### Task 1: Daemon Core ✅
- `src/daemon/ipc.rs` — JSON-lines over Unix socket; `Arc<dyn DaemonHandler>`; `IpcClient`; Windows stubs bail.
- `src/daemon/scheduler.rs` — `RootAwareScheduler` (daily 3 AM full + 5 min incremental); async `start`/`stop`; roots mutable at runtime.
- `src/daemon/watcher.rs` — notify with 250 ms debounce, batched `IndexRoots`.
- `src/daemon/resource_guard.rs` — CPU via sysinfo 0.33 (`refresh_cpu_usage`/`cpus()`); battery via Linux `/sys/class/power_supply/BAT*` and macOS `pmset -g batt` (**sysinfo dropped battery support upstream**); metered network = conservative no-op.
- `src/daemon/mod.rs` — dispatch loop handles scheduler/watcher requests; indexing in `spawn_blocking` with a **fresh `Connection` per request** (`Indexer` is not Send/Sync); `GetStatus` counts `DISTINCT path` in `chunks`.
- `daemon` cargo feature is **default**; `paper daemon` runs the daemon.

### Task 3: Language Support ✅
- `icu_segmenter` feature: `WordSegmenter::new_auto()` (needs `compiled_data` + `auto` features).
- `lindera` feature: **lindera 6.2 with `embed-ipadic`** — dictionary embedded in the crate. lindera 0.9 was replaced because its build.rs downloads from Google Drive (broken + violates offline builds). API: `Segmenter::new(Mode::Decompose(Penalty::default()), dict, None)`, cached in a `OnceLock`; tokens via `.surface`.
- Bigram/per-character unit tests are gated `#[cfg(not(feature = "icu_segmenter"))]`; ICU counterparts added. Integration tests in `tests/segmentation.rs` (14 tests, all scripts).

### Task 4: Model Downloader ✅ (release scripts pending)
- `src/cli/model_downloader.rs` — resumable (Range), progress bar, SHA256 verify, atomic `.partial`→final rename, `PAPER_OFFLINE=1`.
- `dist-workspace.toml` — 3 `[[extra-artifacts]]` sections (bge-small-en-v1.5, multilingual-e5-small, bge-reranker-base).
- Tests serialize on an env lock (`PAPER_OFFLINE` raced across parallel test threads); `#![allow(clippy::await_holding_lock)]` is intentional there.
- Pending: `scripts/fetch-models.sh`, `scripts/prepare-release.sh`, release.yml patch (subagent).

### Task 5: Website — design system done, completion pending
- Done: `site/design-tokens.css`, `components.css`, `layout.css`, `index.html`, `paperscope.html`.
- Pending (subagent): `about.html`, `install.sh`, `pick-model.sh`, `.github/workflows/pages.yml`, CNAME/.nojekyll, link check.

### Task 2: Daemon CLI integration — in progress (subagent)
- `paper daemon-control status/pause/resume/index/add-root/remove-root/stop/install/uninstall/start/restart`, systemd user unit + launchd agent, tests for unit/plist rendering.

## Verification (all run, all passing)

```bash
cargo test                                  # full suite, default features
cargo test --features icu_segmenter,lindera # full suite, all language features
cargo clippy --all-targets --features icu_segmenter,lindera -- -D warnings
cargo clippy --no-default-features --all-targets -- -D warnings
cargo fmt --all --check
cargo deny check licenses bans sources      # ok
cargo audit                                 # 2 allow-listed warnings only
cargo publish --dry-run                     # ok
```

Test counts: 4/4 daemon_ipc, 5/5 model_downloader, 14/14 segmentation, 38 lib tests.

## Remaining After Subagents

1. Review the three task reports + code; run the full gate set again; commit.
2. Task 6 integration: `tests/e2e.rs`, `scripts/verify-release.sh`, `docs/CLAUDE_PLUGIN.md` (real channels: crates.io + Homebrew + GitHub Releases + `claude_desktop_config.json`; **no official Claude plugin store exists**), README updates, CHANGELOG.
3. `cargo publish` — needs the owner's `cargo login` (2FA).
4. `git tag v0.1.0 && git push origin main --tags` — first real release-CI run (unsigned artifacts + model extra-artifacts).
5. Push → GitHub Pages deploys `site/` to kintsugi.tools.

## Important Context (do not regress)

- `MAX_STUCK_PARSERS` gates only stalled parser threads; no global live-parser cap.
- `fts_text`/`fts_section` `''` = identity segmentation; never call FTS5 `rebuild`.
- rmcp 3.11: `transport-child-process` feature is broken upstream — do not re-add.
- MSRV 1.94 (candle-core 0.11.1 on x86_64); stable toolchain 1.100 in CI.
- `dist generate --mode=ci` refuses while `allow-dirty=["ci"]`; use `scripts/regenerate-release-ci.sh`, then `scripts/patch-release-ci.sh`.
- Model tests marked `#[ignore]` download 134 MB/487 MB/1.13 GB (network).
- Signing certs unavailable → unsigned, SHA256-checksummed artifacts.
