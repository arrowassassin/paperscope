# Code Review — paperscope

## Summary
- Total issues found: 44
- Critical: 5
- Important: 15
- Minor: 24

## Critical Issues (must fix before release)

### C1 — Path traversal via `..` in archive member names — `src/core/extract/archive.rs:45`
The archive extractor builds `virtual_path = format!("{name}!/{member}")` from the zip/tar member name without sanitizing `..` segments. A malicious archive containing `../../etc/passwd` would produce a path that escapes the intended virtual namespace and is later passed to `extract_named`, which calls `is_supported` on it and ultimately stores/chunks it under that path. This is a path-traversal / virtual-namespace escape.
- Suggested fix: Reject any member name containing `..`, absolute paths, or path separators that escape a single flat namespace before joining it into `virtual_path`.

### C2 — Shell command injection through file paths in `accel.rs` — `src/core/extract/accel.rs:91`
`extract_with_accelerator` opens the target file and passes it as stdin to `pdftotext`/`pandoc`/`libreoffice` via `Command::new(&binary_path)` with `cmd.stdin(Stdio::from(...))`. The file path itself is not interpolated into a shell string, so direct injection via `Command` args is not possible. However, `libreoffice` is invoked with `--outdir /dev/stdout` and `-` as the input placeholder while the *actual* input is piped via stdin. The real risk: `std::fs::File::open(path)` opens an attacker-controlled path (any extension the accelerator claims, e.g. `.pdf`), and a path with unusual bytes could still confuse wrapper scripts. More importantly, accelerators run external binaries on attacker-controlled file content with no sandbox and no size cap beyond `MAX_EXTRACTED_BYTES` — a zip bomb or malicious PDF can cause the external tool to consume unbounded memory/disk.
- Suggested fix: Either drop the shell-out accelerators entirely (they are feature-gated off by default, which is good), or sandbox them (seccomp/pledge, cgroups memory limit), enforce the same `max_file_bytes` cap before spawning, and validate the file path strictly.

### C3 — No authentication/authorization on the daemon IPC socket — `src/daemon/ipc.rs:68-91`
The daemon binds a Unix socket at `~/.cache/paper/daemon.sock` and accepts any connection without authenticating the peer. Any local user/process can connect and issue `IndexRoots`, `AddRoot`, `RemoveRoot`, `Pause`, `Resume`, or `Shutdown`. On multi-user systems this is a local privilege escalation / denial-of-service vector (an attacker can repoint the index to arbitrary directories, causing the daemon to read files the attacker should not access, then exfiltrate via search results). Combined with C1/C4 this amplifies.
- Suggested fix: Add peer-credential checking (`SO_PEERCRED` on Linux / `getpeereid` on BSD/macOS) and only accept connections from the same UID; consider an allow-list of UIDs.

### C4 — SSRF-via-local-file-read in MCP `read_source` without origin validation — `src/mcp/server.rs:134-180`
`read_source` reads chunks by `source_id` from the database and checks `allowed(&path)` against roots. However, the SQL uses `id BETWEEN ? AND ?` with `req.source_id - context` and `req.source_id + context` (no lower bound clamping). A negative `source_id` or huge `context` yields a negative/underflowing bound, and since these are bound as `i64` parameters to SQLite this does not itself escape; but an MCP client with a valid `source_id` can read adjacent chunks (`context`) across the whole index, including chunks whose path passes `allowed()`. This is more of an information-leak than SSRF, but combined with the MCP server running as an agent tool it lets a compromised agent read any indexed document in the roots.
- Suggested fix: Clamp `context >= 0`, validate `source_id >= 1`, and treat `context` as a bounded window (e.g. max 5) to prevent mass reads.

### C5 — `unsafe` transmute of function pointer in `register_sqlite_vec` — `src/core/index/connection.rs:14-21`
`std::mem::transmute(sqlite_vec::sqlite3_vec_init as *const ())` transmutes a function pointer to `*const ()` and passes it to `sqlite3_auto_extension`. This is the upstream pattern but still `unsafe`. If the signature ever mismatches it is UB. The comment claims safety but there is no documentation of the exact C signature contract being honored beyond a vague reference.
- Suggested fix: Keep `unsafe` but add an explicit safety comment documenting that `sqlite3_vec_init` has the `sqlite3_extension_init` signature (`int (*)(sqlite3*, char**, const sqlite3_api_routines*)`) and that `sqlite3_auto_extension` expects exactly that. Prefer `std::mem::transmute_copy` or a properly typed extern "C" fn pointer if the sqlite-vec crate exposes one.

