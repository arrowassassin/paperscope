# Paper — Architecture & Model Manager Spec

**One line:** a local-first document finder that uses the OS's own index first, tiny embeddings only for the gaps, answers with exact citations, borrows the OS brain for polish, and exposes itself to coding agents over MCP + CLI.

---

## 1. Design principles

1. **No mandatory download over ~100 MB.** The app must be useful within 60 seconds of install on a normal laptop.
2. **OS index first, our index second.** macOS Spotlight already indexed the user's files. Rebuilding it is waste.
3. **Retrieval before generation.** The answer is a *found passage with a citation*; prose is optional polish.
4. **Every answer cites file + page/line.** No citation, no answer.
5. **Nothing leaves the machine without an explicit, previewed action.**
6. **Agents are first-class users.** CLI + MCP server ship in v0.1, not v2.

---

## 2. The four-tier retrieval pipeline

| Tier | Component | Cost / size | When it runs | Latency target |
|---|---|---|---|---|
| **0** | Spotlight (`mdfind`) on macOS; SQLite FTS5 on Win/Linux | 0 MB (OS) / ~0 | Every keyword, identifier, exact-phrase query | 20–100 ms warm |
| **1** | Tiny embedding model + local vector search | 8–90 MB | Queries that are paraphrases / conceptual | 5–20 ms for <50k chunks |
| **2** | Reciprocal Rank Fusion (RRF, k=60) | 0 MB | Always, merges Tier 0 + Tier 1 | <1 ms |
| **3** | Reranker (optional) | 0 MB default; ~1 GB if enabled | Only when precision matters; user-enabled | 50–300 ms |
| **4** | Polish (OS built-in model) | 0 MB | Only when a natural sentence is wanted | 0.5–3 s |

**Why this order:** Spotlight handles exact strings and identifiers for free and instantly. Embeddings only fill the semantic gap. RRF fuses without score calibration. Reranking is an opt-in precision upgrade, not a default cost. Generation is last and optional.

