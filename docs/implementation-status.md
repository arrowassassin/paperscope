# Paper — implementation status

Verified against the actual binary on 2026-11-19. This file describes what is
built and tested; `paper-architecture.md` remains the original design spec and is
**not** up to date with the model choices below.

## Naming

The registry identity is **`paperscope`**; the product, the library target and
the installed binary are all **`paper`**. `cargo install paperscope` gives you a
`paper` command.

`paper` itself is taken on crates.io (`paper` v0.4.0, a terminal editor). Twelve
evocative one-word alternatives were free on crates.io but already the name of a
real product in this exact niche, which is why the chosen name is a compound:

| Candidate | Already used by |
| --- | --- |
| `fossick` | getfossick.com — offline private semantic document search for Mac/Windows |
| `shelfmark` | shelfmark.com (industrial AI), an iOS reading tracker, a self-hosted book search |
| `leafmark` | Leafmark EPUB reader (Windows/Android/iOS) |
| `deedbox` | getdeedbox.com — home records vault for deeds and warranties |
| `excerpt` | "Excerpt" browser extension — local search over clipped text |
| `citemark` | citemark.io / citemark.co.uk / getcitemark.com |
| `citetrail` | a GitHub project doing local provenance-backed recall for agents over MCP |
| `pressmark` | pressmark.dev — MCP document engine |
| `paperwork` | Paperwork (OpenPaperwork), a long-running document manager |
| `quotemark` | QuoteMark iOS quote scanner |
| `bookplate` | bookplate.dev, bookplate.eu, and several apps |

`paperscope` was verified absent from both crates.io and npm, with no competing
product found. Trademark and domain clearance have **not** been done.

Because this repository also holds unrelated work, `Cargo.toml` carries an
explicit `include` allow-list. The root `README.md` is not this project's file;
the crates.io page renders `docs/readme.md`. The allow-list paths are rooted
(`/Cargo.toml`, `/docs/...`) deliberately: an unanchored `CHANGELOG.md` pattern
matched `site/node_modules/**/CHANGELOG.md` and pulled three unrelated files into
the tarball.

## Packaging and release (verified)

- `cargo publish --dry-run` succeeds: **72 files, ~154 KiB compressed**, and the
  packaged tarball compiles in `target/package/paperscope-0.1.0`.
- `rust-version = "1.91"` — the highest MSRV in the tree (`notify-types` 2.3.1;
  `zip` 9.8 and `aes` 0.9 require 1.89, the `icu_*` crates require 1.88).
- `cargo deny check licenses bans sources` passes. `deny.toml` allow-lists
  permissive licences and denies wildcard requirements, unknown registries and
  unknown git sources. `MPL-2.0` is permitted only because it enters through
  `option-ext` ← `dirs` unmodified; there is no GPL/AGPL/LGPL in the tree.
- Licence texts ship as `docs/LICENSE-MIT` and `docs/LICENSE-APACHE`; the root
  `LICENSE-*` files belong to the unrelated project and are **not** packaged.
- `repository` and `documentation` metadata are set, so `cargo package` emits no
  metadata warning.

### Release pipeline (as built)

- `dist-workspace.toml` is the source of truth for the release;
  `.github/workflows/release.yml` is **generated** by `dist generate --mode=ci`
  and patched by `scripts/patch-release-ci.sh` (signing secrets + the SBOM
  job). `scripts/regenerate-release-ci.sh` does both in one step.
- The generated jobs are `plan` → `build-local-artifacts` →
  `build-global-artifacts` → `host` → `sbom` → `announce`.
- Artifacts: per-target `tar.xz`/`zip`, source tarball, `sha256.sum`, shell
  and PowerShell installers, a Homebrew formula, an npm package, and CycloneDX
  + SPDX SBOMs.
- **Not** built: `.deb`/`.rpm`/`.apk`, AppImage, MSI/pkg; no code signing or
  notarization (no certificate is configured, so artifacts are unsigned but
  checksummed); no SLSA provenance; no Homebrew-tap or npm publish job (the
  formula and tarball are attached to the release for manual publishing).
- The stale hand-written `packaging/` templates were removed: they were never
  built by CI, carried placeholder SHA256 values, declared a system SQLite
  dependency the binary does not have, and referenced a `paper completion`
  command that does not exist. `docs/PACKAGING.md` now documents what is
  really produced.

## Stack (as built)