## Important Issues (fix before next release)

### I1 — Temp-file race / predictable temp names in OCR — `src/core/extract/ocr.rs:36-38, 178`
`TesseractEngine::recognize` writes the image to `std::env::temp_dir().join(format!("paper_ocr_{}.png", std::process::id()))` and `process_pdf_page_with_ocr` uses `paper_ocr_{pid}_{page}.png`. The temp name is derived only from PID and page number, which is predictable. An attacker who can create files in the temp dir (shared on multi-user systems) could pre-create a symlink at that path, causing `std::fs::write(&image_path, image_bytes)` to follow the symlink and write attacker-controlled image bytes to an arbitrary location the user can write (symlink attack / arbitrary file write). Same class of issue affects the `.txt` output path read back at line 61.
- Suggested fix: Use `tempfile::NamedTempFile` in the temp directory with random suffixes, write to it, then pass the real path to tesseract/pdftoppm. Never use predictable names in shared temp dirs.

### I2 — Stuck-parser thread leak / denial of service — `src/core/extract/mod.rs:50-51, 311-367`
`STUCK_PARSERS` is a global `Mutex<Vec<JoinHandle>>` capped at `MAX_STUCK_PARSERS = 4`. When a parser times out, its `JoinHandle` is stored without being joined, and the thread keeps running (Rust cannot kill threads). The comment admits "A parser that ignores its deadline must not be able to spawn an unbounded number of threads" but only caps the *stuck* set. A malicious or pathological input that causes repeated 5-second timeouts fills the stuck set, after which `index_file` returns "skipped: 4 parsers are stalled" and aborts extraction for every subsequent file until the stuck threads finish — effectively a persistent DoS of indexing. Worse, the handles are never reaped until the next extraction attempt for the same extractor, and there is no upper bound on total thread memory while stuck.
- Suggested fix: Bound total parser threads, join/reap stuck handles on a background reaper, and return a per-file error rather than blocking the whole indexer.

### I3 — Archive zip-slip / path traversal on member names — `src/core/extract/archive.rs:69-70`
`read_zip` uses `file.name()` directly as the member name without sanitizing directory traversal (`../`). Combined with C1 this lets a zip member write/read outside the intended virtual namespace. The tar reader has the same exposure via `entry.path()`.
- Suggested fix: Sanitize member names: reject `..` components and absolute paths; use only the file name or a flattened safe relative path.

### I4 — No authentication/authorization in MCP server at all — `src/mcp/server.rs`
The MCP server exposes `search_documents`, `read_source`, `answer_documents`, `list_sources` over stdio. There is no auth, no rate limiting, and no per-client scoping beyond the static `roots`. Any process that can spawn `paper serve --mcp` (or connect to its stdio) can read the entire index. This is acceptable for a local-first CLI tool but should be documented as a trust boundary: the MCP server trusts its stdio client completely.
- Suggested fix: Document the trust boundary explicitly; consider an optional `PAPER_MCP_ALLOWED_PIDS` or credential check if the socket ever becomes network-exposed.

