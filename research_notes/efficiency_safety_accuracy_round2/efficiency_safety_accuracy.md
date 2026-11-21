# Efficiency, Safety, and Accuracy Improvements — paperscope v0.2.2

**Status:** Draft — research round 2, grounded in `CODE_REVIEW.md` (44 findings) and the `src/` tree  
**Scope:** `paperscope` Rust crate, binary `paper`, crate version 0.2.2, MSRV 1.94, edition 2021  
**Sources consulted:** source tree (`src/`, `tests/`, `benches/`), `Cargo.toml`/`Cargo.lock`, `CODE_REVIEW.md`, `AGENTS.md`, OWASP Path Traversal Prevention Cheat Sheet, OWASP RAG Security Cheat Sheet, OWASP Error Handling Cheat Sheet, Microsoft min-sized-rust release-profiles guidance, sqlite-vec docs.

---

## Priority 1 — Critical Safety (fix before any release)

### [C1] Path traversal via `..` in archive member names — Virtual-namespace escape
**File:** `src/core/extract/archive.rs:45` (and `read_zip`/`read_tar` at lines 69 and 97)  
**Impact:** High. A malicious zip/tar containing `../../etc/passwd` or `../../escape.txt` produces `virtual_path = format!("{name}!/{member}")`, which escapes the intended flat virtual namespace. The member is then passed to `extract_named`, chunked, and stored under that path. Combined with the unauthenticated daemon IPC (C3) an attacker who can cause the daemon to index a hostile archive gains read access outside the configured roots.  
**Detailed recommendation:** Sanitize every archive member name before joining it into `virtual_path`. Reject names containing `..` path components, absolute paths, drive-letter prefixes (`C:\`), Windows reserved names (`CON`, `NUL`, etc.), and symlink/hardlink entries. Use only a flattened safe relative path (basename or a collapsed `..`-free relative). Log rejected entries as warnings; never silently extract.

```rust
fn safe_member_name(member: &str) -> Option<String> {
    let parts: Vec<&str> = member.split('/').collect();
    let mut safe = Vec::new();
    for part in parts {
        match part {
            "" | "." => continue,          // skip empty / current-dir segments
            ".." => return None,            // traversal attempt
            _ => safe.push(part),
        }
    }
    // Reject reserved / dangerous names and anything with backslashes on Windows.
    let name = safe.join("/");
    if name.is_empty() || name.contains('\\') || name.contains(':') {
        return None;
    }
    if is_reserved_device_name(&name) { return None; }
    Some(name)
}
```

Apply in `read_zip` (line 69) and `read_tar` (line 97) before pushing the member, and again in `Archive::extract` (line 45) before `format!("{name}!/{member}")`.

### [C2] Shell command injection / unsandboxed accelerators on attacker-controlled content
**File:** `src/core/extract/accel.rs:91`  
**Impact:** High. `extract_with_accelerator` opens an attacker-controlled path (any extension the accelerator claims, e.g. `.pdf`) and pipes it to `pdftotext`/`pandoc`/`libreoffice` via `Command`. While `Command` args avoid direct shell interpolation, `libreoffice` uses `--outdir /dev/stdout` with `-` as the input placeholder and stdin piping, and the accelerators run external binaries on attacker-controlled content with no sandbox, no size cap beyond `MAX_EXTRACTED_BYTES`, and no timeout. A zip bomb or malicious PDF can cause unbounded memory/disk consumption in the external tool.  
**Detailed recommendation:** Either (a) drop the shell-out accelerators entirely (they are feature-gated off via `shell-accel`, which is good), or (b) keep them only behind the gate and harden: enforce `max_file_bytes` before spawning, add a hard wall-clock timeout, run the subprocess under an OS sandbox (seccomp-bpf/pledge on Linux, `sandbox-exec` on macOS, job objects on Windows), cap subprocess RSS, and validate the file path strictly (reject NUL bytes, reject paths outside roots). Log accelerator use as a security-relevant event.

### [C3] No authentication/authorization on daemon IPC socket
**File:** `src/daemon/ipc.rs:68-91`  
**Impact:** High. The daemon binds `~/.cache/paper/daemon.sock` and accepts any connection without peer authentication. Any local user/process can connect and issue `IndexRoots`, `AddRoot`, `RemoveRoot`, `Pause`, `Resume`, or `Shutdown`. On multi-user systems this is local privilege escalation / DoS / index-redirection: an attacker can repoint the index to directories they should not read, then exfiltrate content via search results. Combined with C1/C4 this is amplified.  
**Detailed recommendation:** Add peer-credential checking. On Linux use `SO_PEERCRED` via `getsockopt` on the accepted `UnixStream`; on BSD/macOS use `getpeereid`. Only accept connections from the same UID as the daemon process; optionally allow-list additional UIDs. Reject (do not process) any request from a different UID and log the attempt. Do not bind the socket to a world-writable path; create the parent directory `0700`.

```rust
#[cfg(unix)]
fn peer_uid(stream: &tokio::net::unix::OwnedReadHalf) -> Option<u32> {
    use std::os::unix::io::AsRawFd;
    let raw = stream.as_raw_fd();
    let mut cred: libc::ucred = std::mem::zeroed();
    let mut len = std::mem::size_of::<libc::ucred>();
    unsafe {
        if libc::getsockopt(raw, libc::SOL_SOCKET, libc::SO_PEERCRED,
                            &mut cred as *mut _ as *mut _, &mut len) == 0 {
            Some(cred.uid)
        } else { None }
    }
}
```

### [C4] MCP `read_source` adjacent-chunk bulk read without clamped window
**File:** `src/mcp/server.rs:134-180`  
**Impact:** Medium-High (information leak). `read_source` runs `SELECT ... FROM chunks WHERE id BETWEEN ? AND ?` with bounds `req.source_id - context` and `req.source_id + context`. `context` is unbounded (`Option<usize>` cast to `i64`), so a malicious MCP client can page backward from id 1 or request a huge `context` to read a wide range of adjacent chunks in one call, enabling bulk enumeration of the index. While `allowed(&path)` gates each row, the client can still harvest every chunk whose path passes the root allow-list.  
**Detailed recommendation:** Clamp `context >= 0` and cap it to a small maximum (e.g. 5). Require `source_id >= 1`. Reject `context > 5` with `invalid_params`. This keeps the neighbour-context feature while preventing mass reads.

### [C5] `unsafe` transmute of function pointer in `register_sqlite_vec`
**File:** `src/core/index/connection.rs:14-21`  
**Impact:** Low-Medium. `std::mem::transmute(sqlite_vec::sqlite3_vec_init as *const ())` transmutes a function pointer to `*const ()` and passes it to `sqlite3_auto_extension`. If the signature ever mismatches it is undefined behavior. The existing comment references upstream but does not document the exact C signature contract.  
**Detailed recommendation:** Keep the `unsafe` (it matches upstream sqlite-vec), but add an explicit safety comment documenting that `sqlite3_vec_init` has the `sqlite3_extension_init` signature `int (*)(sqlite3*, char**, const sqlite3_api_routines*)` and that `sqlite3_auto_extension` expects exactly that. Add `#![forbid(unsafe_code)]` at crate level with `#[allow(unsafe_code)]` on the three specific modules (`connection.rs`, `embedder.rs`, `reranker.rs`), each with a documented justification. Run clippy with `-D unsafe_op_in_unsafe_fn` in CI.

---

## Priority 2 — Important Correctness (fix before next release)

### [I1] Predictable OCR temp-file names — symlink / arbitrary-file-write attack
**File:** `src/core/extract/ocr.rs:36-38, 178`  
**Impact:** High. `TesseractEngine::recognize` writes to `temp_dir().join(format!("paper_ocr_{}.png", std::process::id()))` and `process_pdf_page_with_ocr` uses `paper_ocr_{pid}_{page}.png`. The name is predictable (PID + page). An attacker sharing the temp dir can pre-create a symlink at that path; `std::fs::write(&image_path, ...)` follows the symlink and writes attacker-controlled bytes to an arbitrary writable location (symlink attack / arbitrary file write). Same class affects the `.txt` output path read back at line 61.  
**Detailed recommendation:** Use `tempfile::NamedTempFile` in the temp dir with random suffixes, write the image there, pass the real path to tesseract/pdftoppm, read output from a second `NamedTempFile`, and let both be removed on drop. Never use predictable names in shared temp dirs.

### [I2] Stuck-parser thread leak / denial of service
**File:** `src/core/extract/mod.rs:304-377`  
**Impact:** High. `STUCK_PARSERS` (global `Mutex<Vec<JoinHandle>>`, cap 4) stores timed-out parser `JoinHandle`s without joining; the threads keep running (Rust cannot kill threads). Repeated 5-second timeouts fill the stuck set, after which `index_file` returns "skipped: 4 parsers are stalled" and aborts extraction for every subsequent file until the stuck threads finish — a persistent indexing DoS. Handles are never reaped until the next extraction attempt for the same extractor, and there is no upper bound on total thread memory while stuck.  
**Detailed recommendation:** Bound total parser threads, add a background reaper that joins finished handles, and return a per-file error rather than blocking the whole indexer. Replace the global stuck set with a per-extractor semaphore; when a parser times out, drop its handle (leak is bounded by the semaphore) and let the OS reap the thread on process exit, or run parsers in a child process with OS rlimits (RLIMIT_AS/RLIMIT_CPU) so the OS enforces memory/CPU limits.

### [I3] Archive zip-slip / path traversal on member names
**File:** `src/core/extract/archive.rs:69-70`  
**Impact:** High. `read_zip` uses `file.name()` directly; `read_tar` uses `entry.path()` directly. Combined with C1 this lets a zip/tar member escape the virtual namespace.  
**Detailed recommendation:** Same fix as C1 — sanitize member names (reject `..`, absolute paths, drive letters, reserved names) at read time in both `read_zip` and `read_tar`, before storing `(name, data)`.

### [I4] No auth / rate limiting on MCP stdio server
**File:** `src/mcp/server.rs`  
**Impact:** Medium. The MCP server exposes `search_documents`, `read_source`, `answer_documents`, `list_sources` over stdio with no auth, no rate limiting, no per-client scoping beyond static roots. Acceptable for local-first CLI but should be an explicit documented trust boundary: the MCP server trusts its stdio client completely.  
**Detailed recommendation:** Document the trust boundary explicitly in `README.md` and `docs/`. Consider an optional `PAPER_MCP_ALLOWED_PIDS` allow-list or credential check if the socket ever becomes network-exposed.

### [I5] `read_source` SQL `BETWEEN` allows negative/underflow bounds
**File:** `src/mcp/server.rs:134, 142`  
**Impact:** Low-Medium. `req.source_id - context` can underflow when `context > source_id`, producing a negative lower bound bound to SQLite as `i64`. Not a crash but enables backward paging / bulk enumeration. Covered by C4; clamp `context` and require `source_id >= 1`.

### [I6] Unbounded spreadsheet row iteration
**File:** `src/core/extract/sheet.rs:68`  
**Impact:** Medium. `workbook.worksheet_range(&sheet)` iterates `range.rows()` with `max_entries = 1_000_000` declared in `Capabilities` but never enforced in `sheet.rs`. A malicious spreadsheet with millions of rows causes unbounded memory and time during indexing.  
**Detailed recommendation:** Enforce `capabilities.max_entries` as a row counter inside `Sheet::extract`, returning a warning and stopping when exceeded.

### [I7] `chunk_text` line-number off-by-one / underflow risk
**File:** `src/core/index/indexer.rs:439`  
**Impact:** Medium. `start_line = line_no - kept.len()` can underflow on `usize` when `kept.len() > line_no` (overlap retains more lines than exist before the current line in a short document), producing a huge bogus citation range.  
**Detailed recommendation:** Clamp with `start_line = line_no.saturating_sub(kept.len())`.

### [I8] `prune_missing` TOCTOU between `exists()` and delete
**File:** `src/core/index/indexer.rs:315-355`  
**Impact:** Low. Collects all `source_path`s, filters by `!Path::exists()`, then deletes. Between check and delete a file could be recreated, deleting a chunk that should exist. Minor in single-user local tool, wrong semantics.  
**Detailed recommendation:** Accept and document the TOCTOU, or re-verify existence immediately before delete inside the transaction.

### [I9] Model download URL trust — acceptable but document
**File:** `src/core/models/specs.rs:60-73`  
**Impact:** Low. SHA256 pinning makes GitHub Releases / Hugging Fallback equivalent trust-wise; no change needed. Document this.

### [I10] `Config::save` non-atomic write
**File:** `src/core/config.rs:95-102`  
**Impact:** Low-Medium. `std::fs::write(path, ...)` directly is not atomic; a crash mid-write leaves a corrupt `config.toml`.  
**Detailed recommendation:** Write to `config.toml.tmp` then `fs::rename` into place (atomic on same filesystem).

### [I11] LIKE wildcard injection via `--ext`
**File:** `src/cli/aggregate.rs:57-107`, `src/core/search/filters.rs:100-102`  
**Impact:** Low. `LIKE '%.{ext}'` with user-supplied `ext` lets `%`/`_`/`\` broaden the filter (e.g. `--ext '%'` matches everything). Not SQL injection (parameterized), but LIKE wildcard injection / correctness issue.  
**Detailed recommendation:** Escape LIKE wildcards (`%`, `_`, `\`) in user-supplied extension/path_contains values, or use `instr(lower(chunks.path), lower(?)) > 0` for substring matching.

### [I12] `daemon` `Shutdown` re-enqueues itself
**File:** `src/daemon/mod.rs:189-196`  
**Impact:** Low. The `Shutdown` handler sends another `Shutdown` on the same channel it is handling; works but convoluted, with a window where the client thinks shutdown succeeded but the daemon is still running.  
**Detailed recommendation:** Handle `Shutdown` by setting a flag and letting the dispatch loop break; don't re-enqueue.

### [I13] `Watcher` debounce drops events under load
**File:** `src/daemon/watcher.rs:157`  
**Impact:** Medium. `handle_event` uses `try_send` on a capacity-100 channel; under heavy file churn events are silently dropped and not indexed until the next full reindex (up to 24h).  
**Detailed recommendation:** Use a larger/bounded blocking channel or coalesce + retry; log dropped events.

### [I14] `resource_guard` battery reading best-effort / incomplete
**File:** `src/daemon/resource_guard.rs:60-120`  
**Impact:** Low-Medium. Linux reads only the first `BAT*` entry (dual-battery laptops miss low-battery pause). macOS parses `pmset -g batt` heuristically — fragile across locale/versions.  
**Detailed recommendation:** Linux: aggregate across all `BAT*` entries (pause if any is below threshold and discharging). macOS: parse more robustly or fall back to `sysinfo`.

---

## Priority 3 — Efficiency (high impact, low risk)

### [E1] Per-chunk embedding without batching at index time
**File:** `src/core/index/indexer.rs:287-293`  
**Impact:** High. Each chunk is embedded individually via `embedder.embed(&chunk.content)?` inside the SQLite transaction. With the Candle BERT model on CPU each forward pass is tens of ms; a document with thousands of chunks incurs linear per-chunk inference latency with no SIMD batching across chunks. This is the largest indexing-throughput win on CPU.  
**Detailed recommendation:** Accumulate chunk texts into a batch and call `embedder.embed_batch(texts)` once per N chunks (e.g. 32–64), writing each returned vector to its row. Candle's `BertModel::forward` accepts batched `input_ids` tensors; mean-pooling over the batch dimension gives one vector per chunk. Fall back to per-chunk embedding when batch size is 1. Insert the batch's vectors in a single transaction after all chunks for the file are inserted.

### [E2] sqlite-vec KNN fetch widened to `limit * 64` (min 4096), filtered in Rust
**File:** `src/core/search/tier1.rs:59-66`  
**Impact:** Medium-High on large indexes with filtered queries. With filters/roots, sqlite-vec KNN cannot push WHERE into the KNN clause, so it fetches `usize::max(limit * 64, 4096)` rows and deserializes each into `RawHit` (cloning path/snippet strings), most of which are discarded.  
**Detailed recommendation:** Fetch in smaller batches (e.g. `limit * 8`) and loop, re-issuing the KNN query until `limit` passing results are found or the table is exhausted. Remove the 4096 floor; for typical CLI limits (20) fetching 4096 vectors is excessive. Optionally push root/path filters into SQL on the joined `chunks` table where sqlite-vec permits.

### [E3] 50 individual SQLite hydration round-trips per search
**File:** `src/core/search/pipeline.rs:213-246`  
**Impact:** Small-Medium. `hydrate` prepares the statement once but runs a separate `query_row` per fused hit (50 hits = 50 round-trips).  
**Detailed recommendation:** Batch hydration with a single `WHERE id IN (...)` query or a temporary table + join, fetching all hit rows in one round-trip.

### [E4] Fresh `Connection` per search call re-registers sqlite-vec + runs init
**File:** `src/mcp/server.rs:26-38`, `src/cli/mod.rs:92-99`  
**Impact:** Medium (per-search latency). Every MCP tool call and CLI search opens a fresh `Connection`, calls `register_sqlite_vec()` (cheap `call_once` after first) and `init_db()` (runs `schema_sql` + `table_exists` + possibly `migrate`; on fresh files also `integrity_check` O(db size)).  
**Detailed recommendation:** Cache the `Connection` for the lifetime of the CLI invocation / MCP session, or at least skip `integrity_check`/`init_db` when already initialized. Keep per-request connections only where freshness is required (the comment notes agents see chunks indexed after server start — balance freshness vs latency with a short TTL cache).

### [E5] `prune_missing` stats every indexed file with `Path::exists()`
**File:** `src/core/index/indexer.rs:315-333`  
**Impact:** Small-Medium on large indexes. One `stat` per indexed document on every `prune_missing` call (which runs after every index).  
**Detailed recommendation:** Only `prune_missing` when deletions are known (track via the watcher), or run it less frequently (e.g. daily in the scheduler rather than every index).

### [E6] Entire file read into memory before extraction
**File:** `src/core/index/indexer.rs:196`, `src/core/extract/mod.rs:257`  
**Impact:** High on large files. `std::fs::read(path)` reads the whole file; PDFs allow up to 256 MB, general cap 64 MB per extractor, `MAX_EXTRACTED_BYTES` 32 MB per block. A 256 MB PDF plus parsed DOM/tree can exceed 1 GB RSS, risking OOM on laptops.  
**Detailed recommendation:** Stream large files where the extractor supports it (`Capabilities.streaming` exists but all extractors report `false`). For PDF use per-page `lopdf::Document::load_mem` on a mmap'd slice or page-by-page reading rather than loading the whole 256 MB. For spreadsheets/archives iterate members/rows without materializing full text. Add a global RSS ceiling (e.g. 1 GB) in the resource guard that pauses indexing when approached.

### [E7] `ModelManager::is_installed` re-hashes multi-GB files
**File:** `src/core/models/manager.rs:74-96`  
**Impact:** Small-Medium. Cached hits only (not misses). A miss re-hashes every call; `Workspace::embedder()` calls `is_installed` for two models on every search/pipeline build. The `path.exists()` short-circuit means missing files don't hash, but a present-but-wrong-hash file re-hashes on every search.  
**Detailed recommendation:** Cache misses too (negative cache with short TTL), or record the expected hash in `index_meta`/model metadata and compare file size+mtime before re-hashing.

### [E8] `extract_named` clones `bytes` for the parser thread
**File:** `src/core/extract/mod.rs:328`  
**Impact:** Medium on large files. `let bytes_owned = bytes.to_vec();` copies the entire file into the parser thread (256 MB PDF = 256 MB copy plus original).  
**Detailed recommendation:** Pass `Arc<[u8]>` into the thread; the extractor API takes `&[u8]` which `Arc<[u8]>` derefs to, avoiding the copy.

### [E9] Release profile could shrink further
**File:** `Cargo.toml:140-144`  
**Impact:** Low-Medium. `panic = "unwind"` keeps unwinding tables; `opt-level = 3` is larger than `"z"`.  
**Detailed recommendation:** Add `panic = "abort"` to `[profile.release]` (CLI; abort-on-panic acceptable given all errors are `anyhow::Result`). Experiment with `opt-level = "z"` measuring p95 search latency. Add `debug = false` and `rpath = false`. Cumulative savings: strip (~50–70%), LTO (~10–20%), codegen-units=1 (~5–10%), panic=abort (~5–10%).

### [E10] No dependency-trimming tooling in CI
**Impact:** Low. `cargo deny` is used for licenses/bans/sources, but `cargo-udeps`/`cargo-machete`/`cargo-shear` are not run to detect unused dependencies.  
**Detailed recommendation:** Add `cargo machete` and `cargo-udeps` to CI as advisory checks; remove unused direct dependencies; disable default features on large crates where unneeded (e.g. `serde = { default-features = false }` if only `derive` is used).

---

## Priority 4 — Accuracy

### [A1] Early exit on `kw_hits >= 10` hurts natural-language recall
**File:** `src/core/search/pipeline.rs:117`, `src/core/config.rs:69`  
**Impact:** High — the single largest relevance bug. With OR-combined terms a 3-word query can produce 10+ weak hits from one long document, suppressing semantic search that might find the truly relevant document in another file. The exit condition conflates "many keyword matches" with "sufficient evidence."  
**Detailed recommendation:** Exit keyword-only only when the top keyword hit has full term coverage AND confidence is High (`full_term_coverage` + score ≥ 0.9). Never skip semantic search purely on keyword count. Sweep `keyword_sufficiency_threshold` in {0, 5, 10, 20} against the eval harness and pick the value maximizing nDCG@10.

### [A2] No phrase/field query grammar in FTS5
**File:** `src/core/search/tier0.rs:52-68`  
**Impact:** Medium. FTS5 MATCH uses OR-combined, double-quoted terms with no positional/phrase support. Phrase queries ("fridge warranty") are treated as OR of individual terms, lowering precision for multi-word queries. Exact identifier search works by accident; fielded search ("author:smith") unsupported.  
**Detailed recommendation:** Add a minimal query grammar: quoted `"..."` spans → FTS5 phrase terms; `key:value` pairs → field filters (path, ext, date); bare terms → OR of single terms as today. Preserve operator-injection safety by doubling internal quotes (already done in `fts5_match_expr`).

### [A3] Chunking mid-clause splits citations
**File:** `src/core/index/indexer.rs:402-460`  
**Impact:** Medium. `chunk_text` splits on a 400-word boundary; the 40-word overlap mitigates but does not eliminate mid-thought cuts. A citation may quote half a warranty clause, making the excerpt misleading even though the line range is accurate.  
**Detailed recommendation:** Implement structure-aware chunking: split on markdown headings, PDF page boundaries, spreadsheet rows, and slide boundaries first, then cap each structural unit at 400 words with 40-word overlap. Never split mid-heading or mid-table-row. This preserves citation integrity and improves retrieval relevance (related statements stay together).

### [A4] CJK/Thai segmentation changes based on compile-time features
**File:** `src/core/text.rs:326-352`  
**Impact:** Medium. `fts_segment` dispatches on segmenter name and may route to ICU/Lindera/Jieba depending on compile-time features. If the index is built with one feature set and searched with another, token boundaries differ, silently breaking FTS5 matches for CJK/Thai and reducing recall.  
**Detailed recommendation:** Record the segmenter name in `index_meta` at index time and require the same at search time (analogous to the embedding-model check). Offer `icu_segmenter` and `lindera` as opt-in features; when enabled use the dictionary segmenter for both indexing and querying so tokens match.

### [A5] No citation-resolution test verifying line ranges contain the quoted snippet
**Impact:** Medium. A chunking bug could shift line ranges without failing any test.  
**Detailed recommendation:** Add a deterministic citation-accuracy test: for every hit in the golden set, re-read `path` lines `line_start..line_end` from the source file and assert the expected keyword/phrase is present. Run in CI (`cargo test`, offline). Fail CI if citation accuracy < 0.95.

### [A6] `Confidence::Low` hits excluded from answers but readable via MCP `read_source`
**File:** `src/cli/answer.rs:70-73`  
**Impact:** Low. Consistent with the "abstain" design; document it.  
**Detailed recommendation:** Keep as-is; document that `Confidence::Low` means "not sufficient evidence to answer" and is excluded from `Answer::from_hits`. Optionally surface confidence in search output.

### [A7] Korean segmentation weaker than Chinese/Japanese
**Impact:** Medium on Korean and other space-less scripts. Bigram+first-char for CJK/Thai/Lao/Khmer/Myanmar; Korean matches character pairs not words.  
**Detailed recommendation:** Document the Korean limitation prominently. Offer ICU segmenter (ICU4X word boundaries) and Lindera (Japanese IPADIC) as opt-in features; consider per-language segmenter selection based on detected script.

---

## Prioritized Action Plan

| # | Priority | Action | Effort | Impact | Files to change |
|---|----------|--------|--------|--------|-----------------|
| 1 | P0 | Archive member-name sanitization: reject `..`, absolute paths, drive letters, reserved names; log rejections | Small | High | `src/core/extract/archive.rs` |
| 2 | P0 | Daemon IPC peer-credential (UID) check on Unix socket; reject other UIDs | Medium | High | `src/daemon/ipc.rs` |
| 3 | P0 | OCR temp files via `tempfile::NamedTempFile` (random names); remove PID-based names | Small | High | `src/core/extract/ocr.rs` |
| 4 | P0 | Clamp MCP `read_source` context (max 5), require `source_id >= 1` | Small | Medium-High | `src/mcp/server.rs` |
| 5 | P0 | Document `unsafe` contracts; add `#![forbid(unsafe_code)]` with per-module allow | Small | Low-Medium | `src/lib.rs`, `src/core/index/connection.rs`, `embedder.rs`, `reranker.rs` |
| 6 | P1 | Batch chunk embeddings at index time (`embed_batch`, 32–64) | Medium | High | `src/core/index/indexer.rs`, `src/core/models/embedder.rs` |
| 7 | P1 | Fix early exit: skip semantic only on High keyword confidence + full term coverage | Small | High | `src/core/search/pipeline.rs`, `src/core/config.rs` |
| 8 | P1 | sqlite-vec KNN fetch in batches of `limit * 8`, remove 4096 floor | Small | Medium-High | `src/core/search/tier1.rs` |
| 9 | P1 | Batched hydration (`WHERE id IN (...)`) instead of N round-trips | Small | Medium | `src/core/search/pipeline.rs` |
| 10 | P1 | Stream large files / per-page PDF reading; add RSS ceiling to resource guard | Medium | High | `src/core/extract/{mod,pdf}.rs`, `src/daemon/resource_guard.rs` |
| 11 | P1 | Structure-aware chunking (headings/pages/rows/slides as hard boundaries) | Medium | High | `src/core/index/indexer.rs` |
| 12 | P1 | Stuck-parser reaper / bound total parser threads; per-file error on timeout | Medium | High | `src/core/extract/mod.rs` |
| 13 | P2 | Push `--after`/`--before`/`--ext` filters into FTS5/KNN SQL | Small | Medium | `src/core/search/{tier0,tier1}.rs`, `src/core/search/filters.rs` |
| 14 | P2 | Quoted-phrase + `key:value` field-filter query grammar | Medium | Medium | `src/core/search/tier0.rs`, `src/core/text.rs` |
| 15 | P2 | Citation-accuracy test (line ranges re-verified against source), CI gate | Small | High | `tests/`, `src/core/eval.rs` |
| 16 | P2 | Segmenter name recorded in `index_meta`, required at search time | Small | Medium | `src/core/index/indexer.rs`, `src/core/text.rs` |
| 17 | P3 | Binary size: `panic = "abort"`, try `opt-level = "z"`, add `cargo machete`/`udeps` to CI | Small | Medium | `Cargo.toml`, CI |
| 18 | P3 | `paper models migrate` to re-embed in place on model change | Medium | Low | `src/cli/models.rs`, `src/core/index/indexer.rs` |
| 19 | P3 | Int8 quantization reserve if exact KNN p95 > 100 ms at 10k chunks | Small | Low | `src/core/models/embedder.rs` |

---

## References

- CODE_REVIEW.md — paperscope code review, 44 findings (5 Critical, 14 Important, 24 Minor, 10 Efficiency). `paperscope/CODE_REVIEW.md`
- AGENTS.md — build/test invariants, MSRV 1.94, layout, exit codes. `paperscope/AGENTS.md`
- Cargo.toml — package version 0.2.2, edition 2021, features, release profile. `paperscope/Cargo.toml`
- OWASP Path Traversal Prevention Cheat Sheet — resolve-then-contain, symlink/TOCTOU, archive (Zip Slip) validation, Windows path forms. https://cheatsheetseries.owasp.org/cheatsheets/Path_Traversal_Prevention_Cheat_Sheet.html
- OWASP RAG Security Cheat Sheet — document poisoning, access-control inheritance, chunk isolation, index integrity, fail-closed design. https://cheatsheetseries.owasp.org/cheatsheets/RAG_Security_Cheat_Sheet.html
- OWASP Error Handling Cheat Sheet — generic user responses, details logged server-side. https://cheatsheetseries.owasp.org/cheatsheets/Error_Handling_Cheat_Sheet.html
- johnthagen/min-sized-rust — strip, opt-level="z"/"s", LTO, panic=abort, binary-size impact. https://github.com/johnthagen/min-sized-rust
- sqlite-vec — KNN `MATCH ... AND k = ?` syntax, cosine distance = 1 - dot. https://github.com/asg017/sqlite-vec
- Hugging Face Candle — batched `BertModel::forward`, mean pooling. https://github.com/huggingface/candle
- tempfile crate — `NamedTempFile` for random temp-file names. https://crates.io/crates/tempfile
- rusqlite `SO_PEERCRED` / `getpeereid` peer-credential checking for Unix sockets. https://man7.org/linux/man-pages/man7/unix.7.html


---
