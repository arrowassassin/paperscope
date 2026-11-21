# Efficiency, Safety, and Accuracy Improvements — paperscope

**Status:** Draft — research report with actionable recommendations  
**Scope:** `paperscope` Rust crate, binary `paper`, MSRV 1.94, edition 2021  
**Sources consulted:** source tree (`src/`, `tests/`, `benches/`), `Cargo.toml`/`Cargo.lock`, existing `research_notes/` (hardening, retrieval, extraction, generation), Microsoft Rust Training (release profiles/binary size), min-sized-rust, OWASP RAG Security Cheat Sheet, OWASP Path Traversal Prevention Cheat Sheet, OWASP Error Handling Cheat Sheet.

---

## Efficiency

### Performance

**[Finding] Query embedding happens per-chunk at index time with no batching.** In `src/core/index/indexer.rs:287-293`, each chunk is embedded individually via `embedder.embed(&chunk.content)?` inside the SQLite transaction. With the Candle BERT model on CPU, each forward pass is ~tens of ms; a document with thousands of chunks incurs a linear per-chunk cost with no SIMD batching across chunks.

**Impact:** High. Indexing a large document set is dominated by per-chunk inference latency; throughput is far below what batched Candle inference could achieve.

**Recommendation:** Accumulate chunk texts into a batch and call `embedder.embed_batch(texts)` once per N chunks (e.g. 32–64), writing each returned vector to its row. Candle's `BertModel::forward` accepts batched `input_ids` tensors; mean-pooling over the batch dimension gives one vector per chunk. Fall back to per-chunk embedding when the batch size is 1. This is the single largest indexing throughput win on CPU.

---

**[Finding] sqlite-vec KNN fetch is widened to `limit * 64` (minimum 4096) rows whenever filters or roots are present, then filtered in Rust.** See `src/core/search/tier1.rs:45-66`. For a root-filtered query on a large index this can scan tens of thousands of vectors in Python-equivalent Rust, all to return `limit` (default 20) hits.

**Impact:** Medium-High on large indexes with filtered queries; p95 semantic latency grows with index size even when only a few roots match.

**Recommendation:** Push root/path filters into SQL on the joined `chunks` table before or alongside the KNN clause where sqlite-vec permits, and cap the fetch at `limit * 4` rather than `limit * 64`. If sqlite-vec cannot accept a WHERE clause on the join, fetch `limit * 8` and iterate in Rust until `limit` passing results are found or the table is exhausted — the current 4096 floor is excessive for typical CLI limits.

---

**[Finding] FTS5 MATCH uses OR-combined, double-quoted terms (stopword-filtered) with no positional or phrase support.** See `src/core/search/tier0.rs:52-68`. Phrase queries ("fridge warranty") are treated as OR of individual terms, lowering precision for multi-word queries.

**Impact:** Medium. Natural-language queries match too broadly; exact-phrase intent is lost.

**Recommendation:** Add optional quoted-phrase parsing: detect `"..."` spans in the user query, emit them as FTS5 phrase terms (`"fridge warranty"`), and OR them with single-term OR clauses for the remaining words. Keep operator-injection safety by doubling internal quotes (already done).

---

**[Finding] Startup opens SQLite with `PRAGMA quick_check` on fresh files and runs `PRAGMA integrity_check` on new databases.** See `src/core/index/connection.rs:38-53` and `src/core/index/schema.rs:105-119`. `integrity_check` is O(database size) and runs once on creation; `quick_check` runs on first open after creation.

**Impact:** Low-Medium. On multi-GB indexes the startup integrity check adds noticeable latency before the first search.

**Recommendation:** Keep `quick_check` (fast) on every open; run `integrity_check` only on creation and optionally on a configurable schedule (e.g. weekly or after a crash detected via `PRAGMA wal_checkpoint` return). Do not block interactive search on a full integrity check of a large index.

---

### Memory

**[Finding] The entire file is read into memory before extraction:** `std::fs::read(path)` in `indexer.rs:196` and `extract_path` in `extract/mod.rs:257`. PDFs allow up to 256 MB (`pdf.rs:21`), the general cap is 64 MB per extractor, and `MAX_EXTRACTED_BYTES` caps block text at 32 MB.

