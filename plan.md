# Paper Release Candidate — Remaining Implementation Plan

## Context
Paper is a local-first document search CLI + MCP server (single Rust binary, `paperscope` crate, `paper` binary). Core search (FTS5 + sqlite-vec KNN + RRF + Candle embeddings) and extraction layer (PDF, DOCX, XLSX, PPTX, EPUB, email, HTML, Markdown, archives) are complete and tested (55 tests pass).

This plan covers the **17 remaining items** from the 24-item release-candidate action list.

## Global Constraints
- **Single binary**: no runtime deps, no Python, no ONNX, no shared libraries
- **Local-first**: `PAPER_OFFLINE=1` must work; no telemetry, no accounts, no cloud
- **Permissive licenses only**: MIT/Apache-2.0/BSD/ISC; `cargo deny check licenses bans sources` must pass
- **Exact citations**: every hit carries `path`, `line_start`, `line_end`, `anchor`, `section`
- **Extractive answers only**: no generated prose without explicit engine
- **Model pinning**: every model file hash-pinned (commit + per-file SHA256)
- **Root allow-list**: MCP `read_source` takes chunk id, rejects outside configured roots
- **Exit codes**: 0 found, 1 not found, 2 error, 3 model missing
- **Release profile only for perf**: debug Candle is ~15× slower
- **Warning-free**: `cargo check --all-targets` must be clean

## Tasks

### Phase 1: Foundation (7 tasks)

#### Task 1: Offline Evaluation Harness + Golden Set
**Spec**: `research_notes/Paper release candidate stack/retrieval.md` lines 103-109
**Files**: New `benches/eval.rs`, `tests/eval/`, `src/core/eval.rs`
**Requirements**:
- Deterministic `HashEmbedder` already exists in `tests/common/mod.rs`
- Golden set: 50 queries × 100 docs with relevance judgments (recall@k, MRR, nDCG@10)
- Citation accuracy: does cited line range actually contain the answer?
- Abstention correctness: does `Confidence::Low` correctly gate answers?
- Output: JSON report + CI gate (fail if recall@10 < 0.75 or citation accuracy < 0.95)
- Run with `cargo bench --bench eval` (release profile)

#### Task 2: cargo-dist + Signed Release Workflow
**Spec**: GitHub Actions + `cargo-dist` config
**Files**: `.github/release.yml`, `cargo-dist.toml`, `dist/`
**Requirements**:
- `cargo-dist` generates: macOS universal (notarized), Windows (Authenticode), Linux (musl + glibc)
- Signing: macOS `codesign --timestamp` + `notarytool`; Windows `signtool`; Linux `gpg --detach-sign`
- Artifacts: `.tar.gz`, `.zip`, `.deb`, `.rpm`, `.apk`, Homebrew formula, Scoop manifest
- SBOM: `cyclonedx` + `spdx` via `cargo-cyclonedx`
- SLSA Level 3 provenance via `slsa-github-generator`
- Publish to GitHub Releases on tag push (`v*`)

#### Task 3: CI Matrix + Audit/Deny/SBOM/SLSA
**Spec**: GitHub Actions matrix
**Files**: `.github/ci.yml`
**Requirements**:
- Matrix: `ubuntu-latest` × `macos-latest` × `windows-latest` × Rust `stable` `beta` `1.88` (MSRV)
- Steps: `cargo check --all-targets`, `cargo test`, `cargo deny check licenses bans sources`, `cargo audit`, `cargo cyclonedx`, `cargo sbom`
- Cache: `~/.cargo`, `target/`, `cargo-dist` artifacts
- Fail fast on any check

#### Task 4: NOTICE File (Licence Aggregation)
**Spec**: SPDX-compliant notice
**Files**: `NOTICE`, `scripts/generate-notice.sh`
**Requirements**:
- Auto-generated from `cargo metadata --format-version=1`
- Includes all transitive deps with license text
- `cargo deny check licenses` must pass with same allow-list as `deny.toml`
- Committed to repo, included in binary via `include_str!`

