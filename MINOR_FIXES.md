# Minor fixes — paperscope `0.2.2`

Fixes for the Minor code-review issues (M1, M2, M4, M14–M18, M20, M22).
The 5 Critical issues were already fixed; the Important issues are being handled by another subagent.

## What was fixed

### M1 — Magic numbers scattered (multiple files)
Pulled inline literals into named constants with doc comments.

- `src/core/search/tier1.rs` — replaced `limit * 64` and `4096` with
  `TIER1_FILTERED_FETCH_FACTOR: usize = 64` and
  `TIER1_FILTERED_FETCH_MIN: usize = 4096` (both documented), used in the
  widened KNN fetch in `tier1_search_filtered`.
- `src/core/extract/ooxml.rs` — replaced the inline `350` in
  `merge_paragraphs(paragraphs, 350)` with `DOCX_MERGE_MAX_WORDS: usize = 350`
  (documented).

Other self-evident literals (e.g. the `240` snippet length, `max_pages: 1`,
`max_entries: 1`) were intentionally left alone — they were not flagged as
magic numbers by the review, and converting them adds noise without a behavior
change.

### M2 — Redundant `ModelManager` constructions (`src/cli/mod.rs`)
`Workspace` now holds a single `model_manager: ModelManager`, constructed once
in `Workspace::new()`. `embedder()`, `pipeline()`,
`embedding_model_installed()`, and `reranker_installed()` all borrow
`self.model_manager` instead of each calling `ModelManager::new()`.

### M4 — `Config::load` silently falls back on corrupt config (`src/core/config.rs`)
`load_from` now catches a TOML parse error on an existing config file, logs a
`tracing::warn!`, and falls back to `Self::default()`. Previously a corrupt
file either propagated an error or was silently swallowed by `unwrap_or_default`
at the call site with no visible warning.

### M14 — Model install lacks concurrency guard (`src/cli/models.rs`)
Added a per-model `std::sync::Mutex` lock (`MODEL_INSTALL_LOCKS` backed by a
`OnceLock<HashMap<...>>` + `model_install_lock()` helper). `ModelsCommand::Install`
and `ModelsCommand::Remove` now acquire the per-model lock, serializing
concurrent installs/removes of the same model so they cannot race on the shared
model directory / `.partial` files. Different models still proceed in parallel.

### M15 — Daemon `GetStatus` opens two connections (`src/daemon/mod.rs`)
`GetStatus` now opens one `Connection` and reuses it for both queries.
`count_indexed_files` and `stored_embedding_model` now take `&Connection`
instead of each calling `Connection::open`.

### M16 — OCR engine detection doesn't verify binary (`src/core/extract/ocr.rs`)
`detect_ocr_engine()` now verifies `which::which("tesseract")` succeeds before
advertising any engine. All platform `recognize()` implementations (macOS
Vision, Windows OCR, Linux tesseract) currently fall back to the `tesseract`
binary, so advertising an engine when it's missing leads to a failed `recognize`.
Returns `None` when `tesseract` is unavailable.

### M17 — `config_paths` test hardcodes `/tmp/docs` (`tests/config_paths.rs`)
`config_round_trips_through_toml` now uses `dir.path().join("docs")` (a path
under the `tempfile::tempdir()` root) instead of the hardcoded `/tmp/docs`, and
asserts against that temp path.

### M18 — `eval.rs` `build_golden_index` leaves temp files (`src/core/eval.rs`)
Replaced `env::temp_dir().join(format!("paper_eval_{}", pid))` with
`tempfile::TempDir::new()`, which is dropped at the end of `build_golden_index`
to clean up the index and document files. Removed the now-unused `use std::env;`.

### M20 — Scheduler `try_send` silently drops (`src/daemon/scheduler.rs`)
`trigger_full()` and `trigger_incremental()` now log a `tracing::warn!` when
`try_send` fails (channel full) instead of silently dropping the scheduled
index.

### M22 — `lib.rs` re-exports eval types publicly (`src/lib.rs`)
Removed the `pub use core::eval::{...}` re-export block so `EvalError`,
`EvalReport`, etc. no longer leak from the `paper` crate root. They remain
reachable via `paper::core::eval::`, which is what `benches/eval.rs` uses.
(No `eval` feature was added because `Cargo.toml` is off-limits and there is
no existing `eval` feature to gate behind.)

## Pre-existing clippy issue cleared (required for the deliverable)
`src/daemon/mod.rs`: removed the now-dead `request_tx` field from
`DaemonHandlerImpl` (the Important-fix work changed `Shutdown` to use
`shutdown_notify`, leaving the field written-but-never-read) and its
initialization. This was not one of the listed Minor issues, but it blocked
`cargo clippy --all-targets -- -D warnings`, which the deliverable requires to
be warning-free.

## Files modified
- `src/core/search/tier1.rs` (M1)
- `src/core/extract/ooxml.rs` (M1)
- `src/cli/mod.rs` (M2)
- `src/core/config.rs` (M4)
- `src/cli/models.rs` (M14)
- `src/daemon/mod.rs` (M15, plus pre-existing dead field removal)
- `src/core/extract/ocr.rs` (M16)
- `tests/config_paths.rs` (M17)
- `src/core/eval.rs` (M18)
- `src/daemon/scheduler.rs` (M20)
- `src/lib.rs` (M22)

## What was NOT fixed (with reason)
- Other inline literals across the codebase — not flagged as magic numbers by
  the review; converting them is out of scope and risks noise.
- `Cargo.toml` / `Cargo.lock` — explicitly off-limits (no dependency or feature
  changes; M16 uses the existing `which` dep behind `ocr`, M18 uses the existing
  `tempfile` dep).
- `site/` and `scripts/` — explicitly off-limits.

## Verification
- `cargo check --features icu_segmenter,lindera` — passes
- `cargo test --features icu_segmenter,lindera` — all suites pass, 0 failures
- `cargo clippy --all-targets --features icu_segmenter,lindera -- -D warnings` — no warnings
