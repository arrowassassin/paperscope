# Complete Product Launch Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship paperscope as a complete product: auto-indexing daemon with full language support, install.sh with model distribution, redesigned website (kintsugi.tools + paperscope), published to crates.io and Claude plugin store.

**Architecture:** Four independent workstreams executed in parallel by 4 subagents, then integration review:
1. **Daemon Workstream** — Background indexer with scheduler, watcher, resource management, IPC, systemd/launchd installer
2. **Language Workstream** — ICU segmenter + Lindera integration for all major scripts (CJK, Korean, Thai, etc.)
3. **Distribution Workstream** — cargo-dist install.sh with extra-artifacts for models, first-run model downloader with progress/SHA256/resumable/offline
4. **Website Workstream** — New Apple-level design system with CSS variables, kintsugi.tools homepage with product cards, dedicated paperscope landing page

**Tech Stack:** Rust 1.94+, tokio, rusqlite, sqlite-vec, candle, tokenizers, icu_segmenter (feature), lindera (feature), notify, clap, reqwest, cargo-dist, axum (for website if needed), vanilla HTML/CSS/JS for static site

**Spec:** This plan implements the complete product vision discussed in conversation — auto-indexing daemon, full language support, model distribution, website redesign, crates.io + Claude plugin store publish.

## Global Constraints

- **MSRV:** 1.94 (candle-core 0.11.1 requires 1.94 on x86_64 for safe `__cpuid_count`)
- **Licenses:** MIT OR Apache-2.0 only; `cargo deny check licenses bans sources` must pass
- **No telemetry:** `PAPER_OFFLINE=1` must work completely; no network unless opted in
- **Single binary:** `paper` binary installs daemon, CLI, MCP server; no separate daemon binary
- **Permissive deps only:** All new dependencies must be MIT/Apache-2.0/BSD/ISC
- **Model hashes pinned:** Every model file has SHA256 in specs; verify on download
- **Extractive answers only:** No generative inference without explicit `--polish`
- **Exact citations:** Every hit returns `path`, `page`, `line_start`, `line_end`, `anchor`, `section`
- **Dedup + compression:** BLAKE3 content hash dedup; `fts_text`/`fts_section` store only changed text
- **Resource awareness:** Daemon pauses on battery < 20%, CPU > 80%, metered network
- **Binary size:** Default features ~15 MB, `--no-default-features` ~12 MB

## Review Focus

1. **Daemon resource exhaustion** — Daemon must never OOM or spin CPU on large indexes; test with 10k+ documents
2. **Model download corruption** — Resumable downloads must verify SHA256 post-resume; partial files never mistaken for complete
3. **Language segmentation regression** — ICU/Lindera must not break existing CJK bigram fallback; test all supported scripts
4. **Install.sh atomicity** — Failed install must leave system clean; no partial binaries on PATH
5. **MCP index freshness** — `paper serve --mcp` must see newly indexed chunks without restart; test concurrent index + search
6. **Website accessibility** — Dark/light mode, reduced motion, focus visible, semantic HTML, 4.5:1 contrast

---

### Task 1: Daemon Core — IPC Server + Scheduler + Watcher Integration

**Files:**
- Create: `src/daemon/ipc.rs` — Unix socket / named pipe server, request/response protocol
- Create: `src/daemon/scheduler.rs` — Cron-like scheduler (daily full + 5min incremental)
- Create: `src/daemon/watcher.rs` — notify integration with debounce, debounced batch reindex
- Create: `src/daemon/resource_guard.rs` — Battery/CPU/network monitoring, pause/resume logic
- Create: `src/daemon/auto_discover.rs` — Scan Documents/Downloads/Desktop, return default roots
- Create: `src/daemon/mod.rs` — Daemon entry point, ties together IPC + scheduler + watcher + guard
- Modify: `src/cli/mod.rs` — Add `DaemonArgs` subcommand, `run_daemon()` function
- Modify: `src/main.rs` — Add `Daemon` to `Commands` enum
- Test: `tests/daemon_ipc.rs` — IPC protocol, scheduler, watcher, resource guard