#### Task 5: Parser Capability Flags + Isolation + Fuzz Targets
**Spec**: Each extractor declares capabilities; fuzz via `cargo-fuzz`
**Files**: `src/core/extract/mod.rs` (add `Capabilities`), `fuzz/`, `Cargo.toml` (add `arbitrary`, `libfuzzer-sys`)
**Requirements**:
- `Extractor::capabilities()` → `Capabilities { max_file_bytes, max_pages, max_entries, streaming, thread_safe, .. }`
- Per-parser timeout (5s default) + memory cap (64 MB)
- Fuzz targets: `fuzz/pdf.rs`, `fuzz/docx.rs`, `fuzz/xlsx.rs`, `fuzz/pptx.rs`, `fuzz/epub.rs`, `fuzz/zip.rs`, `fuzz/tar.rs`
- Corpus: 1000+ malicious files per format (from `fuzz-corpus` or generated)
- CI runs `cargo fuzz run <target> -- -max_total_time=60`

#### Task 6: Migrations + busy_timeout + Integrity Check + Atomic Writes
**Spec**: SQLite durability
**Files**: `src/core/index/schema.rs`, `src/core/index/connection.rs`, `src/core/index/indexer.rs`
**Requirements**:
- `PRAGMA busy_timeout = 5000` (already in `Connection::finish`)
- `PRAGMA integrity_check` on `init_db` (first connection only)
- `PRAGMA quick_check` on every `Connection::open`
- Atomic writes: index to `chunks.db.tmp`, `fs::rename` on commit
- WAL checkpoint: `PRAGMA wal_autocheckpoint = 1000`
- `page_size = 8192` (set at creation, requires VACUUM to change)

#### Task 7: Coverage Gate
**Spec**: `cargo llvm-cov` threshold
**Files**: `.github/ci.yml`, `Cargo.toml` (dev-dep)
**Requirements**:
- `cargo llvm-cov --lcov --fail-under 80` in CI
- Exclude: `tests/`, `benches/`, generated code
- Report uploaded as artifact

---

### Phase 2: Retrieval Quality (2 tasks)

#### Task 8: Structure-Aware Chunking with Real Page
**Spec**: Use `Block::page` and `Block::anchor` to never split across pages/slides/sheets
**Files**: `src/core/index/indexer.rs` (`chunk_blocks`), `src/core/extract/mod.rs`
**Requirements**:
- Chunk boundary = page boundary (PDF), slide boundary (PPTX), sheet boundary (XLSX), spine boundary (EPUB)
- Overlap only *within* same page/slide/sheet; never across
- Citation always resolves to single page/slide/sheet
- Update `chunk_text` for plain text to respect `max_chunk_words` only

#### Task 9: Deterministic Field Extraction + SQL Aggregation
**Spec**: Structured fields (dates, amounts, kinds) as queryable SQL
**Files**: New `src/core/fields.rs`, `src/core/index/schema.rs` (add `metadata_json`), `src/core/search/pipeline.rs`
**Requirements**:
- Extract: ISO dates, currency amounts (symbol + code), doc kind (warranty/receipt/invoice)
- Store as JSON in `chunks.metadata` (already added in schema v2)
- CLI: `paper aggregate --field date --op min/max/count --filter "kind=warranty"`
- SQL: `SELECT json_extract(metadata, '$.dates'), json_extract(metadata, '$.amounts') FROM chunks WHERE ...`
- No inference: only verbatim patterns from the text

---

### Phase 3: Multilingual + Optional Accelerators (6 tasks)

#### Task 10: Dictionary Segmenters (Feature-Gated)
**Spec**: `icu_segmenter` (ICU), `lindera` (Japanese), `jieba-rs` (Chinese) behind features
**Files**: `Cargo.toml` (features), `src/core/text.rs` (pluggable segmenter), `src/core/search/mod.rs`
**Requirements**:
- Features: `icu-segmenter`, `lindera`, `jieba-rs` (all optional, default off)
- `Segmenter` trait with `segment(text) -> Vec<&str>`; default = bigram+first-char
- `fts_segment` and `query_terms` use configured segmenter
- Binary size impact documented in `docs/implementation-status.md`