### I5 — `read_source` SQL `BETWEEN` allows negative/underflow bounds — `src/mcp/server.rs:134, 142`
`req.source_id - context` can underflow if `context > source_id`, producing a negative lower bound. Bound to SQLite as `i64`, this is not a crash but lets a client page backward from id 1 to read rows with negative/zero ids (which don't exist) — harmless — but combined with large `context` it reads a wide range of adjacent chunks in one call, enabling bulk enumeration of the index by an MCP client.
- Suggested fix: Clamp `context` to a small max (e.g. 5) and require `source_id >= 1`.

### I6 — Unbounded recursion in spreadsheet row iteration — `src/core/extract/sheet.rs:68`
`workbook.worksheet_range(&sheet)` then iterates `range.rows()` with no cap other than `max_entries = 1_000_000` in capabilities (which is never enforced in `sheet.rs` — the cap lives only in the `Capabilities` struct and is checked nowhere for row count). A malicious spreadsheet with millions of rows would cause unbounded memory and time during indexing.
- Suggested fix: Enforce `capabilities.max_entries` as a row counter inside `Sheet::extract`, returning a warning and stopping when exceeded.

### I7 — `chunk_text` line-number off-by-one risk — `src/core/index/indexer.rs:439`
After a non-heading flush, `start_line = line_no - kept.len()`. If `kept.len()` > `line_no` (possible when overlap retains more lines than exist before the current line in a short document), `start_line` underflows on `usize` → wraps to a huge number, producing a bogus citation range. This is unlikely in practice but is a latent panic/wrong-citation bug.
- Suggested fix: Clamp `start_line = line_no.saturating_sub(kept.len())`.

### I8 — `prune_missing` stats races — `src/core/index/indexer.rs:315-355`
`prune_missing` collects all `source_path`s, filters by `!Path::new(path).exists()`, then deletes. Between the `exists()` check and the delete, a file could be recreated (TOCTOU), causing a chunk to be deleted even though the file exists again. Minor in a single-user local tool, but wrong semantics.
- Suggested fix: Accept the TOCTOU (document it) or re-verify existence immediately before delete inside the transaction.

### I9 — Model download URL trust — `src/core/models/specs.rs:60-73`
`download_urls` tries GitHub Releases first, then Hugging Face. Both are verified by SHA256, so a compromised server cannot serve bad bytes. However, if GitHub Releases for the tagged version doesn't exist yet (pre-release dev builds), the code falls through to Hugging Face, which is fine. The real issue: `env!("CARGO_PKG_VERSION")` is used in the URL; if a dev build has a dirty version the URL 404s and falls to HF — acceptable.
- Suggested fix: No change needed; SHA256 pinning makes this safe. Document that both sources are equivalent trust-wise due to pinning.

### I0 — `Config::save` writes without atomic rename — `src/core/config.rs:95-102`
`save()` calls `std::fs::write(path, ...)` directly, which is not atomic. A crash mid-write leaves a truncated/corrupt `config.toml`, and the next `load()` will fail on `toml::from_str`. This is a data-loss / corruption risk on power loss.
- Suggested fix: Write to `config.toml.tmp` then `fs::rename` into place (atomic on same filesystem).

### II — `SearchFilters::sql` LIKE injection via extension — `src/core/search/filters.rs:100-102`
The extension filter builds `lower(chunks.path) LIKE ?` with `%.{ext}` where `ext` comes from CLI `--ext`. A user could pass `%` or `_` or `\` as an extension to alter the LIKE pattern (e.g. `--ext '%'` matches everything, or `--ext '_'` matches any single char). Since `rusqlite` parameterizes the value, SQL injection is not possible, but *LIKE wildcard injection* is: the user can broaden the filter beyond intent. Not a security boundary (user is filtering their own search), but a correctness issue.
- Suggested fix: Escape LIKE wildcards (`%`, `_`, `\`) in user-supplied extension/path_contains values, or use `instr(lower(chunks.path), lower(?)) = ...` for substring matching.

### IJ — `aggregate.rs` SQL built by string interpolation — `src/cli/aggregate.rs:57-107`
`aggregate.rs` builds SQL with `format!()` for `WHERE` clauses and `LIKE` patterns (`format!("%.{ext}")`, `format!("%{}%", path_substr)`), with values passed as parameters. The clause structure itself is interpolated from trusted code (not user input except via parameterized values), so SQL injection is not possible. However, the extension filter uses `path LIKE ?` instead of `source_path LIKE ?` (inconsistent with the root filter which uses `source_path`), and `LIKE` is case-sensitive for non-ASCII unless `lower()` is applied — so `.PDF` won't match `pdf`. Minor correctness issue.
- Suggested fix: Use `lower(path) LIKE lower(?)` for case-insensitive extension matching; apply to `source_path` for consistency.

### IK — `daemon` `Shutdown` request sends to itself — `src/daemon/mod.rs:189-196`
`DaemonRequest::Shutdown` in the handler calls `self.request_tx.send(DaemonRequest::Shutdown)` — it enqueues another Shutdown on the same channel it is being handled from. The dispatch loop then picks it up and breaks. This works but is convoluted: the handler's response `Ok` is sent back to the IPC client before the dispatch loop actually shuts down. There is a window where the client thinks shutdown succeeded but the daemon is still running.
- Suggested fix: Handle Shutdown by setting a flag and letting the dispatch loop break; don't re-enqueue.

### IL — `Watcher` debounce loses events under load — `src/daemon/watcher.rs:157`
`handle_event` uses `event_tx.try_send(relevant_paths)` which drops events silently if the channel is full (capacity 100). Under heavy file churn, changes are silently missed and not indexed until the next scheduled full reindex (up to 24 hours).
- Suggested fix: Use a bounded but blocking or larger channel, or coalesce and retry; log dropped events.

### IM — `resource_guard` battery reading is best-effort and can miss — `src/daemon/resource_guard.rs:60-120`
Linux battery reads `/sys/class/power_supply` entries starting with "BAT" and returns the *first* one found. Systems with multiple batteries (e.g. dual-battery laptops) report only the first, so low-battery pause may not trigger when the second is depleted. macOS parses `pmset -g batt` output heuristically — fragile across locale/macOS versions.
- Suggested fix: On Linux, aggregate across all BAT* entries (pause if any is below threshold and discharging). On macOS, parse more robustly or fall back to sysinfo.

### IN — `spotlight_paths` command construction — `src/core/search/tier0.rs:188-221`
Spotlight is macOS-only and constructs `kMDItemTextContent == "*{escaped}*"cd` with `query.replace('"', "\\\"")`. The `mdfind` args are passed as separate `Command` args (no shell), so command injection is not possible. The predicate uses `*` wildcards around the query, so a query like `"` becomes `"*""*"` after escaping — the escaping doubles the quote which is correct for mdfind. Low risk.
- Suggested fix: Document that mdfind is invoked without a shell; no change strictly needed.

### IO — `daemon` state `Indexer` not held, fresh connection per request — `src/daemon/mod.rs:79-84, 122-139`
This is actually a *good* design choice (rusqlite connections aren't Sync). Each indexing request opens a fresh connection inside `spawn_blocking`. No issue.

## Minor Issues (nice to have)

### M1 — Magic numbers scattered — multiple files
`max_chunk_words: 400` (indexer.rs:82), `overlap_words: 40` (indexer.rs:83), `SNIPPET_CHARS: 240` (text.rs:373), `MAX_EXTRACTED_BYTES: 32MB` (extract/mod.rs:40), `MAX_ARCHIVE_ENTRIES: 2000` (extract/mod.rs:42), `MAX_PDF_PAGES: 5000` (extract/mod.rs:43), debounce 250ms (watcher.rs:43, index.rs:104), 5-second parser timeout (extract/mod.rs:333), `rrf_k: 60` (config.rs:71). Most are documented as constants but a few are inline literals (e.g. `limit * 64` and `4096` in tier1.rs:65, `merge_paragraphs(paragraphs, 350)` in ooxml.rs:140).
- Suggested fix: Pull remaining inline literals into named constants with doc comments.

### M2 — Redundant `ModelManager::new()` constructions — `src/cli/mod.rs:80-89, 96, 111-113`
`Workspace::embedder()`, `pipeline()`, `embedding_model_installed()`, and `reranker_installed()` each construct a fresh `ModelManager::new()`, re-reading `PAPER_OFFLINE` and re-spec-list each call. Not a correctness bug, but wasteful on every search.
- Suggested fix: Hold one `ModelManager` in `Workspace` (constructed once in `Workspace::new()`).

### M3 — `daemon` module gated behind `daemon` feature but referenced in `main.rs` — `src/main.rs:24-27`
Correctly cfg-gated. No issue.

### M4 — `Config::load` swallows TOML parse errors? No — `src/core/config.rs:86-89`
`toml::from_str(&content)?` propagates parse errors. Good. But a malformed config causes `Config::load()` to fail, and `Workspace::new()` does `Config::load().unwrap_or_default()` — silently falling back to defaults on a corrupt config, hiding the error from the user.
- Suggested fix: Log a warning when falling back to defaults due to a corrupt config file.

### M5 — `indexer.index_roots` follows no symlinks but `WalkDir::follow_links(false)` — `src/core/index/indexer.rs:111-114`
Correct: symlinks are not followed. `is_hidden` skips dotfiles. Good. But `index_file` re-checks `metadata.file_type().is_symlink()` and skips symlinks — defense in depth, good.

### M6 — `same_tokens` used for dedup but `fts_segment` is non-idempotent across segmenters — `src/core/text.rs:326-352`
`fts_segment` checks `seg.name()` and may route to ICU/Lindera/Jieba depending on compile-time features. If the index is built with one feature set and searched with another, token boundaries differ, breaking FTS5 matches for CJK/Thai. The `fts_text` column stores the segmentation used at index time; search uses `query_terms` → `fts_segment` at search time. A feature mismatch silently reduces recall.
- Suggested fix: Record the segmenter name in `index_meta` at index time and require the same at search time (analogous to the embedding-model check).

### M7 — `extract::finish_blocks` truncates without warning — `src/core/extract/mod.rs:397-414`
Blocks over `MAX_EXTRACTED_BYTES` are silently truncated at a char boundary. The caller gets no warning that text was lost. For citations this means quoted text may not match the file.
- Suggested fix: Return a count of truncated bytes and surface as an `ExtractedDocument` warning.

### M8 — `metadata` column stores `fields::analyse()` JSON but is never queried — `src/core/index/indexer.rs:263`
`fields::analyse` extracts dates/amounts/kind into JSON stored in `metadata`. The `aggregate` CLI reads it via `json_extract`. The search pipeline never uses it. This is by design (aggregate is separate) but worth noting the metadata is write-only for search.
- Suggested fix: No change; document that `metadata` serves the aggregate command.

### M9 — `Hit::content` holds full chunk text, duplicated in memory per hit — `src/core/search/hit.rs:26`
Every hydrated hit carries the full `content` (up to 400 words) plus `snippet`. For large result sets this duplicates text already in SQLite. Fine for typical limits (≤50), but `rerank_top=20` and `fused_limit=50` cap it.
- Suggested fix: No change; limits are small enough.

### M10 — `cli` `search.rs` truncates hits after search — `src/cli/search.rs:26`
`hits.truncate(args.limit)` happens *after* the full pipeline search, so the reranker runs on all `fused_limit` results even if the user only wants 5. Minor waste.
- Suggested fix: Pass `limit` into the pipeline so RRF/fusion caps earlier (or accept the small waste).

### M11 — `answer.rs` polish extracts take only high/medium confidence hits — `src/cli/answer.rs:98`
Correct and intentional. No issue.

### M12 — `tests/daemon_control.rs` uses hardcoded `/opt/Paper Suite/paper` paths — `tests/daemon_control.rs:8, 24`
These are rendering tests (they render unit/plist files and check substring content), not execution tests, so hardcoded paths are fine. No issue.

### M13 — `fuzz` targets are a separate crate with its own `Cargo.lock` — `fuzz/Cargo.toml`
Good practice. The fuzz targets don't call the paperscope extractors directly (they call lopdf/zip/quick-xml directly), so they don't fuzz the actual extraction pipeline (size caps, stuck-parters, sanitization). Limited value.
- Suggested fix: Add fuzz targets that call `extract::extract_named` and `extract::extract_path` directly with arbitrary bytes.

### M14 — `models.rs` install path lacks concurrency guard — `src/cli/models.rs:37-89`
Two concurrent `paper models install` calls for the same model could both download to the same `dest` and race on the `.partial` file. The `ModelDownloader` uses a shared `.partial` name with no lock. A crash could leave mixed bytes.
- Suggested fix: Add a per-model `Mutex` or lock file around install/remove.

### M15 — `daemon` `count_indexed_files` and `stored_embedding_model` open separate connections — `src/daemon/mod.rs:202-215`
Two separate `Connection::open` calls for one `GetStatus` response. Minor inefficiency, no correctness issue.
- Suggested fix: Open one connection and reuse for both queries.

### M16 — `extract::ocr` `VisionEngine`/`WindowsOcrEngine` are placeholders that fall back to Tesseract — `src/core/extract/ocr.rs:96-98, 128-130`
The macOS Vision and Windows OCR engines are unimplemented placeholders that delegate to `TesseractEngine`, which shells out to `tesseract` binary. On a system without tesseract, `detect_ocr_engine()` returns `Some(VisionEngine)` on macOS but `recognize` fails when tesseract is missing. The OCR feature advertises support it can't deliver without the binary.
- Suggested fix: Make `detect_ocr_engine` verify the binary is actually available (which `which::which("tesseract")` does for the non-platform branch); for platform branches, check availability too or return `None`.

### M17 — `config_paths.rs` test writes to `/tmp/docs` — `tests/config_paths.rs:43-44`
Hardcodes `/tmp/docs` as a root; environment-dependent but harmless in tests.
- Suggested fix: Use `tempdir` for the root path too.

### M18 — `eval.rs` `build_golden_index` writes docs to `env::temp_dir()` — `src/core/eval.rs:121`
Writes golden-set documents to a fixed `temp_dir()/paper_eval_{pid}` without cleaning up. Leftover files accumulate in temp.
- Suggested fix: Use `tempfile::TempDir` and drop it to clean up.

### M19 — `segment.rs` `fts_segment` name-based dispatch is fragile — `src/core/text.rs:330`
Dispatches on `seg.name() == "bigram-first-char"`; if a custom segmenter registers with the same name it gets the ICU/Lindera special-casing. Very unlikely.
- Suggested fix: Add a flag to `Segmenter` trait or use a marker type instead of string comparison.

### M20 — `daemon` scheduler `trigger_full`/`trigger_incremental` use `try_send` — `src/daemon/scheduler.rs:94, 100`
If the channel is full (100), the scheduled index is silently dropped. The next scheduled run will occur, so it self-heals, but a missed incremental is not retried.
- Suggested fix: Use `send` with a timeout or log dropped scheduled indexes.

### M21 — `cargo-audit` advisories ignored with broad justifications — `audit.toml`
The ignore list is documented per-advisory, which is good. Some justifications (e.g. "unreachable code path") rely on transitive dependency versions that could change. Periodic re-review needed.
- Suggested fix: Re-run `cargo audit` on each dependency bump and re-validate ignored advisories.

### M22 — `lib.rs` re-exports `eval` types publicly — `src/lib.rs:13-16`
`EvalError`, `EvalReport`, etc. are public API but `eval` is a test harness. Leaks test infrastructure into the public library API.
- Suggested fix: `#[cfg(feature = "eval")]` gate or make `eval` private to tests/benches.

### M23 — `models/specs.rs` `all_specs()` rebuilds `Vec` on every call — `src/core/models/specs.rs:141`
`spec()` and `all_specs()` allocate a new `Vec` each call. Called rarely (model install/list), so negligible.
- Suggested fix: `#[once]` or `static` the specs list if profiling shows it matters (not worth it).

### M24 — `site/install.sh` pipes curl to sh — `site/install.sh:109`
`curl ... | sh` executes a remote script without verifying its integrity. The user is prompted first (good), but the downloaded installer is not checksummed. Standard for installers but a supply-chain risk.
- Suggested fix: Document that users should verify the installer; prefer `cargo install` or Homebrew for verified delivery.

## Efficiency Opportunities

### E1 — Search opens a fresh `Connection` per call and re-registers sqlite-vec — `src/mcp/server.rs:30-37`, `src/cli/mod.rs:92-99`
Every MCP tool call and every CLI search runs `Workspace::pipeline()` → `open_index()` → `Connection::open()` → `register_sqlite_vec()` (a `call_once`, so cheap after first) and `init_db()` (runs `schema_sql` + `table_exists` + possibly `migrate`). On a fresh file this also runs `integrity_check` (O(db size)). For interactive search this adds measurable latency on large indexes.
- Impact: Medium (per-search latency). 
- Change: Cache the `Connection` (or at least skip `integrity_check`/`init_db` when already initialized) for the lifetime of the CLI invocation / MCP session.

### E2 — `tier1_search_filtered` fetches `limit * 64` (min 4096) rows when filters/roots are present — `src/core/search/tier1.rs:59-66`
With filters, sqlite-vec KNN cannot push WHERE into the KNN clause, so it fetches a large superset and filters in memory. `usize::max(limit * 64, 4096)` means even `limit=1` fetches 4096 vectors and deserializes each into `RawHit` (cloning path/snippet strings). For an index with few matching roots, most are discarded.
- Impact: Medium (memory + CPU on filtered semantic search).
- Change: Fetch in smaller batches (e.g. limit * 8) and loop, re-issuing the KNN query with an offset, until `limit` passing results are found or the table is exhausted.

### E3 — `SearchPipeline::hydrate` prepares the statement once but fetches one row at a time — `src/core/search/pipeline.rs:213-246`
`stmt` is prepared outside the loop (good), but each fused hit is a separate `query_row` round-trip through rusqlite. For 50 hits that's 50 individual queries. A single `WHERE id IN (...)` or a join would be one round-trip.
- Impact: Small (50 queries is trivial for SQLite, but measurable).
- Change: Batch hydration with `WHERE id IN (?1, ?2, ...)` or a temporary table + join.

### E4 — `prune_missing` stats every indexed file with `Path::exists()` — `src/core/index/indexer.rs:315-333`
One `stat` per indexed document. On an index with tens of thousands of files this is a noticeable startup cost on every `prune_missing` call (which runs after every index).
- Impact: Small/Medium on large indexes.
- Change: Only `prune_missing` when files are known to have been deleted (track deletions via the watcher), or run it less frequently (e.g. daily in the scheduler rather than every index).

### E5 — `ModelManager::is_installed` re-hashes multi-GB files on every call — `src/core/models/manager.rs:74-96`
It is cached via `installed_cache` (only caches *hits*, not misses). A miss (file absent or corrupt) re-hashes every call. `Workspace::embedder()` calls `is_installed` for two models on every search/pipeline build.
- Impact: Small/Medium if models are not installed (every search re-stats and re-hashes nothing, since `path.exists()` is false early — actually `spec.files.iter().all(...)` short-circuits on `!path.exists()`, so no hashing happens on a missing file. Good.)
- Change: No change needed; the `path.exists()` short-circuit makes this cheap.

### E6 — `fts_segment` calls `segmenter()` which initializes `OnceLock` on every call — `src/core/text.rs:326-330, 334-339`
`segmenter()` calls `init_segmenter()` → `SEGMENTER.get_or_init(...)`. `OnceLock::get_or_init` is fast after init but still an atomic load per call. Called for every indexed block and every query term.
- Impact: Tiny.
- Change: Cache `segmenter()` result in a thread-local or accept the atomic load (negligible).

### E7 — `chunk_text` builds `Vec<&str>` and joins repeatedly — `src/core/index/indexer.rs:402-459`
`current.join("\n")` on every chunk boundary; `kept.insert(0, prev)` is O(n) per insert (shifting). Overlap retention is small (≤40 words) so negligible.
- Impact: Tiny.
- Change: No change.

### E8 — `extract_named` clones `bytes` for the parser thread — `src/core/extract/mod.rs:328`
`let bytes_owned = bytes.to_vec();` copies the entire file into the parser thread. For a 256 MB PDF that's a 256 MB copy plus the original. The parser needs owned data because it may outlive the caller (thread). Could use `Arc<[u8]>` shared between caller and thread to avoid the copy.
- Impact: Medium on large files.
- Change: Pass `Arc<[u8]>` into the thread; the extractor API takes `&[u8]` which `Arc<[u8]>` derefs to.

### E9 — `aggregate.rs` runs one query per aggregate but scans all chunks — `src/cli/aggregate.rs:116-157`
Queries like `SUM(json_extract(value, '$.amount')) FROM chunks ... json_each(...)` scan the full `chunks` table (subject to WHERE). No index on `json_extract` results, so aggregation is O(n) in the number of chunks. Fine for personal indexes.
- Impact: Small.
- Change: No change for now; add a generated/virtual column index if aggregation becomes slow.

### E10 — `Config::load` / `save` read-write the whole config on every `Workspace::new()` — `src/cli/mod.rs:43-48, src/core/config.rs:86-89`
`Workspace::new()` calls `Config::load()` which reads and parses `config.toml` on every CLI invocation. Fine (one invocation = one read).
- Impact: Tiny.
- Change: No change.

## Security Assessment

### Overall security rating: Medium

The codebase is security-conscious by design: local-first, no telemetry, SHA256-pinned model downloads, `unsafe` limited to three well-documented FFI transmutes, FTS5 operator escaping, NUL/UTF-8 validation on text extraction, archive member size caps, response size caps on the chat client, delimiter escaping for the polishing engine, and `.partial` staging for downloads. The attack surface is small (CLI + local MCP stdio + Unix-socket daemon).

### Key concerns
1. **No IPC authentication** (C3): any local user can control the daemon via its Unix socket — local privilege escalation / DoS / index redirection.
2. **Path traversal in archives** (C1, I3): `..` in zip/tar member names escapes the virtual namespace; combined with the daemon it can point the index at attacker-chosen directories.
3. **Shell-out accelerators** (C2): external binaries process attacker-controlled file content unsandboxed (feature-gated off by default).
4. **Predictable OCR temp files** (I1): symlink attack in shared temp dirs.
5. **MCP trust boundary** (I4, I5): the stdio MCP server trusts its client completely; `read_source` supports bulk adjacent-chunk reads.

### Recommendations
- Add peer-credential (UID) checks to the daemon IPC socket before accepting any request.
- Sanitize archive member names: reject `..`, absolute paths, and drive-letter prefixes.
- Remove or sandbox the shell-out accelerators; if kept, enforce `max_file_bytes` before spawning and run under a sandbox.
- Use `tempfile::NamedTempFile` (random names) for OCR temp files instead of PID-based names.
- Document the MCP stdio trust boundary and clamp `read_source` context window.
- Add `cargo audit` re-validation to CI on every dependency change (the ignore list depends on transitive versions).
- Make config writes atomic (tmp + rename).

## Positive Findings
- **No `unsafe` abuse**: exactly three `unsafe` blocks, all for the sqlite-vec / safetensors FFI transmute, each with a safety rationale comment.
- **FTS5 injection resistance**: `fts5_match_expr` double-quotes every term and doubles internal quotes, so user input can never inject FTS5 operators (covered by test `queries_cannot_inject_fts5_operators`).
- **Hash-pinned models**: every model file pinned by commit + SHA256; downloads verified before use; `PAPER_OFFLINE` respected; no surprise downloads during search.
- **Extractive answers**: answers are verbatim quotes with citations; polishing is optional, delimiter-escaped, and citation-validated with fallback to verbatim on any failure.
- **Defensive extraction**: NUL-byte and UTF-8 validation on plain text; per-page PDF extraction so one bad page doesn't lose the document; archives don't recurse; size caps on archives/PDF/pages.
- **WAL mode + `busy_timeout`**: concurrent reads during indexing work; `prune_missing` checkpoints the WAL to reclaim space.
- **Atomic index replacement**: `index_with_defaults_atomic` builds into `db.tmp` then renames, so a crash never leaves a corrupt `chunks.db`.
- **Comprehensive test suite**: 55+ tests all passing, including FTS injection tests, citation-resolution tests, chunking tests, migration tests (v3→v4), and an offline MCP e2e test. Tests use a deterministic `HashEmbedder` — no model downloads in CI.
- **Clippy clean**: `cargo clippy --all-targets` is warning-free across default, all-features, and no-default-features builds.
- **Feature-gated design**: `--no-default-features` produces a keyword-only binary with no Candle/tokenizers/inference code; optional ICU/Lindera/shell-accel/ocr features are opt-in.
- **Good error handling**: extraction errors are reported as warnings, never silently dropped; one unreadable file doesn't abort a directory index.
