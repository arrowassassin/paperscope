# Paper

Local-first document search with exact citations. One Rust binary, two
front-ends: a CLI and an MCP stdio server. No telemetry, no account, no cloud.

```
$ paper search "when does the fridge warranty expire"
/Users/you/Documents/fridge-warranty.txt:1-4
  Samsung refrigerator warranty certificate ... Coverage ends on 2027-01-15.
  1.000 · matched by keyword search
```

Every hit carries `path` + `line_start` + `line_end`, so a citation always
resolves to the exact lines it quotes.

## Install

```bash
cargo install paperscope      # installs the `paper` binary
paper models install bge-small-en-v1.5   # optional: 134 MB, enables semantic search
paper index --root ~/Documents
```

Searching **never** downloads a model. Without weights you get keyword-only
search plus a hint; `--semantic` turns a missing model into a hard error
(exit code 3) instead of a silent download.

## Commands

```
paper index  --root <dir> [--embeddings] [--file <path>]
paper search <query> [--root <dir>] [--limit N] [--json] [--semantic]
paper answer <question> [--json]
paper models list|install <id>|remove <id>
paper serve  --mcp [--root <dir>]
paper doctor
```

Exit codes: `0` found, `1` not found, `2` error, `3` model missing.

| Variable | Effect |
| --- | --- |
| `PAPER_HOME` | Overrides the data directory entirely (tests and sandboxes). |
| `PAPER_OFFLINE=1` | Forbids all network access. |

## How it searches

1. **FTS5** keyword search, terms combined with `OR` and ranked by `bm25`
   (an `AND` of every word makes natural-language questions match nothing).
2. **Early exit** if keyword evidence is already sufficient — no model loads.
3. **sqlite-vec** cosine KNN, with a relevance floor so nonsense queries return
   nothing instead of the nearest neighbour.
4. **RRF** (k=60) fuses both lists.
5. **Optional cross-encoder** reranks the top candidates only.

## Models

All ungated (no Hugging Face account), first-party publishers, pinned by commit
and per-file SHA256.

| Model | Role | Size | Dims |
| --- | --- | --- | --- |
| `BAAI/bge-small-en-v1.5` | default embedding | 134 MB | 384 |
| `intfloat/multilingual-e5-small` | opt-in multilingual | 487 MB | 384 |
| `BAAI/bge-reranker-base` | opt-in cross-encoder | 1.13 GB | — |

Inference runs through Hugging Face Candle (pure Rust): no ONNX Runtime, no
Python, no native shared library.

## MCP

`paper serve --mcp --root <dir>` exposes four tools over stdio:
`search_documents`, `read_source`, `list_sources`, `answer_documents`.

`read_source` takes a chunk id, never a path, and rejects anything outside the
configured roots. Retrieved text is data, never instructions.

## Answers are extractive

An answer is a verbatim quote plus its citation — never generated prose — so it
cannot state a fact that is not on disk.

## Status

`0.1.0` indexes UTF-8 text formats (plain text, Markdown, source code). PDF,
DOCX, spreadsheet and email extraction is the next milestone; until then `page`
is always 0 and citations are `path:line-line`. The as-built description, model
details, measured performance and the full list of known gaps are in
[implementation-status.md](https://github.com/arrowassassin/paperscope/blob/main/docs/implementation-status.md).

## Licence

MIT OR Apache-2.0. Licence texts are in
[LICENSE-MIT](https://github.com/arrowassassin/paperscope/blob/main/docs/LICENSE-MIT)
and [LICENSE-APACHE](https://github.com/arrowassassin/paperscope/blob/main/docs/LICENSE-APACHE).
