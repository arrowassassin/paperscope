# Paperscope Complete Product Launch — Handover for New Codex CLI Session

## Project Status Summary

**Goal:** Ship paperscope v0.1.0 as a complete product with:
1. Auto-indexing daemon (background service with scheduler, watcher, resource guard)
2. Full language support (ICU segmenter + Lindera for all major scripts)
3. Model distribution via install.sh (cargo-dist extra-artifacts + first-run downloader)
4. Redesigned website (kintsugi.tools + paperscope landing page, Apple-level design)
5. Published to crates.io + documented for Claude Desktop MCP config

**Current State:** 4 subagents dispatched, made significant progress but hit blockers. **No work committed yet** — all changes are uncommitted.

---

## What's Done (Working)

### Task 3: Language Support (ICU + Lindera) — **Core implementation complete, blocked on ICU4X 1.5 API**
- ✅ `Cargo.toml` — Added `icu_segmenter` and `lindera` features with all deps (Apache-2.0/MIT)
- ✅ `src/core/text/icu_segmenter.rs` — Module structure with feature gate
- ✅ `src/core/text/lindera_segmenter.rs` — Module structure with feature gate
- ✅ `src/core/text.rs` — Updated `fts_segment()` dispatch + `detect_language()`
- ✅ `tests/segmentation.rs` — Comprehensive tests for all scripts
- ❌ **Blocker:** ICU4X 1.5 API changed — `WordBreakIterator` internal, need `WordSegmenter` with `compiled_data` + `auto` features. Need to fix API usage.

### Task 4: Distribution (install.sh + Model Downloader) — **Core Rust complete, CI/scripts remaining**
- ✅ `src/cli/model_downloader.rs` — Full implementation: resumable (Range headers), progress bar (indicatif), SHA256 verify, atomic rename, PAPER_OFFLINE=1 support
- ✅ `src/cli/models.rs` — Updated `ModelsCommand::Install` to use ModelDownloader with aggregate progress
- ✅ `dist-workspace.toml` — 3 extra-artifacts sections for all models (bge-small-en-v1.5, multilingual-e5-small, bge-reranker-base)
- ✅ `tests/model_downloader.rs` — **All 5 tests PASSING**: resumable, SHA256 verify, mismatch, offline, partial resume
- ❌ Remaining: `scripts/fetch-models.sh`, `scripts/prepare-release.sh`, `.github/workflows/release.yml` patch

### Task 1: Daemon Core (IPC + Scheduler + Watcher) — **All modules created, blocked on thread-safety**
- ✅ `src/daemon/ipc.rs` — JSON lines protocol, Unix socket / named pipe, request/response types
- ✅ `src/daemon/scheduler.rs` — Cron scheduler (daily 3 AM + 5min incremental)
- ✅ `src/daemon/watcher.rs` — notify crate with 250ms debounce
- ✅ `src/daemon/resource_guard.rs` — Battery/CPU/network monitoring
- ✅ `src/daemon/auto_discover.rs` — Documents/Downloads/Desktop discovery
- ✅ `src/daemon/mod.rs` — Daemon entry point tying all together
- ✅ `tests/daemon_ipc.rs` — Integration tests
- ❌ **Blockers:**
  - `Indexer` not Send/Sync (rusqlite RefCell) — need separate connections per task or Arc<Mutex>
  - Sysinfo 0.33 API mismatch
  - Scheduler Clone issue (JoinHandle not Clone)
  - IPC handler lifetime (need Arc<dyn DaemonHandler>)
  - Notify unwatch API

### Task 5: Website Redesign — **Design system + kintsugi homepage done, paperscope page missing**
- ✅ `site/design-tokens.css` — Complete token system (both brands, dark/light, motion, shadows)
- ✅ `site/components.css` — Buttons, cards, code blocks, nav, footer
- ✅ `site/layout.css` — Container, grid, section rhythm, hero
- ✅ `site/index.html` — kintsugi homepage with product cards (kintsugi + paperscope)
- ❌ Missing: `site/paperscope.html`, `site/about.html`, `site/pick-model.sh`, `site/install.sh`, `.github/workflows/pages.yml`

