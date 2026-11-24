# Paper

Local-first document search with exact citations. Single Rust binary: CLI + MCP
stdio server. No telemetry, no accounts, no cloud unless explicitly requested.

The crates.io package is `paperscope` (the name `paper` was taken); the library
target and the installed binary are both `paper`.

## Build & test

```bash
cargo check          # must be warning-free
cargo test           # offline, no model downloads (count: docs/implementation-status.md)
cargo build --release
cargo deny check licenses bans sources   # no copyleft in the shipped tree
cargo publish --dry-run                  # packaging must succeed warning-free
```

Enable the pre-commit lint once per clone (runs `cargo fmt --all --check` and
`cargo clippy --all-targets -- -D warnings` whenever a commit touches Rust
sources or manifests; bypass with `git commit --no-verify`):

```bash
./scripts/install-hooks.sh
```

`Cargo.toml` has an `include` allow-list because this repository holds unrelated
work. Its paths are rooted (`/Cargo.toml`, `/docs/...`): unanchored patterns
match inside `site/node_modules` and leak files into the crate.

Model integration tests download real weights and are therefore `#[ignore]`d:

```bash
cargo test --test models -- --ignored   # 134 MB / 487 MB / 1.13 GB
```

Always measure performance with `--release`; debug Candle is ~15x slower.

## Layout

- `src/core/paths.rs` — data locations. `PAPER_HOME` overrides everything (tests
  and sandboxes rely on this), `PAPER_OFFLINE=1` forbids network access.
- `src/core/config.rs` — `Config` (file) and `SearchConfig` (retrieval tuning).
- `src/core/index/` — `Connection` (registers sqlite-vec once per process),
  `schema` (chunks + FTS5 external content + `chunks_vec` `float[384]`),
  `indexer` (incremental, line-accurate chunks).
- `src/core/search/` — `tier0` (FTS5), `tier1` (sqlite-vec KNN), `pipeline`
  (RRF + confidence + hydration), `hit`.
- `src/core/models/` — `specs` (pinned repos, revisions, per-file SHA256),
  `manager` (download/verify/load), `embedder` (Candle BERT), `reranker`
  (Candle XLM-RoBERTa sequence classifier).
- `src/core/answer.rs` — extractive answers with citations.
- `src/cli/`, `src/mcp/` — thin front-ends over `core`.
- `tests/` — integration tests; `tests/common/mod.rs` has a deterministic
  offline `HashEmbedder` so retrieval logic never needs real weights.

## Invariants — do not break these

1. **Search never downloads a model.** Missing weights mean keyword-only search
   plus a hint, or exit code 3 when `--semantic` was requested.
2. **Citations must resolve.** Every hit carries `path`, `line_start`,
   `line_end`; chunking is responsible for keeping them accurate.
3. **Answers are extractive.** No generated prose without an explicit engine.
4. **Every model file is hash-pinned.** Never add an `UNPINNED` or placeholder
   digest, and never point a spec at `main` instead of a commit.
5. **Root allow-list.** MCP `read_source` takes a chunk id, never a path, and
   rejects anything outside the configured roots.
6. **Unsupported model architectures must fail loudly** rather than return
   plausible-looking vectors.
7. Exit codes: `0` found, `1` not found, `2` error, `3` model missing.

## Documentation

`docs/implementation-status.md` is the verified as-built description and the
source of truth. The original design documents (which describe superseded model
choices: EmbeddingGemma, ONNX) live on the `archive/design-docs` branch.
Update `implementation-status.md` whenever behaviour, models or test counts change.
