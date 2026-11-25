# Paper — implementation status

Verified against the actual binary on 2026-11-19. This file describes what is
built and tested. The original design documents live on the `archive/design-docs`
branch and are **not** up to date with the model choices below.

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

Remote-config maintainer guide (keys, signing, CI): [remote-config.md](remote-config.md).

## Packaging and release (verified)

- `cargo publish --dry-run` succeeds: **72 files, ~155 KiB compressed**, and the
  packaged tarball compiles in `target/package/paperscope-0.1.0`.
- `rust-version = "1.94"` — the highest MSRV in the tree. `candle-core` 0.11.1
  calls `__cpuid_count` from safe code on x86_64, which needs 1.94; the Windows
  and Linux builds are therefore 1.94. (`notify-types` 2.3.1 needs 1.91,
  `zip` 9.8 and `aes` 0.9 1.89, the `icu_*` crates 1.88.)
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
  job + model prefetch). `scripts/regenerate-release-ci.sh` does both in one
  step.
- The generated jobs are `plan` → `build-local-artifacts` →
  `build-global-artifacts` → `host` → `sbom` → `announce`.
- Artifacts: per-target `tar.xz`/`zip`, source tarball, `sha256.sum`, shell
  and PowerShell installers, a Homebrew formula, an npm package, CycloneDX
  + SPDX SBOMs, and nine separate model files. `scripts/fetch-models.sh`
  downloads the pinned Hugging Face files, verifies their sizes and SHA256
  hashes, and stages unique `<model-id>-<file>` release asset names under
  `models/release/`; `scripts/prepare-release.sh` prints the local build and
  tag-driven release commands. Models are not bundled into platform archives.
  The nine unique assets are confirmed by `dist plan`; a real tag run is still
  pending.
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
| Storage | `rusqlite` 0.43 (`bundled`, SQLite 3.54.1) | no system SQLite dependency |
| Vectors | `sqlite-vec` 0.1.9, **statically linked** via `sqlite3_auto_extension` | `0.1.10-alpha.4` fails to build (missing `sqlite-vec-diskann.c`); static linking keeps the single-binary promise, no runtime `.dylib` |
| Inference | Hugging Face **Candle** 0.11 (`candle-transformers`), `tokenizers` 0.23 | pure Rust, no ONNX Runtime, no Python |
| MCP | `rmcp` 3.11 (`server`, `transport-io`) | official Rust MCP SDK, stdio only |

### Cargo features

| Feature | Default | Effect |
| --- | --- | --- |
| `embeddings` | on | Candle + tokenizers: semantic search and reranking. Off = keyword-only binary with no inference code (11.8 MB vs 15.1 MB). |
| `daemon` | on | Background indexing daemon, Unix-socket control CLI, and Linux/macOS user-service integration. |
| `jieba-rs` | off | Chinese word segmentation instead of the dictionary-free bigrams. |
| `shell-accel` | off | Prefer `pdftotext`/`pandoc` when present; pure Rust otherwise. |
| `ocr` | on | PP-OCRv4 via pure-Rust tract for scanned pages and images; weights auto-downloaded. |

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
| `BAAI/bge-small-en-v1.5` | optional English-only embedding | **134 MB** | 384 | BERT | semantic ordering correct |
| `intfloat/multilingual-e5-small` | default multilingual embedding (auto-downloaded) | **488 MB** | 384 | BERT | English query → French document retrieved |
| `BAAI/bge-reranker-base` | opt-in cross-encoder | **1129 MB** | — | XLM-RoBERTa seq-classification | relevant passage ranked first, score > 0.5 |

