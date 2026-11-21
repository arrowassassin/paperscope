# PaperScope

Local-first document search with exact page and line citations. Search your own documents offline through the paper CLI and its built-in MCP server.

## Use it

Ask Claude to search your documents. PaperScope returns exact file, page, and line citations. Nothing leaves your machine.

## Data

PaperScope reads only the directories you configure. It stores its index locally and sends nothing to the network unless you explicitly download model weights for semantic search.

## Install

- **Claude plugin:** Upload `paperscope-plugin.zip` at Customize > Plugins > Add > Upload plugin
- **Direct:** `cargo install paperscope` then `paper models install bge-small-en-v1.5`
- **Homebrew:** `brew install arrowassassin/tap/paper`

## License

MIT OR Apache-2.0
