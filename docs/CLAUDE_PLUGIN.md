# Using Paper with Claude and other coding agents

Paper is a local-first document search engine with exact page/line citations.
Any agent that speaks the [Model Context Protocol](https://modelcontextprotocol.io)
can use it through the single `paper` binary — no plugin store, account, or
cloud service involved.

> **There is no official "Claude plugin store".** Claude Desktop and Claude
> Code load MCP servers from a local JSON config. Distribution for Paper is
> therefore: crates.io, Homebrew, and GitHub Releases (this document covers
> the MCP wiring once `paper` is installed).

## 1. Install `paper`

```sh
# Prebuilt binary (macOS/Linux; SHA256-verified)
curl -fsSL https://kintsugi.tools/paper.sh | sh

# …or from source
cargo install paperscope

# …or Homebrew
brew install kintsugi-tools/tap/paper
```

The models download on their own on first run. To fetch one explicitly
(GitHub Release first, Hugging Face fallback; SHA256-verified, resumable):

```sh
paper models install bge-small-en-v1.5      # 134 MB, English-only
paper models install multilingual-e5-small  # 488 MB, multilingual (default)
paper models install bge-reranker-v2-m3     # optional 2.3 GB best-quality reranker
```

`PAPER_OFFLINE=1` works completely once models are cached: no network is ever
required for indexing or search.

## 2. Index your documents

```sh
paper index --root ~/Documents --root ~/Downloads

# …or run the background daemon: auto-discovers Documents/Downloads/Desktop,
# watches for changes, reindexes daily at 3 AM + every 5 min incrementally,
# and pauses on low battery (<20%) or high CPU (>80%).
paper daemon --foreground

# Optional: install as an OS service (systemd user unit / launchd agent)
paper daemon-control install
```

## 3. Wire up Claude Desktop

Edit `~/Library/Application Support/Claude/claude_desktop_config.json`
(macOS) or `%APPDATA%\Claude\claude_desktop_config.json` (Windows):

```json
{
  "mcpServers": {
    "paper": {
      "command": "paper",
      "args": ["serve", "--mcp"]
    }
  }
}
```

Use the absolute path to `paper` if it is not on the default `PATH`
(`which paper` / `where paper`). Restart Claude Desktop after editing.

## 4. Wire up Claude Code

```sh
claude mcp add paper -- paper serve --mcp
```

or add the same `mcpServers` block to `.mcp.json` in your project root.

## 5. Other MCP agents

Any MCP client works the same way — spawn `paper serve --mcp` over stdio. Examples:

- **Codex CLI**: add to `~/.codex/config.toml`:
  ```toml
  [mcp_servers.paper]
  command = "paper"
  args = ["serve", "--mcp"]
  ```
- **OpenCode**: add to `opencode.json`:
  ```json
  {
    "mcp": {
      "paper": {
        "type": "local",
        "command": ["paper", "serve", "--mcp"]
      }
    }
  }
  ```

## What the agent can do

The MCP server exposes search over your indexed documents. Every result
includes `path`, `page`, `line_start`, `line_end`, `anchor`, and `section`, so
the agent can cite your documents precisely instead of guessing. Answers are
extractive by default; generative polishing only happens with an explicit
`--polish`.

## Verifying the setup

```sh
paper doctor            # index, models, and environment health check
paper search "warranty" # CLI search works without any agent
```

Protocol note: Paper negotiates MCP protocol versions — a client requesting
`2024-11-05` gets exactly that; current clients get `2025-11-25`.

## Community listing

A PR to the community server list at
[`modelcontextprotocol/servers`](https://github.com/modelcontextprotocol/servers)
is welcome once v0.1.0 is tagged; it is a listing, not a store, and is not
required for any of the above to work.