---

## Files Changed (Uncommitted)

```
Modified:
  Cargo.toml                    # Added icu/lindera features, daemon deps
  dist-workspace.toml           # Extra-artifacts for 3 models
  src/cli/mod.rs                # Export DaemonArgs
  src/cli/models.rs             # ModelDownloader integration
  src/core/index/indexer.rs     # Minor changes
  src/core/text.rs              # ICU/Lindera dispatch
  src/lib.rs                    # pub mod daemon
  src/main.rs                   # Daemon command variant
  Cargo.lock                    # Updated

New Files:
  src/daemon/                   # ipc.rs, scheduler.rs, watcher.rs, resource_guard.rs, auto_discover.rs, mod.rs
  src/core/text/                # icu_segmenter.rs, lindera_segmenter.rs
  src/cli/model_downloader.rs
  tests/daemon_ipc.rs
  tests/model_downloader.rs     # 5 tests PASSING
  tests/segmentation.rs
  site/                         # design-tokens.css, components.css, layout.css, index.html
  docs/superpowers/plans/2026-11-21-complete-product-launch.md
  research_notes/               # Modern landing page + rust install research
```

---

## Key Blockers to Fix

### 1. Daemon Thread-Safety (Task 1)
**Problem:** `Indexer` holds `rusqlite::Connection` which contains `RefCell` → not `Send`/`Sync`.
**Fix Options:**
- Option A: Create separate `Connection` per daemon task (recommended — matches existing pattern in `Indexer::index_roots` which opens fresh connections)
- Option B: Wrap `Indexer` in `Arc<Mutex<>>` (simpler but less concurrent)
- **Recommended:** In `DaemonHandlerImpl`, don't store `Indexer` — create fresh `Indexer` per request using `Workspace::open_index()` like CLI does.

### 2. ICU4X 1.5 API (Task 3)
**Problem:** `WordBreakIterator` is private; must use `WordSegmenter` with compiled data.
**Fix:**
```toml
# In Cargo.toml icu_segmenter features:
icu_segmenter = { version = "1.5", features = ["compiled_data", "auto"] }
```
```rust
// In icu_segmenter.rs
use icu::segmenter::WordSegmenter;
let segmenter = WordSegmenter::new_auto();
for word in segmenter.segment_str(text) { ... }
```

### 3. Sysinfo 0.33 API (Task 1)
**Problem:** Old trait-based API (`SystemExt`, `BatteryExt`) replaced with direct methods.
**Fix:** Check actual API — e.g., `system.batteries()`, `system.cpus()`, `system.networks()`.

### 4. Scheduler Clone (Task 1)
**Problem:** `RootAwareScheduler` derives `Clone` but contains `JoinHandle`.
**Fix:** Remove `Clone` derive, use `Arc<Mutex<Option<JoinHandle>>>` for task handle.

---

## Next Steps for New Session

### Priority 1: Fix Blockers & Get Tests Green
```bash
# 1. Fix ICU segmenter API
cargo check --features icu_segmenter  # iterate until clean

# 2. Fix daemon thread-safety
# Edit src/daemon/mod.rs DaemonHandlerImpl to not hold Indexer

# 3. Fix sysinfo API in resource_guard.rs

# 4. Run all tests
cargo test --features icu_segmenter,lindera
cargo test --test model_downloader
cargo test --test daemon_ipc
```

### Priority 2: Complete Remaining Deliverables
- **Task 4:** Create `scripts/fetch-models.sh`, `scripts/prepare-release.sh`, patch release workflow
- **Task 5:** Build `site/paperscope.html`, `site/about.html`, `site/pick-model.sh`, GitHub Pages workflow
- **Task 2:** Daemon integration (MCP + CLI control) — depends on Task 1 fixes