| Area | Choice | Why |
| --- | --- | --- |
| Language | Rust, single crate, `src/lib.rs` + `src/main.rs` | one binary, integration tests import the lib |
| Storage | `rusqlite` 0.32 (`bundled`) | no system SQLite dependency |
| Vectors | `sqlite-vec` 0.1.9, **statically linked** via `sqlite3_auto_extension` | `0.1.10-alpha.4` fails to build (missing `sqlite-vec-diskann.c`); static linking keeps the single-binary promise, no runtime `.dylib` |
| Inference | Hugging Face **Candle** 0.8 (`candle-transformers`) | pure Rust, no ONNX Runtime, no Python |
| MCP | `rmcp` 0.2 (`server`, `transport-io`) | official Rust MCP SDK, stdio only |

### Cargo features

| Feature | Default | Effect |
| --- | --- | --- |
| `embeddings` | on | Candle + tokenizers: semantic search and reranking. Off = keyword-only binary with no inference code (10.9 MB vs 13.7 MB). |
| `jieba-rs` | off | Chinese word segmentation instead of the dictionary-free bigrams. |
| `shell-accel` | off | Prefer `pdftotext`/`pandoc` when present; pure Rust otherwise. |
| `ocr` | off | Tesseract OCR for scanned pages. |

`icu-segmenter` and `lindera` are **not** implemented — only `jieba-rs` exists.
The dictionary-free default (overlapping bigrams + first character) is the
right default for Chinese and Japanese; for Korean it is weaker, because
Hangul words are space-separated and bigrams match character pairs rather than
whole words. This limit is documented in the README rather than papered over.

## Models (as built)

All three are **ungated** (no Hugging Face account or token), published by the
**first-party model authors**, pinned by **commit** and by **SHA256 of every
file**, and verified by downloading the real weights and running inference.

| Model | Role | Size (measured) | Dims | Architecture | Verified |
| --- | --- | --- | --- | --- | --- |
| `BAAI/bge-small-en-v1.5` | default embedding | **134 MB** | 384 | BERT | semantic ordering correct |
| `intfloat/multilingual-e5-small` | opt-in multilingual embedding | **487 MB** | 384 | BERT | English query → French document retrieved |
| `BAAI/bge-reranker-base` | opt-in cross-encoder | **1129 MB** | — | XLM-RoBERTa seq-classification | relevant passage ranked first, score > 0.5 |

### Why not EmbeddingGemma / bge-reranker-v2-m3 (the original spec)

Measured, not assumed:

- `google/embeddinggemma` returns **HTTP 401** without authentication — it is a
  gated repository, which breaks the "no account" requirement.
- Ungated EmbeddingGemma safetensors mirrors are **1.21 GB**, and the model is
  Gemma3-based, which Candle's BERT/Gemma modules cannot load as an encoder.
- The "~200 MB EmbeddingGemma" figure only exists as **quantized ONNX**, which
  Candle cannot run.
- `bge-reranker-v2-m3` safetensors are **2.27 GB**, not 1.1 GB.

Both replacement embedders are 384-dim, so `chunks_vec` is `float[384]` and either
model can serve the same index schema.

## Retrieval pipeline (as built)

