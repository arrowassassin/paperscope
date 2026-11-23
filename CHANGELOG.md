# Changelog

All notable changes to this project are recorded here. This project follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html); while the version is
`0.x`, minor releases may change behaviour.

## [Unreleased]

### Fixed
- **A root with unreadable entries no longer re-reads and re-warns every one of
  them on every pass.** Denied directories are pruned from the walk and counted
  as deferred, denied files are remembered in a new additive `unreadable` table
  with a ten-minute backoff, and the root gets one actionable line ("grant access
  … Paper picks it up on the next pass") instead of one warning per file. That
  loop — a single revoked grant producing 1,070 failed reads per five minutes —
  is what made Paper look like it was asking for the same permission forever. A
  restored permission is picked up without a reinstall once the window elapses,
  and `paper index` reports the numbers (`N unreadable (permission denied), M
  deferred by the backoff`).
- **A revoked grant no longer deletes what it cannot see.** A path inside an
  unreadable directory answers "does not exist", so re-indexing used to delete its
  chunks and searches stopped matching. Content under a path recorded as
  unreadable is now never pruned — and the record no longer expires on a timer,
  so a prune that does not re-walk that root (`paper index --file …`, a different
  `--root`, `--watch`) cannot mistake a still-revoked grant for a deleted file.
  The mark is cleared when the path is read successfully, or when it is gone from
  the disk entirely.
- **`paper serve` never bootstraps, but it says so.** The MCP server is what an
  editor starts, often five sessions at once; it took the one-time setup lock,
  and each session tried to install the background service, so a project's first
  `initialize` could wait behind provisioning. It is now skip-listed — and when
  setup has not run yet it prints one line naming `paper index`, rather than
  leaving an editor-only install unprovisioned and silent.
- **The one-time setup lock is a kernel lock, not a file-existence test.** The
  old `create_new` file with a fifteen-minute "stale, take it over" heuristic was
  racy: two processes could both read the same old file, both decide it was
  stale, and both proceed. `File::try_lock` (`flock` / `LockFileEx`) is released
  by the OS when the holder dies, so the heuristic — and its dependence on an
  mtime that a clock change or a restore can move — is gone.
- **Error messages print context chains.** `{e}` → `{e:#}` at the user-facing
  log and print sites, so a failure says what was being attempted as well as what
  failed, instead of only the innermost message.

## [0.2.11] — 2026-11-22

### Changed
- The repository now lives at `github.com/kintsugi-tools/paperscope` (GitHub
  redirects every old URL, so nothing breaks). This release carries the move
  into the package index: crates.io metadata, the Claude plugin marketplace
  entry, the Homebrew tap path (`kintsugi-tools/tap/paper`), and the binary's
  built-in URLs — the `paper update` self-updater and the model-download
  release URLs — are org-owned. No feature changes.

## [0.2.10] — 2026-11-22

### Fixed
- `paper update` failed immediately with `write installer: No such file or
  directory`: it built a path inside a per-process temporary directory without
  creating that directory. The directory is now created before the installer is
  written and removed afterwards.
## [0.2.9] — 2026-11-22

### Fixed
- **The background service no longer runs a deleted binary.** On macOS,
  `launchctl bootstrap` refuses to replace an already-loaded job, so reinstalling
  after the install path changed (for example, removing a Homebrew copy) left
  launchd spawning the old executable forever. It exited `78: EX_CONFIG` and the
  daemon socket never appeared, which silently disabled automatic indexing.
  `daemon-control install` now boots the loaded job out before bootstrapping the
  new one.
- The release workflow locates `cargo-cyclonedx`'s output instead of assuming
  `target/sbom.json` (the tool names it after the package), and attaching SBOMs
  no longer fails the release when one format is missing.

### Changed
- Claude Code plugin is distributed through a repository marketplace
  (`/plugin marketplace add arrowassassin/paperscope`) and no longer bundles an
  executable, which the hosted plugin validator rejects.

## [0.2.8] — 2026-11-22

### Added
- `paper update` installs the newest release, and `paper update --check` reports
  whether one exists without changing anything. It runs the same installer a
  fresh install uses, so updates and first installs cannot drift apart.
- `paper doctor` and `paper update` warn when another `paper` earlier in `PATH`
  shadows the one you are running — the usual cause of "I upgraded but the
  version did not change".

### Changed
- Every subcommand now has a plain-language description and a runnable example
  in `paper --help` and `paper help <command>`.

## [0.2.7] — 2026-11-22

### Added
- **OCR for scans and images, on by default.** Scanned PDF pages and standalone
  PNG/JPEG/TIFF/WebP/BMP/GIF images are now searchable. Text found this way is
  cited as `page 12 (OCR)` or `image (OCR)` so a citation never pretends to come
  from a text layer.
- OCR runs on PaddleOCR **PP-OCRv4** models executed by pure-Rust `tract-onnx`.
  No Tesseract, no ONNX Runtime, no C/C++ toolchain, no system binaries — the
  static musl build stays static.
- The ~16 MB OCR weights download on first run with SHA-256 verification, like
  the embedding models. `--no-ocr` or `PAPER_NO_OCR=1` turns OCR off;
  `paper doctor` reports engine and model state.

### Changed
- Scanned PDFs are read by pulling the page bitmap out of the PDF object graph
  (`DCTDecode` JPEG or `FlateDecode` RGB/gray), so no rasteriser is required.
- A page counts as scanned when it has fewer than 32 non-whitespace characters.
- The per-file extraction deadline rises from 5 s to 180 s when OCR is enabled.
- `--ocr` now forces OCR; `--no-ocr` is new and skips it.

### Fixed
- `--ocr` was parsed but ignored: the extraction pipeline passed a hardcoded
  `false`, and no image format was registered. Both paths are now wired.

## [0.2.6] — 2026-11-22

### Changed
- First-run setup now downloads **both** default models in the background: the
  134 MB `bge-small-en-v1.5` embedder first, then the 1.13 GB
  `bge-reranker-base`. Ranking sharpens on its own once the reranker lands.
  `PAPER_NO_RERANKER=1` skips the second download (metered connections, small
  disks); searches keep working with dense + keyword results.
- README, crates.io README and the website document the full first-run model
  set instead of calling the reranker "optional".

## [0.2.5] — 2026-11-22

### Fixed
- `cargo install paperscope` no longer needs a C++ compiler: `tokenizers` is
  built without its `esaxx` suffix-array trainer (Paper only loads pretrained
  tokenizers). This also unblocks the static `x86_64-unknown-linux-musl` build.
- Static Linux builds point `CC`/`AR`/linker at `musl-gcc` (Ubuntu's
  `musl-tools` name) and define the BSD integer aliases `sqlite-vec` expects.

### Changed
- crates.io metadata now leads with AI/LLM discovery terms
  (`ai`, `llm`, `mcp`, `search`, `rag`) and the homepage points at
  <https://kintsugi.tools/paperscope.html>.
- README rewritten around a one-minute start, with an animated terminal demo,
  a lighter install section, and a separate crates.io landing page.

## [0.2.4] — 2026-11-22

### Added
- Zero-touch first-run bootstrap: the first `paper` command installs the
  background service (launchd / systemd), downloads the default embedding model,
  and indexes `~/Documents`, `~/Downloads`, `~/Desktop` immediately — no manual
  steps required. Escape hatches: `PAPER_NO_AUTOSTART=1`, `PAPER_OFFLINE=1`,
  `PAPER_HOME` (tests/portable), `CI=1`, `paper daemon-control uninstall`.
- Agent-facing indexing control via MCP: `index_documents` (recursive),
  `add_index_root` (persisted + daemon watcher), `remove_index_root`,
  `index_status` (files, chunks, model, roots, daemon reachability). Path
  vetting prevents prompt-injection escapes (absolute paths only, canonicalised,
  inside home/configured roots, sensitive dirs blocked: `.ssh`, `.aws`,
  `.gnupg`, `.kube`, `.password-store`, `Library`, `.Trash`).
- CLI persistence: `paper daemon-control add-root|remove-root` now writes
  `config.toml` so roots survive daemon restarts and reboots.
- Native pre-commit hook (`scripts/install-hooks.sh`) running `cargo fmt
  --all --check` + `cargo clippy --all-targets -- -D warnings` on Rust changes.

### Fixed
- `clippy::empty_line_after_doc_comments` lint in `accel.rs` (converted
  explanatory block to line comments).
- Borrow-after-move in OCR feature (`ocr.rs`).
- NOTICE file regenerated for updated dependency tree.

### Changed
- Daemon unions config roots with auto-discovery instead of replacing them.
- Immediate first index pass instead of waiting for the five-minute scheduler
  tick.
- Embedding model downloaded at daemon startup (background) so the first index
  pass already has vectors.

## [0.1.0] — 2026-11-21

First published release.

### Added

- `paper daemon` — background indexer with IPC (JSON-lines over Unix socket),
  daily 3 AM full + 5-minute incremental scheduling, file watching (250 ms
  debounce), and a resource guard (battery <20%, sustained CPU >80%).
  Auto-discovers `~/Documents`, `~/Downloads`, `~/Desktop`.
- `paper daemon-control` — `status`/`pause`/`resume`/`index`/`add-root`/
  `remove-root`/`stop` over IPC, plus `install`/`uninstall`/`start`/`restart`
  for the systemd user service (Linux) and launchd agent (macOS).
- `icu_segmenter` feature — ICU4X word segmentation for CJK, Korean, Thai,
  Lao, Khmer, Myanmar, Arabic, Hebrew, Devanagari and more.
- `lindera` feature — Japanese IPADIC segmentation (search mode) with the
  dictionary embedded in the crate: no network at build or run time.
- Model distribution through GitHub Releases: release CI fetches the pinned
  models (`scripts/fetch-models.sh`), verifies sizes/SHA256, and uploads nine
  `<model-id>-<file>` assets. `paper models install` downloads from the
  release first (resumable, SHA256-verified, atomic rename) with Hugging Face
  as fallback.
- `scripts/verify-release.sh` — every release gate in one command.
- `docs/CLAUDE_PLUGIN.md` — MCP setup for Claude Desktop, Claude Code,
  Codex CLI and OpenCode.
- Website: kintsugi.tools portfolio homepage, `paperscope.html` landing page,
  interactive `install.sh`/`pick-model.sh`, GitHub Pages deploy.
- End-to-end tests: full offline product flow, and MCP index freshness (a
  running server sees newly indexed documents without restart).

### Added (initial)

- `paper index` — incremental indexing of UTF-8 text files into one SQLite file,
  with line-accurate, overlapping chunks (`blake3` + mtime + size skip).
- `paper search` — four-tier retrieval: FTS5 keyword search (`bm25`, `OR`
  semantics), sqlite-vec cosine KNN with a relevance floor, reciprocal-rank
  fusion (k=60), and optional cross-encoder reranking.
- `paper answer` — extractive answers: a verbatim quote plus its citation, never
  generated prose.
- `paper models list|install|remove` — model download with pinned commit and
  per-file SHA256 verification.
- `paper serve --mcp` — MCP stdio server exposing `search_documents`,
  `read_source`, `list_sources` and `answer_documents` with a root allow-list.
- `paper doctor` — environment and index diagnostics.
- Exit codes: `0` found, `1` not found, `2` error, `3` model missing.
- `PAPER_HOME` (data directory override) and `PAPER_OFFLINE=1` (no network).

### Models

- `BAAI/bge-small-en-v1.5` — default embedding model, 134 MB, 384 dims, MIT.
- `intfloat/multilingual-e5-small` — opt-in multilingual embeddings, 487 MB,
  384 dims, MIT.
- `BAAI/bge-reranker-base` — opt-in cross-encoder reranker, 1.13 GB, MIT.

All three are ungated, pinned by commit and per-file SHA256, and run through
Hugging Face Candle (pure Rust). Searching never downloads a model.

### Known gaps

- Daemon IPC and service installation are Unix-only (Linux/macOS); Windows
  named-pipe IPC and Windows Service support are future work.
- Release artifacts are unsigned (no signing certificates available); every
  artifact carries a SHA256 checksum instead.
- `--json` and text output are not yet a stability guarantee.