### Priority 3: Release Gates
```bash
./scripts/verify-release.sh  # Create this: fmt, clippy, test (1.94+1.100), deny, audit, publish --dry-run, actionlint
cargo publish  # Requires cargo login (2FA)
git tag v0.1.0 && git push origin main --tags
```

### Priority 4: Website Deploy
- Push to main → GitHub Pages deploys to kintsugi.tools
- Verify: https://kintsugi.tools (new design), https://kintsugi.tools/paperscope.html

---

## Research & Context (Already Done)

- **Modern landing page design:** `research_notes/modern_landing_page_design/apple_level_design.md` (900+ lines)
- **Multi-product site patterns:** `research_notes/modern_landing_page_design/multi_product_site.md`
- **Rust install with models:** `research_notes/rust install with models/rust_install_with_models.md`

---

## Commands Reference

```bash
# Test specific features
cargo test --features icu_segmenter --test segmentation
cargo test --features icu_segmenter,lindera --test segmentation
cargo test --test model_downloader
cargo test --test daemon_ipc

# Check all targets
cargo +1.100.0 fmt --all --check
cargo +1.100.0 clippy --all-targets --all-features -- -D warnings
cargo +1.100.0 clippy --no-default-features --all-targets -- -D warnings
cargo +1.94.0 test
cargo +1.100.0 test

# Deny & audit
cargo deny check licenses bans sources
cargo audit

# cargo-dist build
cargo dist build --artifacts=tarball,zip --installers=shell,powershell,homebrew

# Release dry-run
cargo +1.100.0 publish --dry-run

# Actionlint
actionlint .github/workflows/ci.yml .github/workflows/release.yml .github/workflows/pages.yml
```

---

## Architecture Notes for Implementer

### Daemon IPC Protocol
```json
// Request
{"type":"IndexRoots","roots":["/path/to/docs"]}
{"type":"GetStatus"}

// Response
{"type":"IndexRoots","stats":{"scanned":10,"indexed":5,"chunks":200}}
{"type":"Status","indexed_files":10,"queued":0,"paused":false,"model":"bge-small-en-v1.5"}
```

### Model File Structure (from specs.rs)
```rust
// bge-small-en-v1.5: 3 files, 134 MB total
ModelFile { name: "model.safetensors", sha256: "3c9f31...", size: 133_466_304 }
ModelFile { name: "config.json", sha256: "094f8e...", size: 743 }
ModelFile { name: "tokenizer.json", sha256: "d241a6...", size: 711_396 }

// multilingual-e5-small: 3 files, 487 MB
// bge-reranker-base: 3 files, 1.13 GB
```

### Website Brand Tokens
- **kintsugi:** dark-native, gold `#D4AF37`, IBM Plex Sans/Mono
- **paperscope:** light-native, teal `#1a8a6a`, Inter Variable/Berkeley Mono

---

## Handover Checklist for New Session

- [ ] Read this HANDOVER.md completely
- [ ] Read the implementation plan: `docs/superpowers/plans/2026-11-21-complete-product-launch.md`
- [ ] Fix ICU4X API in `src/core/text/icu_segmenter.rs`
- [ ] Fix daemon thread-safety in `src/daemon/mod.rs`
- [ ] Fix sysinfo API in `src/daemon/resource_guard.rs`
- [ ] Run all tests green
- [ ] Complete Task 4 scripts (fetch-models.sh, prepare-release.sh, release.yml)
- [ ] Complete Task 5 website pages (paperscope.html, about.html, pick-model.sh, pages.yml)
- [ ] Complete Task 2 daemon integration (MCP + CLI)
- [ ] Run all release gates
- [ ] Publish to crates.io
- [ ] Deploy website
- [ ] Document Claude Desktop MCP config in README

---

**Good luck!** The hard research and scaffolding is done. The remaining work is fixing API mismatches and wiring up the remaining pieces.