**Interfaces:**
- Consumes: `Indexer`, `Connection`, `Workspace` from existing code
- Produces: `DaemonHandle` with `start()`, `stop()`, `status()`; IPC protocol types `DaemonRequest`, `DaemonResponse`

- [ ] **Step 1: Write failing test for IPC protocol**

```rust
#[tokio::test]
async fn ipc_request_index_roots_returns_ok() {
    let (server, client) = setup_ipc().await;
    let resp = client.send(DaemonRequest::IndexRoots { roots: vec![] }).await.unwrap();
    assert!(matches!(resp, DaemonResponse::IndexRoots { stats: _ }));
}
```

Run: `cargo test --test daemon_ipc ipc_request_index_roots_returns_ok -- --nocapture`
Expected: FAIL (module doesn't exist)

- [ ] **Step 2: Implement IPC server in `src/daemon/ipc.rs`**

Signature: `pub async fn run_ipc_server(socket_path: &Path, handler: DaemonHandler) -> Result<()>`
Protocol: JSON lines over Unix socket (Linux/macOS) / named pipe (Windows); each message = `DaemonRequest` or `DaemonResponse`
Requests: `IndexRoots`, `AddRoot`, `RemoveRoot`, `GetStatus`, `Pause`, `Resume`, `Shutdown`
Responses: `IndexRoots { stats: IndexStats }`, `Status { indexed_files, queued, paused, model }`, `Ok`, `Error { msg }`

- [ ] **Step 3: Implement scheduler in `src/daemon/scheduler.rs`**

Signature: `pub struct Scheduler { full_cron: CronExpr, incremental_interval: Duration, sender: mpsc::Sender<DaemonRequest> }`
Methods: `start()`, `stop()`, `trigger_full()`, `trigger_incremental()`
Uses `tokio-cron-scheduler` or custom cron parser; full reindex daily at 3 AM, incremental every 5 min

- [ ] **Step 4: Implement watcher in `src/daemon/watcher.rs`**

Signature: `pub struct Watcher { roots: Vec<PathBuf>, debounce_ms: u64, sender: mpsc::Sender<DaemonRequest> }`
Methods: `start()`, `stop()`, `add_root()`, `remove_root()`
Debounce: collect events for 250ms, batch into single `IndexRoots` request
Uses `notify::RecommendedWatcher`; respects `.gitignore` hidden files

- [ ] **Step 5: Implement resource guard in `src/daemon/resource_guard.rs`**

Signature: `pub struct ResourceGuard { battery_threshold: u8, cpu_threshold: u8, check_interval: Duration }`
Methods: `should_pause() -> bool`, `start_monitoring(sender: mpsc::Sender<PauseReason>)`
Linux: read `/sys/class/power_supply/BAT*/capacity`, `/proc/stat`; macOS: `pmset -g batt`, `sysctl`; Windows: WMI
Pauses indexing when battery < 20% OR CPU > 80% OR metered network detected

- [ ] **Step 6: Implement auto-discovery in `src/daemon/auto_discover.rs`**

Signature: `pub fn discover_default_roots() -> Vec<PathBuf>`
Returns: `~/Documents`, `~/Downloads`, `~/Desktop` if they exist; filters non-directories
Used by daemon on first start if no roots configured

- [ ] **Step 7: Wire daemon in `src/daemon/mod.rs` and CLI**

`pub async fn run_daemon(args: DaemonArgs) -> Result<i32>`
Creates IPC socket at `$PAPER_HOME/daemon.sock` (or platform equivalent)
Spawns scheduler, watcher, resource guard as tokio tasks
Handles shutdown signal (SIGTERM/SIGINT) → graceful stop all tasks

- [ ] **Step 8: Run all daemon tests**

Run: `cargo test --test daemon_ipc`
Expected: All PASS

- [ ] **Step 9: Commit**

```bash
git add src/daemon tests/daemon_ipc.rs src/cli/mod.rs src/main.rs
git commit -m "feat(daemon): IPC server, scheduler, watcher, resource guard, auto-discovery"
```

---

### Task 2: Daemon Integration — MCP Server Connects to Daemon, CLI Controls Daemon

**Files:**
- Modify: `src/mcp/server.rs` — Connect to daemon IPC on startup, forward search/index requests
- Modify: `src/cli/mod.rs` — Add `daemon` subcommands: `install`, `start`, `stop`, `status`, `add-root`, `remove-root`, `schedule`
- Create: `src/daemon/client.rs` — Async IPC client for CLI/MCP to talk to daemon
- Create: `src/daemon/service_installer.rs` — systemd/launchd/Windows Service installer
- Test: `tests/daemon_integration.rs` — Full daemon + CLI + MCP integration

**Interfaces:**
- Consumes: `DaemonClient` from Task 1 IPC types
- Produces: `paper daemon install/start/stop/status` commands; MCP server uses daemon for fresh index

- [ ] **Step 1: Write failing test for daemon client**

```rust
#[tokio::test]
async fn client_can_connect_to_daemon_and_get_status() {
    let _daemon = spawn_test_daemon().await;
    let client = DaemonClient::connect().await.unwrap();
    let status = client.status().await.unwrap();
    assert!(!status.paused);
}
```

Run: `cargo test --test daemon_integration client_can_connect_to_daemon_and_get_status -- --nocapture`
Expected: FAIL

- [ ] **Step 2: Implement daemon client in `src/daemon/client.rs`**

Signature: `pub struct DaemonClient { socket_path: PathBuf }`
Methods: `connect() -> Result<Self>`, `index_roots(roots: &[PathBuf]) -> Result<IndexStats>`, `add_root(path: &Path) -> Result<()>`, `remove_root(path: &Path) -> Result<()>`, `status() -> Result<DaemonStatus>`, `pause() -> Result<()>`, `resume() -> Result<()>`, `shutdown() -> Result<()>`

- [ ] **Step 3: Implement service installer in `src/daemon/service_installer.rs`**

Signature: `pub fn install_service() -> Result<()>`, `pub fn uninstall_service() -> Result<()>`
Linux: writes `/etc/systemd/system/paper-daemon.service` (or user `~/.config/systemd/user/`) with `ExecStart=paper daemon`, `Restart=always`, `RestartSec=5`
macOS: writes `~/Library/LaunchAgents/com.arrowassassin.paper.daemon.plist` with `RunAtLoad`, `KeepAlive`
Windows: creates service via `sc create` or writes NSSM config
All: sets `PAPER_HOME` env, working dir, logging to journal/syslog

- [ ] **Step 4: Add CLI subcommands in `src/cli/mod.rs`**

```rust
#[derive(Subcommand)]
enum DaemonCommand {
    Install, Start, Stop, Restart, Status,
    AddRoot { path: PathBuf },
    RemoveRoot { path: PathBuf },
    Schedule { cron: String },  // e.g., "0 3 * * *"
}
```

Each calls `DaemonClient` or `service_installer`

- [ ] **Step 5: Update MCP server in `src/mcp/server.rs`**

On startup: `DaemonClient::connect().await?` (spawns daemon if not running via `paper daemon start` background)
`search_documents` → forwards to daemon via IPC → returns hits
`answer_documents` → same
`list_sources` → queries daemon's index metadata
Ensure fresh index: daemon handles incremental updates; MCP just queries

- [ ] **Step 6: Run integration tests**

Run: `cargo test --test daemon_integration`
Expected: All PASS

- [ ] **Step 7: Commit**

```bash
git add src/daemon/client.rs src/daemon/service_installer.rs src/mcp/server.rs src/cli/mod.rs tests/daemon_integration.rs
git commit -m "feat(daemon): CLI control, service installer, MCP integration"
```

---

### Task 3: Language Support — ICU Segmenter + Lindera Integration

**Files:**
- Create: `src/core/text/icu_segmenter.rs` — ICU4C bindings for word segmentation (all scripts)
- Create: `src/core/text/lindera_segmenter.rs` — Lindera (pure Rust Japanese) integration
- Modify: `src/core/text.rs` — Unify segmentation: `fts_segment()` dispatches to ICU/Lindera/bigram fallback
- Modify: `Cargo.toml` — Add `icu_segmenter` and `lindera` optional features
- Modify: `src/core/models/specs.rs` — Update language metadata for models
- Test: `tests/segmentation.rs` — All scripts: Chinese, Japanese, Korean, Thai, Lao, Khmer, Myanmar, Latin, Cyrillic, Arabic, Hebrew, Devanagari

**Interfaces:**
- Consumes: `fts_segment()` signature unchanged; `same_tokens()` unchanged
- Produces: `segment(text, lang_hint) -> String` using ICU/Lindera when feature enabled, else bigram fallback

- [ ] **Step 1: Write failing test for ICU Chinese segmentation**

```rust
#[test]
fn icu_segments_chinese_words_not_bigrams() {
    let text = "人工智能改变世界";
    let segmented = fts_segment(text); // with icu_segmenter feature
    // Should contain word boundaries, not just bigrams
    assert!(segmented.contains("人工") || segmented.contains("智能") || segmented.contains("改变") || segmented.contains("世界"));
}
```

Run: `cargo test --features icu_segmenter --test segmentation icu_segments_chinese_words_not_bigrams -- --nocapture`
Expected: FAIL (module doesn't exist)

- [ ] **Step 2: Add ICU feature to Cargo.toml**

```toml
[features]
icu_segmenter = ["dep:icu_segmenter", "dep:icu_provider", "dep:icu_provider_adapters", "dep:icu_normalizer", "dep:icu_properties"]
lindera = ["dep:lindera", "dep:lindera-ipadic"]
```

Dependencies: `icu_segmenter = "1.4"`, `icu_provider = "1.4"`, `icu_provider_adapters = "1.4"`, `icu_normalizer = "1.4"`, `icu_properties = "1.4"` (all Apache-2.0)
`lindera = "0.8"`, `lindera-ipadic = "0.8"` (MIT)

- [ ] **Step 3: Implement ICU segmenter in `src/core/text/icu_segmenter.rs`**

Signature: `pub fn icu_segment(text: &str, locale: &str) -> String`
Uses `icu::segmenter::WordBreakIterator` with locale (e.g., "zh", "ja", "ko", "th", "lo", "km", "my", "ar", "he", "hi")
Returns space-separated tokens for FTS5 indexing
Falls back to bigram if ICU unavailable

- [ ] **Step 4: Implement Lindera segmenter in `src/core/text/lindera_segmenter.rs`**

Signature: `pub fn lindera_segment(text: &str) -> String`
Uses `lindera::tokenizer::Tokenizer` with IPADIC dictionary
Mode: `TokenizeMode::Normal` or `Search` (search better for retrieval)
Returns space-separated tokens

- [ ] **Step 5: Update `fts_segment()` in `src/core/text.rs`**

```rust
pub fn fts_segment(text: &str) -> String {
    #[cfg(feature = "icu_segmenter")]
    if let Ok(lang) = detect_language(text) {
        return icu_segment(text, &lang);
    }
    #[cfg(feature = "lindera")]
    if is_japanese(text) {
        return lindera_segment(text);
    }
    // Fallback: overlapping bigrams + first char
    bigram_segment(text)
}
```

Add `detect_language()` using `whatlang` or simple script detection (Unicode block ranges)

- [ ] **Step 6: Run segmentation tests for all scripts**

Run: `cargo test --features icu_segmenter,lindera --test segmentation`
Expected: All PASS (Chinese words, Japanese words, Korean eojeols, Thai words, Latin unchanged)

- [ ] **Step 7: Commit**

```bash
git add src/core/text/icu_segmenter.rs src/core/text/lindera_segmenter.rs src/core/text.rs Cargo.toml tests/segmentation.rs
git commit -m "feat(lang): ICU segmenter + Lindera for all major scripts"
```

---

### Task 4: Distribution — cargo-dist Install.sh with Model Extra-Artifacts

**Files:**
- Modify: `dist-workspace.toml` — Add `extra-artifacts` for model files, configure shell installer
- Create: `scripts/fetch-models.sh` — CI script to download + verify models for release
- Create: `scripts/prepare-release.sh` — Orchestrates model fetch + cargo-dist build
- Modify: `.github/workflows/release.yml` — Run `scripts/prepare-release.sh` before `dist build`
- Modify: `src/cli/models.rs` — Add `ModelDownloader` with progress, resumable, SHA256, offline support
- Create: `src/cli/model_downloader.rs` — Download logic with `curl --continue-at -`, progress bar, verification
- Test: `tests/model_downloader.rs` — Resumable download, SHA256 verify, offline mode, progress

**Interfaces:**
- Consumes: `ModelSpec` from specs (has SHA256, size, URLs)
- Produces: `paper models install` downloads with progress; install.sh includes model assets on GitHub Releases

- [ ] **Step 1: Write failing test for resumable download**

```rust
#[tokio::test]
async fn resumable_download_recovers_from_interruption() {
    let server = MockServer::start().await;
    let _m = server.mock(|when, then| {
        when.method(GET).path("/model.bin");
        then.status(200).header("Accept-Ranges", "bytes").body(vec![0u8; 10_000_000]);
    });
    let downloader = ModelDownloader::new(server.url("/model.bin"), temp_path(), "sha256...");
    let result = downloader.download_with_progress().await;
    assert!(result.is_ok());
    // Simulate interruption: kill halfway, resume
}
```

Run: `cargo test --test model_downloader resumable_download_recovers_from_interruption -- --nocapture`
Expected: FAIL

- [ ] **Step 2: Add extra-artifacts to `dist-workspace.toml`**

```toml
[[extra-artifacts]]
artifacts = [
    "models/model-small.bin",
    "models/model-medium.bin", 
    "models/model-large.bin"
]
build = ["scripts/fetch-models.sh"]
```

- [ ] **Step 3: Create `scripts/fetch-models.sh`**

Downloads each model from Hugging Face (using pinned revision + SHA256 from specs)
Places in `models/` directory for cargo-dist to pick up
Verifies SHA256 post-download
Exits non-zero on any failure

- [ ] **Step 4: Implement `ModelDownloader` in `src/cli/model_downloader.rs`**

```rust
pub struct ModelDownloader {
    url: String,
    dest: PathBuf,
    expected_sha256: String,
    total_size: u64,
    client: reqwest::Client,
}
impl ModelDownloader {
    pub async fn download_with_progress(&self) -> Result<()> {
        // 1. Check existing .part file for resume
        // 2. HEAD request for total size
        // 3. GET with Range header if resuming
        // 4. Stream to .part with progress bar (indicatif)
        // 5. Verify SHA256
        // 6. Atomic rename .part -> final
    }
}
```

Handles `PAPER_OFFLINE=1` → error if not cached
Uses `indicatif` for progress (already in deps)

- [ ] **Step 5: Update `src/cli/models.rs` install command**

Use `ModelDownloader` for each model file
Shows aggregate progress: "Downloading medium model (487 MB)... ████░░ 62% 2.4 MB/s ETA: 2m"
Verifies each file SHA256
On success: updates installed cache

- [ ] **Step 6: Update release workflow `.github/workflows/release.yml`**

Add step before `dist build`:
```yaml
- name: Fetch and verify models
  run: ./scripts/fetch-models.sh
```

- [ ] **Step 7: Run model downloader tests**

Run: `cargo test --test model_downloader`
Expected: All PASS (resumable, verify, offline error, progress)

- [ ] **Step 8: Test cargo-dist build locally**

Run: `cargo dist build --artifacts=tarball,zip --installers=shell,powershell,homebrew`
Verify: Release artifacts include model files in GitHub Releases

- [ ] **Step 9: Commit**

```bash
git add dist-workspace.toml scripts/fetch-models.sh scripts/prepare-release.sh .github/workflows/release.yml src/cli/model_downloader.rs src/cli/models.rs tests/model_downloader.rs
git commit -m "feat(dist): install.sh with model extra-artifacts, first-run downloader"
```

---

### Task 5: Website Redesign — Design System + kintsugi.tools Homepage + paperscope Page

**Files:**
- Create: `site/design-tokens.css` — CSS custom properties (colors, spacing, type, motion, shadows)
- Create: `site/components.css` — Button, card, code block, nav, footer components using tokens
- Create: `site/layout.css` — Container, grid, section rhythm, hero layout
- Create: `site/index.html` — kintsugi.tools homepage with product cards (kintsugi + paperscope)
- Create: `site/paperscope.html` — Dedicated paperscope landing page
- Create: `site/about.html` — Updated about page with both products
- Create: `site/install.sh` — paperscope install.sh (generated by cargo-dist, copied here)
- Create: `site/pick-model.sh` — paperscope model picker (adapted from kintsugi)
- Delete: `site/index.html` (old inline-style version) — replace entirely
- Test: Manual visual review + Lighthouse CI in workflow

**Interfaces:**
- Consumes: Design research (Apple-level tokens), kintsugi brand (gold), paperscope brand (teal/green)
- Produces: Static site deployed to GitHub Pages at kintsugi.tools

- [ ] **Step 1: Create design tokens in `site/design-tokens.css`**

```css
:root {
  /* Spacing (4px base) */
  --space-1: 4px; --space-2: 8px; --space-3: 12px; --space-4: 16px;
  --space-5: 20px; --space-6: 24px; --space-8: 32px; --space-10: 40px;
  --space-12: 48px; --space-16: 64px; --space-20: 80px; --space-24: 96px;
  /* Radius */
  --radius-sm: 4px; --radius-md: 6px; --radius-lg: 8px; --radius-xl: 12px; --radius-2xl: 16px; --radius-full: 9999px;
  /* Typography */
  --font-sans: 'Inter Variable', system-ui, sans-serif;
  --font-mono: 'Berkeley Mono', ui-monospace, monospace;
  --font-display: 'Inter Variable', system-ui, sans-serif;
  --tracking-display: -0.022em; --tracking-h1: -0.015em; --tracking-h2: -0.009em; --tracking-tag: 0.1em;
  /* Motion */
  --duration-instant: 100ms; --duration-fast: 150ms; --duration-standard: 200ms; --duration-moderate: 300ms; --duration-slow: 450ms;
  --ease-standard: cubic-bezier(0.33, 1, 0.68, 1);
  /* Shadows (light default) */
  --shadow-ring: 0 0 0 1px rgba(0,0,0,0.08);
  --shadow-card: var(--shadow-ring), 0 2px 2px rgba(0,0,0,0.04), 0 8px 8px -8px rgba(0,0,0,0.04);
  --shadow-elevated: rgba(50,50,93,0.25) 0 30px 45px -30px, rgba(0,0,0,0.1) 0 18px 36px -18px;
  /* Z-index */
  --z-dropdown: 100; --z-sticky: 200; --z-modal: 300; --z-toast: 400; --z-tooltip: 500;
}

/* kintsugi brand (dark-native) */
[data-brand="kintsugi"] {
  --color-bg: #0b0e16; --color-bg-panel: #121a2b; --color-bg-surface: #0e1626;
  --color-text: #e8ecf5; --color-text-secondary: #8b95ad; --color-text-muted: #6a7488;
  --color-border: #283557; --color-border-subtle: rgba(40,53,87,.5);
  --color-accent: #D4AF37; --color-accent-hover: #EBC65A; --color-accent-line: rgba(212,175,55,.35);
}

/* paperscope brand (teal/green, light-native) */
[data-brand="paperscope"] {
  --color-bg: #f3efe3; --color-bg-panel: #fbf8ef; --color-bg-surface: #f6f1e4;
  --color-text: #1b2230; --color-text-secondary: #6a7488; --color-text-muted: #8b95ad;
  --color-border: #d8cfb6; --color-border-subtle: rgba(201,192,166,.55);
  --color-accent: #1a8a6a; --color-accent-hover: #16a37a; --color-accent-line: rgba(26,138,106,.35);
}

/* Dark mode for paperscope */
@media (prefers-color-scheme: dark) {
  [data-brand="paperscope"] {
    --color-bg: #08090a; --color-bg-panel: #0f1011; --color-bg-surface: #191a1b;
    --color-text: #f7f8f8; --color-text-secondary: #d0d6e0; --color-text-muted: #8a8f98;
    --color-border: rgba(255,255,255,0.08); --color-border-subtle: rgba(255,255,255,0.05);
    --color-accent: #2dd4a3; --color-accent-hover: #5eead4; --color-accent-line: rgba(45,212,163,.35);
  }
}
```

- [ ] **Step 2: Create components in `site/components.css`**

Buttons (primary, ghost, secondary), cards (surface, border, radius, shadow, hover), code blocks (mono, line-height 1.6, scrollable), nav (sticky, backdrop-blur, mobile drawer), footer (dark on light, grid)

- [ ] **Step 3: Create layout in `site/layout.css`**

Container max-width 1200px, section vertical rhythm 64-120px, hero centered, 2-3 column feature grids, eyebrow→H2→body→CTA section anatomy

- [ ] **Step 4: Build `site/index.html` — kintsugi homepage**

Hero: "Kintsugi — Safety layer for AI coding agents" + gold accent + animated terminal demo
Product cards (2-column): Kintsugi card + Paperscope card → link to dedicated pages
Features: Intercepts dangerous commands, deterministic rules, undo with snapshots, tamper-evident log, local-first
Footer: Product grid (2 cols), company links, GitHub

- [ ] **Step 5: Build `site/paperscope.html` — paperscope landing page**

Hero: "Paper — Local-first document search with exact citations" + teal accent + live search demo
Features: FTS5 + sqlite-vec, extractive answers, MCP server, formats table, multilingual
Install section: `cargo install paperscope` + `paper models install bge-small-en-v1.5`
Footer: Links to GitHub, docs, crates.io

- [ ] **Step 6: Update `site/about.html`**

"Arrowassassin Tools" — both products with status badges, links to each

- [ ] **Step 7: Copy cargo-dist generated install.sh + pick-model.sh to site/**

After release build, copy `target/dist/paperscope-installer.sh` → `site/install.sh`
Adapt kintsugi's `pick-model.sh` for paperscope models

- [ ] **Step 8: Add GitHub Pages deploy workflow**

`.github/workflows/pages.yml` — builds site, deploys to `gh-pages` branch

- [ ] **Step 9: Visual review + Lighthouse**

Manual: open `site/index.html`, `site/paperscope.html` in browser
Check: dark/light toggle, reduced motion, focus visible, mobile responsive
Lighthouse: Performance > 90, Accessibility > 95, Best Practices > 90, SEO > 90

- [ ] **Step 10: Commit**

```bash
git add site/design-tokens.css site/components.css site/layout.css site/index.html site/paperscope.html site/about.html site/install.sh site/pick-model.sh .github/workflows/pages.yml
git commit -m "feat(site): Apple-level redesign, kintsugi + paperscope product pages, design system"
```

---

### Task 6: Integration — End-to-End Testing, crates.io Publish, Claude Plugin Store

**Files:**
- Modify: `README.md` — Update with daemon usage, install.sh, model auto-install
- Modify: `docs/implementation-status.md` — Mark all gaps as implemented
- Create: `docs/CLAUDE_PLUGIN.md` — Submission guide for Claude plugin store
- Test: `tests/e2e.rs` — Full flow: install.sh → daemon install → index → search → MCP
- Script: `scripts/verify-release.sh` — Runs all gates (fmt, clippy, test, deny, audit, publish --dry-run)

**Interfaces:**
- Consumes: All previous tasks
- Produces: Published crate, live website, Claude plugin submission

- [ ] **Step 1: Write E2E test in `tests/e2e.rs`**

```rust
#[tokio::test]
async fn full_install_daemon_index_search_flow() {
    // 1. Run install.sh (or cargo install) in temp dir
    // 2. paper daemon install && paper daemon start
    // 3. paper index --root ~/test-docs
    // 4. paper search "query" → hits with citations
    // 5. paper serve --mcp --root ~/test-docs → MCP tools work
    // 6. paper daemon stop && paper daemon uninstall
}
```

- [ ] **Step 2: Run all gates in `scripts/verify-release.sh`**

```bash
#!/bin/bash
set -euo pipefail
cargo +1.100.0 fmt --all --check
cargo +1.100.0 clippy --all-targets --all-features -- -D warnings
cargo +1.100.0 clippy --no-default-features --all-targets -- -D warnings
cargo +1.94.0 test
cargo +1.100.0 test
cargo deny check licenses bans sources
cargo audit
cargo +1.100.0 publish --dry-run
actionlint .github/workflows/ci.yml .github/workflows/release.yml .github/workflows/pages.yml
```

Run: `bash scripts/verify-release.sh`
Expected: All PASS

- [ ] **Step 3: Publish to crates.io**

Run: `cargo login` (interactive, 2FA) → `cargo publish`
Verify: `cargo search paperscope` shows v0.1.0

- [ ] **Step 4: Trigger release workflow**

Push tag: `git tag v0.1.0 && git push origin v0.1.0`
GitHub Actions: `release.yml` builds all targets, uploads artifacts, generates installers
Verify: GitHub Release has tarballs, zip, shell installer, Homebrew formula, SHA256SUMS

- [ ] **Step 5: Deploy website**

Push to main → `pages.yml` deploys to GitHub Pages
Verify: https://kintsugi.tools shows new design, https://kintsugi.tools/paperscope.html works

- [ ] **Step 6: Submit to Claude Plugin Store**

Follow `docs/CLAUDE_PLUGIN.md`:
- MCP server name: `paper`
- Description: "Local document search with exact citations"
- Install command: `cargo install paperscope` or `curl -fsSL https://kintsugi.tools/install.sh | sh`
- Config example for `claude_desktop_config.json`
- Test with Claude Desktop

- [ ] **Step 7: Update documentation**

`README.md` — add daemon, install.sh, auto-model sections
`docs/implementation-status.md` — all gaps ✅ implemented
`CHANGELOG.md` — v0.1.0 release notes

- [ ] **Step 8: Commit final release**

```bash
git add README.md docs/implementation-status.md CHANGELOG.md docs/CLAUDE_PLUGIN.md scripts/verify-release.sh tests/e2e.rs
git commit -m "release: v0.1.0 — complete product launch"
git tag v0.1.0
git push origin main --tags
```

---

## Execution Order

**Phase 1 (Parallel — 4 subagents):**
- Subagent 1: Task 1 (Daemon Core)
- Subagent 2: Task 3 (Language Support)
- Subagent 3: Task 4 (Distribution)
- Subagent 3: Task 5 (Website Redesign)

**Phase 2 (Sequential — after Phase 1 complete):**
- Task 2 (Daemon Integration) — depends on Task 1
- Task 6 (Integration) — depends on all

**Note:** Task 2 must wait for Task 1 IPC types. Tasks 3, 4, 5 are fully independent.

## Rulings Log

- Ruling: Single binary `paper` with `daemon` subcommand (not separate `paper-daemon` binary) — keeps distribution simple, matches user request
- Ruling: ICU + Lindera as optional features (not default) — keeps binary size down for users who don't need full language support; default stays bigram
- Ruling: Models as extra-artifacts on GitHub Releases (not bundled in tarball) — avoids bloating every platform download
- Ruling: Design system from scratch (not refactor kintsugi inline styles) — inline styles unmaintainable; new token system is cleaner
- Ruling: kintsugi.tools homepage shows both products as cards (PlanetScale pattern) — scales to 3-4 tools, clear product identity