1. **Tier 0 — FTS5.** Terms are `OR`-combined and ranked with `bm25`. This is
   deliberate: `AND`-ing every word makes natural-language questions ("when does
   the fridge warranty expire?") match nothing. A small stopword list removes
   question words. Every term is double-quoted, so FTS5 operators cannot be
   injected. Queries under 3 characters are rejected.
2. **Early exit.** Enough keyword hits → no model is loaded at all.
3. **Tier 1 — sqlite-vec KNN** (`MATCH ... AND k = ?`), cosine distance, with a
   **relevance floor** (`semantic_min_score = 0.25`). Without a floor, vector
   search returns a nearest neighbour for *every* query, including nonsense.
4. **Tier 2 — RRF (k=60)** over both lists, computed in Rust.
5. **Tier 3 — optional cross-encoder**, applied to the top `rerank_top` (20) only;
   remaining hits keep their fused order.

`Hit::score` is the normalised relevance (cosine similarity / normalised bm25 /
reranker sigmoid), **not** the RRF score. RRF values (~0.016) are used for
ordering only; displaying them made every result report "Low" confidence.

## Commands

```
paper index  --root <dir> [--embeddings] [--file <path>] [--watch]
paper search <query> [--root <dir>] [--limit N] [--json] [--semantic]
paper answer <question> [--json] [--polish]
paper models list|install <id>|remove <id>
paper serve  --mcp [--root <dir>]
paper doctor
```

Exit codes: `0` found, `1` not found, `2` error, `3` model missing.

Search **never downloads a model**. Without weights it runs keyword-only and
prints a hint. `--semantic` makes a missing model an explicit error (exit 3).
`PAPER_HOME` overrides the data directory; `PAPER_OFFLINE=1` forbids all network
access.

## Answers

Answers are **extractive**: a verbatim quote from a local document plus exact citations. Without `--polish` nothing generative is involved.

`paper answer --polish` optionally rewrites the extract through an engine from
the config file:

```toml
[local_engine]
base_url = "http://localhost:11434"
model = "llama3.1"
```

`--engine openrouter` is the opt-in cloud fallback and needs an API key
(`OPENROUTER_API_KEY` or `[openrouter] api_key`). `PAPER_OFFLINE=1` refuses to
contact any engine.

Grounding rules, all enforced in code:

- The question and the extracts are wrapped in `<<<QUESTION>>>` / `<<<EXTRACTS>>>`
  blocks, and any `<<<`/`>>>` inside them is escaped, so a document cannot close
  the block and impersonate the instruction.
- The engine must cite every claim as `[doc:N]`.
- A reply citing an extract that does not exist, or citing nothing at all, is
  rejected: the verbatim extract is returned instead.
- The reply is size-capped (64 KiB), stripped of control characters, and the
  local citations stay attached to the answer.
- Any failure — transport, HTTP status, malformed reply — falls back to the
  verbatim extract with a note on stderr.

## MCP tools

`search_documents`, `read_source`, `list_sources`, `answer_documents`.
Connections are opened per request (a `rusqlite::Connection` is not `Sync`, and
fresh handles see newly indexed chunks). `read_source` takes a chunk id, never a
path, and rejects anything outside the server roots.

Answers are **extractive**: a verbatim quote plus `path:line_start-line_end`
citations. No generative model is bundled, so an answer cannot state a fact that
is not on disk.

## Index storage (schema v4)

- Whole-file dedup by BLAKE3 content hash: an unchanged file is skipped even when
  `mtime`/`size` change (`touch`, rsync, restores). Chunks are **not** deduplicated
  across documents — every document owns its rows, which keeps citations simple.
- `fts_text` / `fts_section` store only text that segmentation actually changed.
  Latin chunks store `''` and the FTS5 triggers index `content` / `section`
  through a `COALESCE` fallback, so the database no longer keeps a second copy of
  every Latin passage. Measured on `docs/` (142 chunks): `fts_text` payload
  98 KB → 1.8 KB, database **452 KiB → 356 KiB (−21%)**. The comparison is on
  whitespace-separated tokens because FTS5 tokenises newlines and spaces alike.
- Snippets are verbatim windows of the stored `content` around the first matching
  query term, never FTS5 `snippet()` output. No desegmentation pass exists, so
  CJK excerpts cannot be garbled, and a match deep inside a chunk stays visible.
- Citations carry `section` (heading path) alongside `path`, `page`,
  `line_start`/`line_end` and `anchor` — in CLI JSON, answers and `read_source`.
- Queries in scripts without spaces are searchable from one character; Latin
  queries still require three. (The old 3-character check rejected `検索` even
  though the segmenter already supported it.)
- `prune_missing` batches its deletes in one transaction: one fsync per prune,
  not one per deleted document.

## Tests

`cargo test` — 99 tests, no network access, no model downloads.
Model integration tests are `#[ignore]`d and download real weights:

```
cargo test --test models -- --ignored   # 134 MB / 487 MB / 1.13 GB
```

Coverage: schema + triggers + WAL + KNN, schema migration, chunking and
line-number fidelity, incremental reindex, orphan-vector cleanup, pruning,
FTS5 injection, root allow-listing, RRF ordering, keyword early-exit,
model-free operation, CLI exit codes and JSON, MCP over real stdio JSON-RPC,
model pinning and checksum corruption, index storage (no duplicated text),
snippet fidelity, dense-script queries, and the polish path (grounding,
citation validation, offline refusal, fallback).

## Measured performance (release build, M-series Mac, 3 small documents)

- Binary size: **13.7 MB** with the default `embeddings` feature, **10.9 MB**
  without it (`--no-default-features`, keyword-only, no inference code).
- Keyword search: **~0.5 s** including process start.
- Semantic search including a cold model load: **~0.47 s**.
- The same query in a debug build takes ~6.8 s, so always measure `--release`.
- Model download + hash verification + first embedding: ~16 s (134 MB).

## Known gaps (not yet built)

- **Windows Search / Spotlight as live Tier 0.** `spotlight_paths` exists but is
  not wired into the pipeline; FTS5 is the keyword engine on every platform.
- **Model/index mismatch guard.** Switching embedding models requires a reindex;
  the DB does not yet record which model produced its vectors.
- **Code signing and notarization.** The workflow maps the `CODESIGN_*`
  secrets, but no certificate is configured: artifacts are unsigned (and
  checksummed). No SLSA provenance, no `.deb`/`.rpm`/`.apk`, no Homebrew-tap or
  npm publish job.
- **Threat-model document.** The security properties are described in the
  README and in `docs/implementation-status.md`, but there is no dedicated
  threat model yet.