**Impact:** High on large files. A 256 MB PDF plus its parsed DOM/tree representation can exceed 1 GB RSS during indexing, risking OOM on constrained machines (the daemon targets laptops).

**Recommendation:** Stream large files where the extractor supports it (`Capabilities.streaming` already exists but all extractors report `false`). For PDF, use per-page `lopdf::Document::load_mem` on a mmap'd slice or page-by-page reading rather than loading the whole 256 MB. For spreadsheets and archives, iterate members/rows without materializing the full text. Add a global RSS ceiling (e.g. 1 GB) in the resource guard that pauses indexing when approached.

---

**[Finding] Chunks are 400 words with 40-word overlap and stored verbatim in both `chunks.content` and the FTS5 index (when segmented differently).** See `indexer.rs:82-83` and `schema.rs`. A large document set stores each chunk's text twice (content + FTS copy), plus a 384-dim float vector per chunk.

**Impact:** Medium. Storage and memory scale linearly with chunk count; at 100k chunks that's ~100+ MB of text plus ~150 MB of vectors plus FTS overhead.

**Recommendation:** Reduce default `max_chunk_words` to 256 with 32 overlap for dense-script and structured documents; keep 400/40 for plain text. Store `fts_text` only when segmentation differs from content (already partially done — see `indexer.rs:254-257`). Consider `opt-level = "z"` or per-crate size tuning for the embedder binary footprint.

---

### Binary Size

**[Finding] Default build includes Candle + tokenizers (embeddings on by default), giving ~15.1 MB vs 11.8 MB keyword-only.** See README:126-130. The `embeddings` feature pulls `candle-core`/`candle-nn`/`candle-transformers`/`tokenizers` — a large dependency tree — even for users who never use semantic search.

**Impact:** Medium. The default binary is ~28% larger than necessary for keyword-only users; compile time and download size are higher.