**Evidence:**
- `mdfind` can search content via `kMDItemTextContent` and is faster than `NSMetadataQuery` for wildcards ([Eclectic Light](https://eclecticlight.co/2025/09/04/quirks-of-spotlight-local-search/); [Apple](https://developer.apple.com/library/archive/documentation/Carbon/Conceptual/SpotlightQuery/Concepts/QueryingMetadata.html)).
- Live Text/OCR content is only searchable via `mdfind`, not `NSMetadataQuery` ([Eclectic Light](https://eclecticlight.co/2025/08/13/how-to-search-spotlight-for-live-text-and-objects-in-images/)).
- Hybrid (BM25 + dense) + rerank cut top-20 retrieval failure from 5.7% → 1.9%; without rerank it was 49% ([Anthropic Contextual Retrieval, via wz-it](https://wz-it.com/en/knowledge/ki/hybrid-search-reranking/)).
- RRF uses rank positions, needs no score normalization; k=60 is the stable default ([Cormack et al.; Qdrant](https://qdrant.tech/documentation/tutorials-basics/reranking-hybrid-search/)).
- Hybrid search is cheap: ~14 ms p50 warm on 1M docs ([turbopuffer](https://turbopuffer.com/docs/hybrid)).

---

## 3. Tier 0 — Spotlight integration (macOS)

**Use `mdfind` as the fast path.** Call it as a child process; do not embed `NSMetadataQuery` unless live updates are needed.

```bash
# Content search, scoped, case/diacritic-insensitive
mdfind -onlyin "$HOME/Documents" 'kMDItemTextContent == "*warranty*"cd'

# Any metadata or content (catches Live Text/OCR), scoped
mdfind -onlyin "$HOME/Documents" '(** == "warranty*"cdw)'

# Exact identifier (part numbers, error codes)
mdfind -onlyin "$HOME/Documents" 'kMDItemTextContent == "TS-999"cd'
```

**Rules learned from the research:**
- **Minimum 3 characters.** 1–2 char queries return 100k–300k results and take >5 s; 5+ chars drops to milliseconds ([StackOverflow](https://stackoverflow.com/questions/68622160/how-to-limit-the-number-of-results-returned-from-nsmetadataquery)).
- **Cap results** and stop reading after N hits; never materialize the full result set.
- **Use the display-name key**, not the filesystem-name key — the latter forces a filesystem check and is slow (same source).
- **Handle disappearing files.** Spotlight results are "a changing projection of metadata services," not a guarantee ([danielcosenza.com](https://danielcosenza.com/posts/mac-spotlight-metadata-query-lifecycle/)).
- **Spotlight is not complete.** It skips some volumes, system dirs, and file types. Always fall back to a parser when a file matters.
- **Private/encrypted folders:** expect TCC prompts; degrade gracefully.

**Cross-platform parity:** on Windows use the Windows Search index where available, else SQLite FTS5. On Linux use FTS5 over watched folders. The interface is the same: `search_keyword(query, roots) -> hits`.

---

## 4. Tier 1 — the embedding layer

### Model choice

| Model | Size | Quality note | Use |
|---|---|---|---|
| `potion-mxbai-micro` | **0.7 MB** | Surprising for the size; weakest | Extreme-low-footprint fallback |
| `potion-mxbai-256d-v2` | **7.5 MB** | Good retrieval-per-MB | Default on weak machines |
| `ogma-micro` | **8.9 MB** | MTEB 52.18, 128-d | Tiny default alternative |
| `all-MiniLM-L6-v2` | **87 MB** | The proven workhorse | Balanced default |
| `EmbeddingGemma` (quantized) | **~200 MB** | Best-in-class under 500M, 100+ languages | "Best" tier |

Sources: [potion/model2vec family](https://www.reddit.com/r/LocalLLaMA/comments/1sapdue/700kb_embedding_model_that_actually_works_built_a/); [ogma-micro](https://huggingface.co/axiotic/ogma-micro); [EmbeddingGemma](https://developers.googleblog.com/en/introducing-embeddinggemma/).

**Default:** `all-MiniLM-L6-v2` (87 MB) on machines with ≥8 GB RAM; `potion-mxbai-256d-v2` (7.5 MB) below that. Both are permissively licensed.

### Indexing rules

- **Incremental only.** Key each chunk by `(path, mtime, size, content-hash)`. Skip unchanged files.
- **Heading-aware chunks**, ~300–500 tokens, 10–15% overlap. Store `page` and `line_start` with every chunk so citations are exact.
- **Cache the index** and invalidate only when the corpus size changes; repeated rebuilds were a measured cost in a comparable system ([FoundryRAG](https://github.com/MEK-0/Local-Foundry-assistant)).
- **Brute-force cosine is fine below ~50k chunks.** No ANN library needed; simpler and smaller.
- **Extract text, don't embed binaries.** Skip images/video; use OCR output where present.
- **Cap file size** (e.g. 20 MB) and log skipped files.

**Storage:** one SQLite file. Tables: `files`, `chunks`, `vectors` (BLOB), `meta`. FTS5 virtual table for keyword search on non-macOS.

---

## 5. Tier 2–3 — fusion and reranking

**RRF:**
```
score(d) = Σ_r 1 / (60 + rank_r(d))
```
Merge Tier 0 and Tier 1 lists. Keep top 20 candidates.

**Reranker:** off by default (footprint). Offer `bge-reranker-v2-m3` (Apache-2.0, ~1.1 GB, CPU-capable) as an opt-in "precision" download. Never make it mandatory.

**Abstention:** if the best fused score is below a calibrated threshold, answer **"I couldn't find that in your documents"** and show the closest passages. The threshold must be tuned on questions the corpus deliberately cannot answer ([wz-it](https://wz-it.com/en/knowledge/ki/hybrid-search-reranking/)).

---

## 6. Model manager (the part users actually see)

### Availability probe (run at startup, cache result)

```
detect:
  macOS 26+ & Apple Intelligence on  -> "apple-fm"      (0 MB, free, on-device)
  Windows Copilot+ / Aion Instruct   -> "windows-aion"  (0 MB, preinstalled)
  Chrome 148+                        -> "gemini-nano"   (0 MB, browser)
  Ollama / LM Studio running         -> "byo-local"
  OpenRouter key present             -> "openrouter"    (opt-in, previewed)
  none                               -> "extractive"    (still fully functional)
```

**Critically: `extractive` is a complete product.** The app never blocks on a model.

Sources for built-ins: [Apple Foundation Models](https://developer.apple.com/wwdc26/guides/apple-intelligence/); [Windows Aion Instruct](https://devblogs.microsoft.com/foundry-on-windows/aion-instruct-local-ai-windows-gpu/); [Chrome Gemini Nano](https://flutteredge.ai/docs/builtin-ai).

### Download flow

Three buttons, each showing the real size:

| Button | What it installs | Size |
|---|---|---|
| **Fast** | 7.5 MB embedding model | ~8 MB |
| **Balanced** (default) | 87 MB embedding model | ~90 MB |
| **Best** | ~200 MB EmbeddingGemma | ~200 MB |

- Lazy, resumable, checksummed.
- **Never bundle weights in the installer.**
- No generative model is downloaded unless the user explicitly asks for summarization *and* no OS brain exists.

### Router

| Task | Engine |
|---|---|
| Find / exact / identifier | Spotlight (Tier 0) |
| Semantic find | embedding (Tier 1) |
| Rank | RRF (+ optional reranker) |
| Natural sentence | OS brain, else extractive |
| "Summarize this whole contract" | OS brain; else offer ~1 GB upgrade or OpenRouter |
| Classification / extraction | smallest available model |

### Privacy preview

Any time a cloud engine (OpenRouter) is selected, show **exactly** the prompt and passages that would be sent, with a one-click cancel. Log every egress to a visible `egress.log`.

---

## 7. Answer format

Every result is a card:

```
[Answer or extractive sentence]

Source: warranty-fridge.pdf · page 2 · lines 14–16
"Why this matched": "…guarantee period of 24 months from…"
Confidence: high | medium | low
Engine: local (Tier 0+1) | Apple FM | OpenRouter
```

- **Extractive default.** The highlighted passage *is* the answer.
- **Polish optional.** The OS brain rewrites it into a sentence; the citation never changes.
- **"I don't know"** is a first-class result.

---

## 8. Agent hooks (CLI + MCP)

### CLI (scripts, CI, power users)

```bash
paper search "fridge warranty" --json          # ranked hits with citations
paper get <source-id> --json                   # full text + metadata
paper index ~/Documents --watch                # (re)build index
paper answer "when does the warranty end?"     # extractive + optional polish
paper serve --mcp                              # stdio MCP server
paper doctor                                   # probe engines, index health
```

All commands support `--json` and meaningful exit codes (`0` found, `1` not found, `2` error).

### MCP server (stdio)

**Narrow by design: 3–4 tools** ([Workato best practices](https://docs.workato.com/en/mcp/mcp-server-design.html); [MCP best practices](https://modelcontextprotocol.info/docs/best-practices/)).

| Tool | Input | Output |
|---|---|---|
| `search_documents` | `query`, `roots?`, `limit?` | ranked passages with `source_id`, `path`, `page`, `line`, `snippet`, `score` |
| `read_source` | `source_id` | full text + metadata (validated against an allowlist) |
| `list_sources` | `roots?` | indexed roots and counts |
| `answer_documents` | `question`, `roots?`, `polish?` | extractive answer + citations |

Design rules:
- **Typed schemas**, documented errors, deterministic behavior.
- **Stateless tools**; index state lives in SQLite.
- **Root allowlist** — refuse anything outside configured roots (`roots/list` aware).
- **Treat document text as data, not instructions.** Flag/neutralize prompt-injection patterns in retrieved content.
- stdio for local single-user; if HTTP, bind `127.0.0.1` and require a token ([MCP security](https://modelcontextprotocol.info/docs/best-practices/)).

**Install:**
```bash
claude mcp add paper --scope user -- paper serve --mcp --root ~/Documents
```

Works with Claude Code, Cursor, Windsurf, Codex, Zed, VS Code (any MCP host).

### Claude Code hook recipes

**1. Index changed files after edits** (`PostToolUse`):
```json
{
  "hooks": {
    "PostToolUse": [{
      "matcher": "Write|Edit",
      "hooks": [{
        "type": "command",
        "command": "paper index --file \"$CLAUDE_FILE_PATH\" --quiet"
      }]
    }]
  }
}
```

**2. Inject a compact context summary at session start** (`SessionStart`):
```json
{
  "hooks": {
    "SessionStart": [{
      "hooks": [{
        "type": "command",
        "command": "paper context --budget 1200 --json"
      }]
    }]
  }
}
```

**3. On-demand retrieval from a prompt** (`UserPromptSubmit`) — only when the prompt mentions documents:
```json
{
  "hooks": {
    "UserPromptSubmit": [{
      "hooks": [{
        "type": "command",
        "command": "paper hook prompt --budget 800"
      }]
    }]
  }
}
```
`paper hook prompt` reads the event JSON from stdin, and emits **nothing** unless the prompt looks document-related — keeps token cost near zero.

**4. Call the MCP tool directly as a hook** (`mcp_tool`), for policy-style checks:
```json
{
  "hooks": {
    "PostToolUse": [{
      "matcher": "Write|Edit",
      "hooks": [{
        "type": "mcp_tool",
        "server": "paper",
        "tool": "search_documents",
        "input": { "query": "${tool_input.file_path}" }
      }]
    }]
  }
}
```

Sources: [Claude Code MCP](https://code.claude.com/docs/en/mcp); [Claude Code hooks](https://code.claude.com/docs/en/hooks/).

---

## 9. Performance budgets

| Metric | Budget |
|---|---|
| Cold start to first search | < 2 s |
| Warm keyword search (Spotlight) | < 100 ms |
| Warm semantic search (< 50k chunks) | < 50 ms |
| Index 10,000 documents | < 5 min (background) |
| Idle RAM | < 250 MB |
| Installer | < 40 MB |
| Mandatory model download | < 100 MB |

---

## 10. Security & privacy

- **No network by default.** Cloud is opt-in and previewed.
- **Root allowlist** for indexing and MCP reads.
- **Egress log** the user can open.
- **No telemetry.**
- **Signed releases**, public threat model.
- **Prompt-injection defense:** retrieved text is quoted as data; strip/flag "ignore previous instructions" patterns before handing to an OS/cloud model.
- **MCP over stdio by default**; HTTP only on loopback with a token.

---

## 11. Open questions & risks

1. **Spotlight coverage varies** by file type and volume; OCR quality is inconsistent. Fallback parser required.
2. **OS brain availability** is device-dependent and can be switched off. Extractive mode must be genuinely good.
3. **Reranker footprint** (~1 GB) conflicts with the small-install goal — keep it optional.
4. **Windows/Linux parity** for search: FTS5 is fine but not as good as Spotlight. Measure.
5. **Chunking quality** on scanned PDFs and tables is the biggest retrieval risk.
6. **No public benchmark** for this exact workload; build a 50-question eval set from real household documents before tuning.

---

## 12. Sources

- Spotlight: [Eclectic Light — quirks](https://eclecticlight.co/2025/09/04/quirks-of-spotlight-local-search/); [Eclectic Light — deeper dive](https://eclecticlight.co/2025/08/04/a-deeper-dive-into-spotlight-indexing-and-local-search/); [Apple NSMetadataQuery](https://developer.apple.com/library/archive/documentation/Carbon/Conceptual/SpotlightQuery/Concepts/QueryingMetadata.html); [Apple query syntax](https://developer.apple.com/library/archive/documentation/Carbon/Conceptual/SpotlightQuery/Concepts/QueryFormat.html); [StackOverflow result limits](https://stackoverflow.com/questions/68622160/how-to-limit-the-number-of-results-returned-from-nsmetadataquery); [NSMetadataQuery lifecycle](https://danielcosenza.com/posts/mac-spotlight-metadata-query-lifecycle/).
- Hybrid retrieval: [wz-it hybrid + rerank](https://wz-it.com/en/knowledge/ki/hybrid-search-reranking/); [Qdrant hybrid search](https://qdrant.tech/documentation/tutorials-basics/reranking-hybrid-search/); [turbopuffer hybrid latency](https://turbopuffer.com/docs/hybrid); [FoundryRAG](https://github.com/MEK-0/Local-Foundry-assistant); [SXZ.io RRF tutorial](https://sxz.io/hybrid-search-bm25-embeddings-rank-fusion-python-tutorial/).
- Embeddings: [EmbeddingGemma](https://developers.googleblog.com/en/introducing-embeddinggemma/); [ogma-micro](https://huggingface.co/axiotic/ogma-micro); [potion/model2vec](https://www.reddit.com/r/LocalLLaMA/comments/1sapdue/700kb_embedding_model_that_actually_works_built_a/); [bekko embedding](https://huggingface.co/blog/hotchpotch/bekko-embedding).
- On-device models: [Apple Foundation Models](https://developer.apple.com/wwdc26/guides/apple-intelligence/); [Windows Aion Instruct](https://devblogs.microsoft.com/foundry-on-windows/aion-instruct-local-ai-windows-gpu/); [Phi Silica docs](https://learn.microsoft.com/en-us/windows/ai/apis/phi-silica); [built-in AI comparison](https://flutteredge.ai/docs/builtin-ai).
- MCP / hooks: [Claude Code MCP](https://code.claude.com/docs/en/mcp); [Claude Code hooks](https://code.claude.com/docs/en/hooks/); [MCP server best practices](https://modelcontextprotocol.info/docs/best-practices/); [Workato MCP design](https://docs.workato.com/en/mcp/mcp-server-design.html); [Local-first docs MCP](https://www.grizzlypeaksoftware.com/articles/p/building-a-local-first-mcp-server-for-your-teams-documentation-a-step-by-step-gu-5ofzrd).
