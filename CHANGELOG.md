# Changelog

All notable changes to this project are recorded here. This project follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html); while the version is
`0.x`, minor releases may change behaviour.

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
