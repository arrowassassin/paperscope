# PaperScope

Local-first document search with exact page and line citations. Search your own documents offline through the paper CLI and its built-in MCP server.

## Use it

Ask Claude to search your documents. PaperScope returns exact file, page, and line citations. Nothing leaves your machine.

## Data

PaperScope reads only the directories you configure. It stores its index locally and sends nothing to the network unless you explicitly download model weights for semantic search.

## Install

1. Install the `paper` binary once (it is not bundled inside the plugin):

```sh
curl -fsSL https://kintsugi.tools/paper.sh | sh
# or: brew install arrowassassin/tap/paper
# or: cargo install paperscope
```

2. Upload `paperscope-plugin.zip` at Customize > Plugins > Add > Upload plugin,
   or from Claude Code: `/plugin install paperscope --marketplace arrowassassin/paperscope`.

The plugin starts `paper serve --mcp` from your `PATH`, so run `paper --version`
first if the tools do not appear.

## License

MIT OR Apache-2.0
