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
  The twelve unique assets are confirmed by `dist plan`; a real tag run is still
  pending.
- **Not** built: `.deb`/`.rpm`/`.apk`, AppImage, MSI/pkg; no code signing or
  notarization (no certificate is configured, so artifacts are unsigned but
  checksummed); no SLSA provenance; no npm publish job (the tarball is attached to
  the release for manual publishing).
- Publishing: dist's `publish-homebrew-formula` job pushes `Formula/paper.rb`
  (`formula = "paper"`, `tap = "kintsugi-tools/homebrew-tap"`, secret
  `HOMEBREW_TAP_TOKEN`), and `publish-crate.yml` runs `cargo publish --locked`
  after approval in the `config-signing` environment (secret
  `CARGO_REGISTRY_TOKEN`). Auto-update only installs the version approved in
  the signed remote config's `[update]` table, so all methods roll out together.
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
| `BAAI/bge-reranker-base` | opt-in cross-encoder (English, Chinese) | **1129 MB** | — | XLM-RoBERTa seq-classification | relevant passage ranked first, score > 0.5 |
| `cross-encoder/mmarco-mMiniLMv2-L12-H384-v1` | multilingual cross-encoder (release-mirrored) | **488 MB** | — | XLM-RoBERTa seq-classification | pinned; inference verified by the evaluation gate below |
| `BAAI/bge-reranker-v2-m3` | large multilingual cross-encoder (Hugging Face only) | **2.29 GB** | — | XLM-RoBERTa seq-classification | pinned; inference verified by the evaluation gate below |

`paper models install` and `ModelManager` currently download these pinned
Hugging Face URLs directly; they do not yet construct GitHub Release asset
URLs. The release assets use `<model-id>-<file>` names to avoid basename
collisions.

### Reranker evaluation gate (spec section 9)

`cargo test --release --test rerank_eval -- --ignored --nocapture` with
`PAPER_HOME` set to a scratch dir (downloads about 4.4 GB). Embedder
`multilingual-e5-small`; corpora `tests/eval/golden.json` (English, answerable
queries only) and `tests/eval/golden-multilingual.json` (fr, de, es, hi, ja, ar:
7 documents and 4 queries each, 24 queries, each in the language of its one
relevant document). Every query is reranked, bypassing the language-coverage
skip. Release build, CPU. Latency is the median per-query search time with the
reranker minus the same search without it.

| Reranker | English MRR@10 | English hit@1 | Non-English MRR@10 | Non-English hit@1 | Median rerank ms/query (en, non-en) |
| --- | --- | --- | --- | --- | --- |
| none | 1.000 | 1.000 | 1.000 | 1.000 | - |
| `bge-reranker-base` | 1.000 | 1.000 | 0.894 | 0.833 | 3932, 3109 |
| `mmarco` | 0.978 | 0.957 | 0.944 | 0.917 | 1693, 1517 |
| `bge-reranker-v2-m3` | 1.000 | 1.000 | 0.979 | 0.958 | 11326, 9236 |

**Gate: FAIL.** `mmarco` English MRR@10 (0.978) is within 5% of `bge-reranker-base`
(1.000), but its non-English MRR@10 (0.944) does not beat the no-reranker
baseline (1.000). Caveat: the no-reranker baseline is at the ceiling on this small corpus
(retrieval alone already ranks every relevant document first), so the set cannot
show a reranker gain; a harder corpus would be needed to test that.

**Decision (maintainer, 0.4.0): `mmarco` is the default reranker anyway.** Head
to head it beats `bge-reranker-base` on non-English (0.944 vs 0.894), is about
twice as fast and less than half the download, and loses one English query in
23. `bge-reranker-base` is retired via the remote config (`[[deprecated]]`);
`v2-m3` stays opt-in.

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
- The daemon fetches the signed remote config (`src/daemon/remote.rs`) at start
  and every 24 hours unless `PAPER_OFFLINE` is set, records the outcome in
  `remote-config.state`, then follows the effective defaults: it downloads a new
  default reranker or embedder, re-embeds from stored text (resumable, status
  line "re-embedding your documents for the new model (N of M)"), and removes
  the old model only if it was Paper's own default, never a pinned one. A
  built-in effective default, which an expired config falls back to, never
  moves an index.
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

## Settings precedence

Local pin > remote default > built-in. `config.toml` accepts optional `embedder`
and `reranker` keys (a pin that remote defaults never move) and optional search
values (`keyword_sufficiency_threshold`, `rerank_top`, `rrf_k`,
`semantic_min_score`); only fields the user wrote are saved. `core::effective`
resolves the models and holds the built-in defaults; `Config::search(remote)`
resolves search values. `auto_update = false` in `config.toml`, or a non-empty
`PAPER_NO_AUTO_UPDATE` other than `0`, stops the daemon installing the version
the remote config's `[update]` approves (`core::update_state::auto_update_enabled`).
The remote config is read from the verified local cache
only, never fetched by the CLI or MCP, and an expired one contributes nothing.

## Auto-update

The daemon (`src/daemon/auto_update.rs`) installs the version named in the signed remote config's
`[update]` table, once a day after the config refresh, through the install method that owns the
binary (brew, npm, cargo, or the shell installer). It never installs a version that is not newer than
the running one, does nothing when `PAPER_OFFLINE=1`, `PAPER_NO_AUTO_UPDATE=1` or `auto_update = false`,
and records the result in `update.state`. A failure keeps the manual command and is not retried the
same day; an unpublished version or a missing Rust toolchain is "unavailable". cargo runs under
`nice` and only when the machine is idle. After a successful install the daemon restarts through its
service manager, and the next start confirms the running version (a mismatch is reported as failed
with a shadowing hint; a shell install restores `paper.old`).

## Tests

`cargo test` — 275 passing tests (plus 5 ignored: 3 model downloads, the reranker evaluation gate
and one child-process helper for the setup-lock test), no network access, no model downloads.
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
