# Important Issues Fixed (I1-I8, II, IJ, IK, IL, IM)

## Summary

All 10 issues with specific fix instructions were fixed. Issues I3, I4, I5 (within the I1-I8 range) and any Minor issues were not fixed because no fix instructions or file locations were provided.

---

## I1 — OCR temp-file race (symlink attack)

**File:** `src/core/extract/ocr.rs`, `Cargo.toml`

**Fix:** Replaced predictable temp-file names (`paper_ocr_{pid}.png/.txt`) with `tempfile::Builder` using random suffixes via `tempfile_in(std::env::temp_dir())`. The image is written to a `NamedTempFile`, the real path is passed to tesseract, and the `.txt` output path is derived from the same random stem and cleaned up after reading.

**Cargo.toml:** Added `tempfile = "3.8"` to `[dependencies]` (it was previously only a dev-dependency).

**Verification:** `cargo check` and `cargo test` pass.

---

## I2 — Stuck-parser thread leak

**File:** `src/core/extract/mod.rs`

**Fix:** Added a background reaper thread (lazily started via `OnceLock`) that wakes every 5 seconds and joins any finished stuck parser handles, reclaiming their thread resources instead of leaking them until the next extraction attempt. `reap_stuck_parsers()` is also called at the start of each extraction attempt. The `MAX_STUCK_PARSERS = 4` cap is retained. Per-file timeout errors are still returned (the whole indexer is not blocked).

**Verification:** `cargo check` and `cargo test` pass.

---

## I6 — Unbounded spreadsheet row iteration

**File:** `src/core/extract/sheet.rs`

**Fix:** Added a row counter (`rows_processed`) inside `Sheet::extract` capped at `capabilities.max_entries` (1,000,000). When the cap is reached, a warning is pushed and iteration stops (`break`), preventing unbounded row iteration on hostile workbooks.

**Verification:** `cargo check` and `cargo test` pass.

---

## I7 — `chunk_text` line-number underflow

**File:** `src/core/index/indexer.rs:439`

**Fix:** Changed `start_line = line_no - kept.len()` to `start_line = line_no.saturating_sub(kept.len())` to prevent `usize` underflow when `kept.len() > line_no`.

**Verification:** `cargo check` and `cargo test` pass.

---

## I8 — `prune_missing` TOCTOU

**File:** `src/core/index/indexer.rs:315-355`

**Fix:** Moved the existence check inside the deletion transaction. Each path is re-verified with `!Path::new(path).exists()` immediately before its `DELETE` statements execute, so a file recreated between the initial filter and the delete is not pruned. The removal count now reflects only actually-deleted rows.

**Verification:** `cargo check` and `cargo test` pass.

---

## II — LIKE wildcard injection

**File:** `src/core/search/filters.rs`

**Fix:** Added `ESCAPE '\\'` to both LIKE clauses (extension filter and `path_contains`) and an `escape_like()` helper that escapes `%`, `_`, and `\` in user-supplied values so they are matched literally rather than interpreted as SQL LIKE patterns.

**Verification:** `cargo check` and `cargo test` pass.

---

## IJ — aggregate.rs SQL (wrong column, case-sensitive)

**File:** `src/cli/aggregate.rs`

**Fix:** Changed the extension filter from `path LIKE ?` to `lower(source_path) LIKE lower(?) ESCAPE '\\'`, applied to the correct column (`source_path`), case-insensitive via `lower()`, with LIKE wildcards escaped via a new `escape_like()` helper. Removed the unused `_placeholders` variable.

**Verification:** `cargo check` and `cargo test` pass.

---

## IK — daemon Shutdown self-send

**File:** `src/daemon/mod.rs`

**Fix:** The `Shutdown` handler no longer re-enqueues another `Shutdown` on the same channel. Instead it sets a `shutdown: RwLock<bool>` flag in `DaemonState` and notifies a `tokio::sync::Notify`. The dispatch loop uses `tokio::select!` to wake on either a channel message or the notification, then breaks when the flag is set. This avoids the self-send race where the handler's `Ok` was sent before the daemon actually stopped.

**Verification:** `cargo check` and `cargo test` pass (all 7 daemon_control tests pass, including `stop_command_shuts_down_daemon_process`).

---

## IL — Watcher debounce loses events

**File:** `src/daemon/watcher.rs:157`

**Fix:** `event_tx.try_send()` failures (channel full) are now logged with `tracing::warn!` including the count of dropped events, instead of silently dropping them. The channel capacity remains 100; `try_send` is still used because `handle_event` runs in the synchronous notify callback thread.

**Verification:** `cargo check` and `cargo test` pass.

---

## IM — resource_guard battery multi-battery

**File:** `src/daemon/resource_guard.rs`

**Fix:** `battery_status()` was replaced with `battery_statuses()` returning `Option<Vec<BatteryStatus>>`. On Linux, all `BAT*` entries are collected (not just the first). A new `any_battery_low()` helper returns true if *any* battery is below threshold and discharging. Both callers (`evaluate()` and `start_monitoring()`) now check all batteries, so multi-battery laptops correctly pause when any battery is low and discharging.

**Verification:** `cargo check` and `cargo test` pass.

---

## Additional fix (pre-existing compilation error)

**File:** `src/cli/models.rs:23`

**Fix:** Changed `let map` to `let mut map` so `HashMap::entry()` can be called mutably. This was a pre-existing compilation error blocking `cargo check`, unrelated to the Important issues but required for the build to succeed.

---

## Files Modified

| File | Issues |
|------|--------|
| `Cargo.toml` | I1 (added `tempfile` dependency) |
| `src/core/extract/ocr.rs` | I1 |
| `src/core/extract/mod.rs` | I2 |
| `src/core/extract/sheet.rs` | I6 |
| `src/core/index/indexer.rs` | I7, I8 |
| `src/core/search/filters.rs` | II |
| `src/cli/aggregate.rs` | IJ |
| `src/daemon/mod.rs` | IK |
| `src/daemon/watcher.rs` | IL |
| `src/daemon/resource_guard.rs` | IM |
| `src/cli/models.rs` | Pre-existing compile error |

---

## NOT Fixed

| Issue | Reason |
|-------|--------|
| I3 | No fix instructions or file location provided in the request |
| I4 | No fix instructions or file location provided in the request |
| I5 | No fix instructions or file location provided in the request |
| Minor issues | No specific Minor issues were listed in the request |

---

## Verification

- `cargo check --features icu_segmenter,lindera` — passes (one benign `dead_code` warning for the now-unused `request_tx` field in `DaemonHandlerImpl`)
- `cargo test --features icu_segmenter,lindera` — all tests pass across all test suites (unit, cli, config_paths, daemon_control, and others)