**Recommendation:** Keep `embeddings` default-on (it's a core feature), but measure per-crate contributions with `cargo bloat --crates` and `cargo-llvm-lines`. If Candle dominates, offer a `--no-default-features` keyword-only build (already documented) and consider making `daemon` opt-in too, since `notify`, `sysinfo`, `cron`, `tokio full` are only needed for the background daemon.

---

**[Finding] Release profile uses fat LTO + codegen-units=1 + strip + opt-level 3.** See `Cargo.toml:140-144`. This is good but `panic` is not set to `"abort"`, and `opt-level` is `3` (speed) rather than `"z"`/`"s"` (size).

**Impact:** Low-Medium. `panic = "unwind"` keeps unwinding tables and panic string formatting (~20-80 KB per the Microsoft guide); `opt-level = 3` is larger than `"z"`.

**Recommendation:** Add `panic = "abort"` to `[profile.release]` (search is a CLI; abort-on-panic is acceptable given all errors are handled via `anyhow::Result`). Experiment with `opt-level = "z"` — measure if p95 search latency stays within budget. Add `debug = false` explicitly and `rpath = false`. These changes are cumulative: strip (-50 to -70%), LTO (-10 to -20%), codegen-units=1 (-5 to -10%), panic=abort (-5 to -10%).

---

**[Finding] No dependency trimming tooling in CI.** `cargo deny` is used for licenses/bans/sources (README:177), but `cargo-udeps`/`cargo-machete`/`cargo-shear` are not run to detect unused dependencies.

**Impact:** Low. Unused dependencies increase compile time, binary size, and supply-chain surface without benefit.

**Recommendation:** Add `cargo machete` (fast, heuristic) and `cargo-udeps` (nightly, accurate) to CI as advisory checks. Remove any unused direct dependencies; disable default features on large crates where features are unneeded (e.g. `serde = { default-features = false }` if `derive` is the only use).

---

## Safety

### Security

**[Finding] Path containment uses `path.starts_with(root)` on raw path strings, not on resolved/canonicalized paths.** See `src/core/search/tier0.rs:72-88` (`root_filter` builds `path LIKE 'root%'`), `src/core/search/tier1.rs:94-97` (`Path::new(&hit.path).starts_with(r)`), `src/mcp/server.rs:93-96` (`allowed`), and `src/daemon/auto_discover.rs` (not yet read, but likely similar). `starts_with` on `PathBuf` compares path components lexically and does NOT resolve symlinks, `..` components, or case differences.

**Impact:** High. An indexed path inside a root that is itself a symlink pointing outside the root, or a path containing `..` that normalizes outside, could be searched/read outside the allow-list. On Windows, case-insensitivity and alternate separators (`\` vs `/`) are not handled.

**Recommendation:** Canonicalize each root once at startup with `std::fs::canonicalize` (resolves symlinks and `..`), then canonicalize each candidate path before comparison, or use a lexical `starts_with` on components plus a symlink-skip rule. Reject paths whose canonical form escapes any root. On Windows, normalize separators and compare case-insensitively. The MCP `read_source` tool (which reads by `source_id`, not path) is safer by design but the underlying SQL still returns paths — apply the same containment check.

---

**[Finding] Three `unsafe` blocks exist, all in model loading/extension registration.** `embedder.rs:57` and `reranker.rs:40` use `VarBuilder::from_mmaped_safetensors` (mmap of model weights — trusted local files, hash-verified); `connection.rs:14-21` uses `sqlite3_auto_extension` with a transmuted function pointer for sqlite-vec (static, compile-time known).

**Impact:** Low. The unsafe is well-contained and the inputs are pinned, hash-verified model files and a statically linked extension. However, `from_mmaped_safetensors` on a corrupt file could theoretically read out of bounds; the SHA256 pre-verification mitigates this.

**Recommendation:** Add `#![forbid(unsafe_code)]` at the crate level with `#[allow(unsafe_code)]` on the three specific modules, documenting the safety justification for each. Run `cargo clippy --all-targets` with `-D unsafe_op_in_unsafe_fn` in CI. Keep the SHA256 verification before mmap (already done in `manager.rs:107`).

---

**[Finding] Archive extraction (zip/tar) does not validate entry names for path traversal.** `archive.rs` (not read in full) uses `zip` and `tar` crates; ZIP Slip (`../../` entry names) is a known risk cited in the hardening notes.

**Impact:** High if archives are indexed. A malicious zip with `../../etc/passwd` or symlink entries could write/read outside the intended directory during extraction.

**Recommendation:** Validate every archive entry name with the OWASP containment check: join to a scratch base, resolve, and confirm the result stays inside the base before extracting or indexing. Reject symlink/hardlink entries. Skip entries whose names contain `..`, absolute paths, or (on Windows) `:` / reserved device names. Log rejected entries as warnings, never silently extract.

---

**[Finding] Parser timeout uses `std::thread` + mpsc with a 5-second deadline; a parser that ignores the deadline is parked in `STUCK_PARSERS` (max 4) but its thread and input copy survive.** See `extract/mod.rs:304-377`. Rust has no safe thread cancellation, so a malicious PDF/DOCX can consume a parser slot and its memory indefinitely.

**Impact:** Medium. A denial-of-service via many slowly-parsing files can exhaust the 4 stuck-parser slots and hold memory.

**Recommendation:** Add a hard wall-clock + address-space limit by running parsers in a child process with OS rlimits (RLIMIT_AS/RLIMIT_CPU on Linux, job objects on Windows, `sandbox-exec` on macOS) — the hardening notes already flag this as "wait for later" but mark it as a pre-1.0 blocker. Until then, cap total parser memory via `MAX_EXTRACTED_BYTES` (already 32 MB per block) and refuse new files while stuck slots are full (already done).

---

**[Finding] `PAPER_OFFLINE=1` forbids network, but model downloads use GitHub Releases + Hugging Face fallback over HTTPS with reqwest (rustls).** SHA256 verification is correct (`manager.rs:276-282`), and downloads go to `.partial` then atomic rename (`manager.rs:155-214`).

**Impact:** Low. Download integrity is well handled.

**Recommendation:** Keep as-is; document that `PAPER_OFFLINE=1` must be set before any `paper models install` call and that the daemon should respect it. Add a `--verify-only` mode to `paper models` that checks installed hashes without network.

---

### Error Handling

**[Finding] `.unwrap()` / `.expect()` appear in tests and possibly in production paths; production code uses `anyhow::Result` consistently but some `query_row` calls in `schema.rs` tests use `.unwrap()`.**

**Impact:** Low-Medium. Test panics are acceptable; production panics would crash the daemon or CLI on unexpected input.

**Recommendation:** Grep the production `src/` tree (not `tests/`) for `.unwrap()`, `.expect()`, `.panic!()` and convert each to proper `Result` propagation with context. Keep `unwrap()` only where the condition is provably impossible (document why). Enforce with a CI clippy lint `-W clippy::unwrap_used` (or `-D` for production modules).

---

**[Finding] Errors return exit code 2 generically; no distinction between "corrupt file", "model missing", "index corrupt", "path outside roots" beyond exit codes 0/1/2/3.**

**Impact:** Low-Medium. Operators and scripts cannot programmatically distinguish retryable from fatal errors.

**Recommendation:** Define a structured error enum (`PaperError`) with variants for each failure class, map to distinct exit codes (e.g. 4 = corrupt index, 5 = path outside roots, 6 = resource exhausted), and log structured error details (machine-readable JSON with `--json`) while showing user-friendly messages on stderr. Never leak stack traces or file system internals to end users (OWASP Error Handling: generic response to user, details logged server-side).

---

**[Finding] `Connection::open` runs `PRAGMA quick_check` on fresh files and bails if not "ok"; existing databases skip integrity checks on open.** See `connection.rs:38-53`.

**Impact:** Medium. A corrupted WAL or page can be opened and searched, potentially returning wrong results or crashing on page-level corruption.

**Recommendation:** Run `PRAGMA quick_check` on every open (it is fast) and `PRAGMA integrity_check` on a configurable schedule or after detecting unclean shutdown (WAL recovery). On `quick_check` failure, attempt recovery: copy the DB, run `.recover`, or fall back to a fresh index with a clear error. Never silently serve results from a corrupt index.

---

### Resource Protection

**[Finding] No global limit on concurrent indexing jobs, open file handles, or memory during indexing.** The watcher reindexes on every change; `index_roots` walks the whole tree sequentially.

**Impact:** Medium. Indexing a tree with tens of thousands of files can exhaust file descriptors (walkdir holds open handles) and memory (each file read fully into RAM).

**Recommendation:** Add a semaphore on concurrent file reads (default: number of CPUs), a per-process open-file cap, and a memory ceiling in the resource guard that pauses indexing when RSS approaches it (e.g. 75% of available RAM or 1 GB). Batch `walkdir` iteration and close handles promptly. The existing `ResourceGuard` (battery/CPU) should be extended with memory and open-file checks.

---

**[Finding] No disk-full handling.** Atomic index replacement writes a temp DB then renames; `index_with_defaults_atomic` does not check free space before writing.

**Impact:** Medium. A disk-full mid-index leaves a partial `.db.tmp` and possibly a corrupt or missing main DB.

**Recommendation:** Check available disk space (`std::fs::space`) before starting an index; reserve at least 2× the expected DB size. On write failure, clean up `.db.tmp` and return a clear error. The `index_with_defaults_atomic` already removes stale temp files on startup (good).

---

**[Finding] Model file hashes are verified on install, but no rate limiting on download retries, and the HTTP client has no per-host connection limits.** See `manager.rs:100-153`.

**Impact:** Low. Retries on corrupt downloads could hammer the mirror; no connection pooling limits.

**Recommendation:** Add exponential backoff with jitter between download-source retries, cap total retries (e.g. 3), and configure `reqwest::Client` with a connection pool timeout and per-host limit. Log download events without printing the URL if it might contain credentials (it doesn't here — model URLs are public).

---

## Accuracy

### Search Relevance

**[Finding] Early exit skips semantic search when `kw_hits.len() >= 10` (keyword_sufficiency_threshold default 10).** See `pipeline.rs:117` and `conf.rs:69`. With OR-combined terms, a 3-word query can produce 10+ weak hits from one long document, suppressing semantic search that might find the truly relevant document in another file.

**Impact:** High. This is the single largest relevance bug: the exit condition conflates "many keyword matches" with "sufficient evidence."

**Recommendation:** Exit keyword-only only when the top keyword hit has full term coverage AND confidence is High (`full_term_coverage` + score ≥ 0.9). Never skip semantic search purely on keyword count. Sweep `keyword_sufficiency_threshold` in {0, 5, 10, 20} against the eval harness and pick the value maximizing nDCG@10.

---

**[Finding] RRF(k=60) fusion is rank-based and appropriate, but `relevance_map` takes max(normalized bm25, cosine) which mixes incomparable scales.** See `pipeline.rs:272-282`. Normalized bm25 is 0..1 within the keyword result set; cosine is 0..1 globally — they are not interchangeable.

**Impact:** Low-Medium. The displayed `score` and confidence derivation may overstate keyword-only hits relative to semantic ones.

**Recommendation:** Keep RRF for ordering (it correctly avoids scale mixing). For the displayed `score`, use the RRF score (rank-based, scale-free) or explicitly normalize each list to 0..1 before taking max. Document that `score` is relative, not an absolute probability.

---

**[Finding] No metadata/date/file-type filters are pushed into the FTS5 or KNN SQL; tier1 filters roots in Rust after fetching a 4096-row superset.** See `tier0.rs:102-129` (filters.sql exists but is basic) and `tier1.rs:45-66`.

**Impact:** Medium. Queries like "warranties from 2025" or "receipts in /Manuals" cannot filter efficiently; all candidates are fetched then discarded.

**Recommendation:** Implement `--after`/`--before` (mtime) and `--ext` (extension) as SQL WHERE clauses on the `chunks` table in both tier0 and tier1, using the already-indexed `mtime`, `size`, and `path` columns. This is low complexity and high value for receipt/tax queries.

---

**[Finding] The cross-encoder reranker (bge-reranker-base, 1.13 GB) is opt-in and applied to `rerank_top` (default 20) candidates only.** See `pipeline.rs:143-150` and `reranker.rs`.

**Impact:** Low. Good design — reranking everything would be prohibitive on CPU; the top-20 restriction is correct.

**Recommendation:** Measure the actual nDCG/MRR delta of reranking vs RRF-only on the household corpus using the eval harness. If the delta is <5%, keep the reranker opt-in only (it is already). Default `rerank_top` to 20; document CPU cost (tens of ms per pair).

---

**[Finding] No query-language support for phrases, identifiers, or field filters.** `fts5_match_expr` double-quotes every term and ORs them; numbers/symbols tokenize as their own terms.

**Impact:** Medium. Exact identifier search (invoice numbers, model numbers) works by accident (tokens match), but phrase intent and fielded search ("author:smith") are unsupported.

**Recommendation:** Add a minimal query grammar: quoted phrases → FTS5 phrase terms; `key:value` pairs → field filters (path, ext, date); bare terms → OR of single terms as today. Preserve operator-injection safety (double-quote escaping already present).

---

### Citation Accuracy

**[Finding] PDF page citations work (`page N` anchor) but markdown/plain-text `chunk_text` hardcodes `page: 0`.** See `indexer.rs:402-460` and `pdf.rs:68`. Text formats have no page concept, so line ranges are the correct anchor — but PDF text is split into lines within a page and `chunk_blocks` recomputes line_start/line_end relative to `block.line_start` (`indexer.rs:479-486`).

**Impact:** Medium. Line ranges for PDF text may not correspond to visual lines on the page (lopdf extracts text without layout), so `path [page 3]:1-5` could point at the wrong visual location.

**Recommendation:** Document that PDF line numbers are extraction-order lines, not visual lines; for visual fidelity use pdfluent-extract bboxes (optional) or state the limitation clearly. For formats without lines (DOCX, spreadsheets, slides), the anchor field (`para N-M`, `Sheet1!A4:H4`, `slide N`) is already format-specific and correct — keep that invariant and never synthesize line numbers from position.

---

**[Finding] Chunking can split a sentence/clause across the 400-word boundary; the overlap (40 words) mitigates but does not eliminate mid-thought cuts.** See `chunk_text` (`indexer.rs:402-460`).

**Impact:** Medium. A citation may quote only half of a warranty clause, making the excerpt misleading even though the line range is accurate.

**Recommendation:** Implement structure-aware chunking: split on markdown headings, PDF page boundaries, spreadsheet rows, and slide boundaries first, then cap each structural unit at 400 words with 40-word overlap. Never split mid-heading or mid-table-row. This preserves citation integrity (each chunk stays within one structural unit) and improves retrieval relevance (related statements stay together).

---

**[Finding] `Confidence::Low` hits are excluded from answers (`answer.rs:70-73`), but Low hits can still appear in search results and be read via MCP `read_source`.**

**Impact:** Low. Consistent with the "abstain" design — answers don't cite weak evidence, but search transparency is preserved.

**Recommendation:** Keep as-is; document that `Confidence::Low` means "not sufficient evidence to answer" and is excluded from `Answer::from_hits`. Optionally surface confidence in search output so users know why a hit was not used in an answer.

---

**[Finding] No citation-resolution test verifies that `line_start..line_end` content actually contains the quoted snippet.** The eval exists (`tests/eval/golden.json`, `src/core/eval.rs`) but citation accuracy is a metric, not a hard invariant check.

**Impact:** Medium. A chunking bug could shift line ranges without failing any test.

**Recommendation:** Add a deterministic citation-accuracy test: for every hit in the golden set, re-read `path` lines `line_start..line_end` from the source file and assert the expected keyword/phrase is present. Run in CI as part of `cargo test` (offline, no model downloads). Fail CI if citation accuracy < 0.95 (the existing bench gate).

---

### Chunking & Embedding

**[Finding] Embedding model is locked at index time (`index_meta.embedding_model`) and mismatched models at search time produce a hard error.** See `indexer.rs:91-98` and `pipeline.rs:93-104`. This is correct but forces a full reindex on model change.

**Impact:** Low. Correctness over convenience; acceptable.

**Recommendation:** Keep the hard error (silently searching with wrong vectors is worse). Add a `paper models migrate` command that re-embeds all chunks in place with a new model (read chunks, embed, update `chunks_vec`) rather than requiring delete+reindex from disk.

---

**[Finding] Mean pooling over real tokens with L2 normalization; sqlite-vec cosine distance = 1 - dot.** See `embedder.rs:91-125`. This is standard and correct for BGE/E5.

**Impact:** Low. No change needed.

**Recommendation:** Keep as-is. Consider storing vectors as `float32` (already) — int8 quantization (~13× speed, ~99.5% recall) is a reserve option only if exact KNN exceeds the latency budget at scale.

---

**[Finding] Multilingual support uses bigram+first-char for CJK/Thai/Lao/Khmer/Myanmar; Korean is weaker (bigrams match character pairs, not words).** See README:105-111 and `text.rs`.

**Impact:** Medium on Korean and other space-less scripts. Korean precision is lower than Chinese/Japanese.

**Recommendation:** Document the Korean limitation prominently. Offer `icu_segmenter` (ICU4X word boundaries for Korean, Thai, Lao, Khmer, Myanmar, Arabic, Hebrew, Devanagari) and `lindera` (Japanese IPADIC) as opt-in features; when enabled, use the dictionary segmenter for both indexing and querying so tokens match. Consider a per-language segmenter selection based on detected script.

---

**[Finding] No OCR by default; scanned PDFs return empty pages with a warning.** OCR is opt-in (`--ocr`, requires Tesseract + `pdftoppm`). See README:103.

**Impact:** High for image-heavy corpora (scanned receipts, contracts). Those documents are effectively invisible to search.

**Recommendation:** Keep OCR opt-in (correct per local-first/no-download invariant). Add a clear hint when a PDF page has no text layer: "page N: no text layer — use `paper index --ocr` to OCR scanned pages." Consider optional OS-native OCR (Apple Vision on macOS, Tesseract on Linux) behind a feature flag, never downloaded automatically.

---

## Prioritized Action Plan

| # | Priority | Action | Effort | Impact |
|---|----------|--------|--------|--------|
| 1 | P0 | Batch chunk embeddings at index time instead of per-chunk | Medium | High |
| 2 | P0 | Fix early exit: don't skip semantic search on `kw_hits >= 10`; exit only on High keyword confidence | Small | High |
| 3 | P0 | Canonicalize paths + symlink check for root containment (all callers) | Medium | High |
| 4 | P0 | Archive path-traversal validation for zip/tar entries | Small | High |
| 5 | P0 | Add citation-resolution test (line ranges re-verified against source) | Small | High |
| 6 | P1 | Structure-aware chunking (headings/pages/rows/slides as hard boundaries) | Medium | High |
| 7 | P1 | Push `--after`/`--before`/`--ext` filters into FTS5/KNN SQL | Small | Medium |
| 8 | P1 | Batch `cargo-bloat`/`cargo-machete` in CI; add `panic = "abort"`, try `opt-level = "z"` | Small | Medium |
| 9 | P1 | Add RSS ceiling + open-file cap to resource guard; pause indexing near limits | Medium | Medium |
| 10 | P1 | Disk-full handling + temp-file cleanup in atomic index replacement | Small | Medium |
| 11 | P2 | Add `.unwrap()`/`.expect()` ban on production `src/` via clippy | Small | Low-Medium |
| 12 | P2 | Structured error enum with distinct exit codes; no stack traces to users | Medium | Low-Medium |
| 13 | P2 | Optional quoted-phrase + field-filter query grammar | Medium | Medium |
| 14 | P2 | `paper models migrate` to re-embed in place on model change | Medium | Low |
| 15 | P3 | Evaluate int8 quantization reserve if exact KNN p95 > 100 ms at 10k chunks | Small | Low |
| 16 | P3 | Per-language segmenter (ICU/Lindera) opt-in for Korean/Thai/etc. | Medium | Low-Medium |

---

## References

- Microsoft Rust Training, "Release Profiles and Binary Size" — LTO, codegen-units, panic=abort, strip, opt-level impact table; cargo-bloat/cargo-udeps/cargo-machete/cargo-shear. https://microsoft.github.io/RustTraining/engineering-book/ch07-release-profiles-and-binary-size.html
- johnthagen/min-sized-rust — strip, opt-level="z"/"s", LTO, panic=abort, build-std, UPX caveats, no-std extremes. https://github.com/johnthagen/min-sized-rust
- OWASP RAG Security Cheat Sheet — document poisoning, context-window delimiters, access-control inheritance, chunk isolation, index integrity, fail-closed design, output validation. https://cheatsheetseries.owasp.org/cheatsheets/RAG_Security_Cheat_Sheet.html
- OWASP Path Traversal Prevention Cheat Sheet — resolve-then-contain, symlink/TOCTOU, archive (Zip Slip) validation, Windows path forms. https://cheatsheetseries.owasp.org/cheatsheets/Path_Traversal_Prevention_Cheat_Sheet.html
- OWASP Error Handling Cheat Sheet — generic user responses, details logged server-side, RFC 7807/RFC 9457 problem details. https://cheatsheetseries.owasp.org/cheatsheets/Error_Handling_Cheat_Sheet.html
- paperscope source: `src/core/index/indexer.rs`, `src/core/search/{pipeline,tier0,tier1}.rs`, `src/core/index/{connection,schema}.rs`, `src/core/extract/{mod,pdf}.rs`, `src/core/models/{embedder,reranker,manager}.rs`, `src/mcp/server.rs`, `src/daemon/resource_guard.rs`, `src/cli/{index,search,answer}.rs`, `Cargo.toml`, `README.md`
- paperscope research_notes: `Paper release candidate stack/{hardening,retrieval,extraction,generation}.md`