#### Task 11: Shell-Out Accelerators (Feature-Gated)
**Spec**: `pdftotext`, `pandoc`, `libreoffice` as optional faster paths
**Files**: `Cargo.toml` (feature `shell-accel`), `src/core/extract/` (new `accel.rs`)
**Requirements**:
- Detect binary in `PATH` at runtime; fall back to pure Rust
- Timeout: 30s per file; kill on timeout
- Log which extractor was used (pure-Rust vs shell-out)
- No network access; binaries must be pre-installed by user

#### Task 12: Gated OCR (Feature-Gated)
**Spec**: Tesseract, macOS Vision, Windows OCR as opt-in
**Files**: `Cargo.toml` (feature `ocr`), `src/core/extract/ocr.rs`
**Requirements**:
- Feature `ocr` pulls `tesseract` (system dep) + `vision` (macOS) + `windows-ocr` (Windows)
- Only runs when: PDF page has no text layer AND `paper index --ocr` flag
- Output: new blocks with `anchor = "page N (OCR)"`, `section = "OCR"`
- Warning logged for each OCR page

#### Task 13: Packaging Scaffolds
**Spec**: Homebrew, Scoop, .deb, .rpm, AppImage
**Files**: `packaging/homebrew/paper.rb`, `packaging/scoop/paper.json`, `packaging/deb/`, `packaging/rpm/`, `packaging/appimage/`
**Requirements**:
- Homebrew: formula with `url` = GitHub Release asset, `sha256` auto
- Scoop: manifest with `url` + `hash`, `architecture: 64bit`
- .deb: `debhelper`, `control` with `Depends: libc6`, `Section: utils`
- .rpm: `spec` file with `%files`, `Requires: glibc`
- AppImage: `appimagetool`, `AppRun` wrapper, desktop entry
- All generated by `cargo-dist` where possible

#### Task 14: cargo-vet / Sandbox Audit Config
**Spec**: `cargo-vet` certifications + `cargo-audit` suppressions
**Files**: `vet/config.toml`, `vet/imports/`, `audit.toml`
**Requirements**:
- `cargo vet certify` for all deps in `Cargo.lock` (use `vet/imports` from `mozilla`, `google`, `rust-secure-code`)
- `audit.toml` for false positives (e.g. `zip` CVE in transitive dep not used)
- CI runs `cargo vet check` + `cargo audit`

#### Task 15: File Watching (Already Implemented)
**Status**: ✅ Done in `src/cli/index.rs` (watch mode with debounced fsnotify)

---

### Phase 4: Optional Generative Polish (2 tasks)

#### Task 16: OpenAI-Compatible Local Server Client
**Spec**: Ollama / LM Studio / vLLM local server
**Files**: `src/core/models/local_client.rs`, `src/core/answer.rs` (polish path), `src/cli/answer.rs`
**Requirements**:
- Config: `local_engine = { type = "openai-compat", base_url = "http://localhost:11434", model = "llama3.1" }`
- `paper answer --polish` sends extract + question to local server
- Streaming response; citations preserved from extract
- Falls back to verbatim extract on any error
- No binary dependency on Python or specific server

#### Task 17: OpenRouter Opt-In + Grounding + Injection Defense
**Spec**: Cloud fallback with strict grounding
**Files**: `src/core/models/openrouter.rs`, `src/core/answer.rs`, `src/cli/config.rs`
**Requirements**:
- Config: `openrouter = { api_key = "sk-...", model = "anthropic/claude-3.5-sonnet" }` (opt-in only)
- `paper answer --polish --engine openrouter`
- Prompt: "Answer ONLY from the provided extracts. Cite every claim with [doc:N]. If extracts don't contain the answer, say 'Not found in local documents.'"
- Injection defense: user question wrapped in `<<<QUESTION>>>`, extracts in `<<<EXTRACTS>>>`; any `<<<` or `>>>` in input escaped
- Response validated: every claim has citation, no hallucinated citations

---

## Success Criteria
All 17 tasks complete → `cargo check --all-targets` clean, `cargo test` 55+ pass, `cargo deny` clean, `cargo bench` runs, `cargo publish --dry-run` clean, signed release artifacts exist for all platforms.