`paper models install` and `ModelManager` currently download these pinned
Hugging Face URLs directly; they do not yet construct GitHub Release asset
URLs. The release assets use `<model-id>-<file>` names to avoid basename
collisions.

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
   remaining hits keep their fused order. The reranker is skipped for queries
   containing non-ASCII letters (it is trained mainly on English).

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
paper daemon --foreground [--socket <path>] [--root <dir>]
paper daemon-control [--socket <path>] status [--json]|pause|resume|stop
paper daemon-control index [--root <dir>]...
paper daemon-control add-root <dir>|remove-root <dir>
paper daemon-control install [--force]|uninstall|start|restart
paper uninstall [--yes] [--keep-data] [--dry-run]
```

`daemon-control` uses a five-second IPC timeout and defaults to
`$PAPER_HOME/daemon.sock`, or the platform cache directory's `paper/daemon.sock`
when `PAPER_HOME` is unset. `install` creates a systemd user service on Linux or
a launchd user agent on macOS; Windows service installation and IPC are not yet
supported. Service definitions are replaced atomically, and replacing a
different existing definition requires `--force`.

`paper uninstall` removes the service (tolerating its absence), deletes the data and config directories Paper owns after a confirmation (`--yes` skips it, `--keep-data` omits it, `--dry-run` changes nothing), then deletes the binary for shell-installer installs or prints the brew, npm or cargo command to finish with.

Exit codes: `0` found, `1` not found, `2` error, `3` model missing.

Search **never downloads a model**. Without weights it runs keyword-only and
prints a hint. `--semantic` makes a missing model an explicit error (exit 3).
`PAPER_HOME` overrides the data directory; `PAPER_OFFLINE=1` forbids all network
access.

Human-readable `search` and `answer` output drops control characters (C0, DEL,
C1) except newline and tab from paths, anchors, sections, snippets and the
rendered answer, so an indexed document or file name cannot send escape
sequences to the terminal. `--json` output is not filtered: JSON escapes C0
controls, while DEL and C1 characters pass through as they are.

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

The protocol version is negotiated: a client that asks for `2024-11-05` gets
`2024-11-05`, a current client gets `2025-11-25` (the newest revision that
still has the `initialize` handshake the server implements). Both are covered
by the integration test, which speaks real JSON-RPC over stdio.

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
- The v3 → v4 migration drops the duplicate on the way in, comparing **tokens**
  rather than bytes — v3 stored the whitespace-normalised segmentation, so a
  byte comparison would have left most duplicates behind. On a synthetic
  5 000-row v3 index the `chunks` table shrank **1.21 MB → 0.74 MB (−39%)**,
  and the file is compacted (`VACUUM`) afterwards so the space is returned to
  the filesystem instead of merely becoming reusable.
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
- Unreadable paths are remembered in an additive `unreadable` table
  (`path`, `reason`, `first_seen`, `last_attempt`) and skipped for ten minutes
  before being retried, so one revoked grant cannot turn into a failed read and
  a warning per file on every five-minute pass. Denied directories are pruned
  from the walk and counted as deferred; a root with any denial reports once,
  with a platform-appropriate remedy. `NotFound` is deliberately *not* treated as
  a denial: a path that vanished is reported as a walk error, not as a missing
  grant.
- A row in `unreadable` means "denied, and not since proven readable". It is
  cleared by a successful read, not by a timer, so the backoff window (which only
  decides when to *retry*) can never expire the evidence a prune depends on. Rows
  for paths that no longer exist at all are dropped.
- `prune_missing` never deletes a path that is, or is under, a path recorded as
  unreadable. Without that rule a revoked grant answers `exists() == false` for
  everything beneath it, so the next pass deleted the content of a folder the
  user merely could not see — including prunes that never re-walked that root.

## Index and search behaviour (0.3.0)

- The daemon's passes over the configured roots also prune deleted files, and
  indexing jobs run one at a time (a mutex in the daemon state).
- A failed or timed-out extraction keeps the file's previous chunks; the file is
  counted as skipped with warnings and retried on the next pass.
- Recording a different embedding model than the one stored drops every chunk and
  vector, so the next pass re-embeds everything; vectors from two models never mix.
- `INDEXER_VERSION` is `extract4+chunk1` (no package version): a release does not
  wipe the index, only a change to extraction or chunking does.
- Chunks and embeddings for a file are computed before its write transaction.
- The MCP server loads the reranker once at start if installed (never downloads).
- A `setup-status` file in the data dir tells the CLI, `paper doctor` and MCP
  (`setup_in_progress`) that the daemon is still downloading or first-indexing;
  the reranker downloads in a background task and never delays the first index.
- `ModelManager::is_installed` (the gate for every model load) requires size, a
  matching SHA-256 and, to skip re-hashing, a `<file>.verified` stamp whose hash,
  size and mtime still match; otherwise it hashes once and re-stamps. `is_verified`
  and `paper doctor` always hash in full, as does every download.

## Tests

`cargo test` — 219 passing tests (plus 4 ignored: 3 model downloads and one
child-process helper for the setup-lock test), no network access, no model downloads.
Model integration tests are `#[ignore]`d and download real weights:

```
cargo test --test models -- --ignored   # 134 MB / 488 MB / 1.13 GB
```

Coverage: schema + triggers + WAL + KNN, schema migration, chunking and
line-number fidelity, incremental reindex, orphan-vector cleanup, pruning,
FTS5 injection, root allow-listing, RRF ordering, keyword early-exit,
model-free operation, CLI exit codes and JSON, MCP over real stdio JSON-RPC,
model pinning and checksum corruption, index storage (no duplicated text),
snippet fidelity, dense-script queries, the polish path (grounding,
citation validation, offline refusal, fallback), and daemon control (real
Unix-socket round-trips, human/JSON status, pause/resume, graceful shutdown,
missing-daemon diagnostics, and systemd/launchd rendering without installing a
real service).

## Measured performance (release build, M-series Mac, 3 small documents)

- Binary size: **15.1 MB** with the default `embeddings` feature, **11.8 MB**
  without it (`--no-default-features`, keyword-only, no inference code).
- Keyword search: **~0.5 s** including process start.
- Semantic search including a cold model load: **~0.47 s**.
- The same query in a debug build takes ~6.8 s, so always measure `--release`.
- Model download + hash verification + first embedding: ~16 s (134 MB).

## Known gaps (not yet built)

- **Windows Search / Spotlight as live Tier 0.** `spotlight_paths` exists but is
  not wired into the pipeline; FTS5 is the keyword engine on every platform.
- **Model/index mismatch guard — implemented.** The `index_meta` table now
  records the embedding model ID (`embedding_model` key) at index time.
  `SearchPipeline::search_with` compares the current embedder's `model_id()`
  against the stored value and returns a clear error on mismatch
  (`embedding model mismatch: index was built with 'X' but search is using 'Y'.
  Re-index or install the correct model.`). Both supported embedders (BGE and
  E5) and the deterministic test embedder implement `model_id()`.
- **Code signing and notarization.** The workflow maps the `CODESIGN_*`
  secrets, but no certificate is configured: artifacts are unsigned (and
  checksummed). No SLSA provenance, no `.deb`/`.rpm`/`.apk`, no Homebrew-tap or
  npm publish job.
- **Threat-model document.** The security properties are described in the
  README and in `docs/implementation-status.md`, but there is no dedicated
  threat model yet.
