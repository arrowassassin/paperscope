# Cross-platform install and MCP registration for a stdio server (`paper serve --mcp`)

Scope: how to auto-register a single stdio MCP server into every AI coding client that
happens to be installed on a machine, idempotently and without destroying user config.
State of the world as of **November 2026**. Dated items are called out explicitly.

Two cross-cutting facts drive almost every design decision below:

1. **The stdio transport has no negotiation for "how do I start you".** The client simply
   spawns `command` with `args` and a merged environment, then exchanges newline-delimited
   JSON-RPC on stdin/stdout. Messages "are delimited by newlines and **MUST NOT** contain
   embedded newlines"; the server "**MAY** write UTF-8 strings to `stderr`"
   ([MCP spec, stdio transport, 2026-07-28](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/stdio)).
   Every client therefore invents its own config schema around `command`/`args`/`env`, and
   none of them share one.

2. **There is no standard for "register yourself with the clients".** The only real
   standardization is Anthropic's `.mcpb` Desktop Extension bundle (single-click install
   *into Claude Desktop*) and Cursor/VS Code deep links. Everything else is "write the
   vendor's config file" or "shell out to the vendor's CLI".

---

## Client-by-client reference table: config path per OS, format, exact stdio schema, and registration mechanism

### Takeaway

There are **five distinct schema families** — `mcpServers` object (Claude Code/Desktop,
Cursor, Windsurf, Cline, Gemini, Copilot, Antigravity), `servers` object (VS Code
editor-managed), `context_servers` object (Zed), `mcp.servers` object with a **command
array** (OpenCode V2), `[mcp_servers.*]` TOML tables (Codex), `mcp_servers` **YAML mapping**
(Hermes), and a **YAML array** keyed by `name` (Continue). Eight of the thirteen expose a
vendor CLI that can register a server; five do not, and for at least one (Codex) the CLI is
*documented-by-bug-report* to destroy user comments. A correct installer must therefore be
a merge tool first and a CLI wrapper second.

### Master table (locations + format)

| Client | macOS | Linux | Windows | Format | Servers key |
|---|---|---|---|---|---|
| **Claude Code** | `~/.claude.json` (user+local), `<proj>/.mcp.json` (project) | same | `%USERPROFILE%\.claude.json`, `<proj>\.mcp.json` | JSON | `mcpServers` |
| **Claude Desktop** | `~/Library/Application Support/Claude/claude_desktop_config.json` | `$XDG_CONFIG_HOME/Claude/claude_desktop_config.json` else `~/.config/Claude/…` | `%APPDATA%\Claude\claude_desktop_config.json` | JSON | `mcpServers` |
| **OpenCode** | `~/.config/opencode/opencode.json[c]` (global), `<proj>/opencode.json[c]` | same | `%USERPROFILE%\.config\opencode\opencode.json` | JSON/**JSONC** | `mcp.servers` (V2) / `mcp` (V1) |
| **Codex CLI** | `~/.codex/config.toml`, `<proj>/.codex/config.toml` | same | `%USERPROFILE%\.codex\config.toml` | **TOML** | `[mcp_servers.<name>]` |
| **Cursor** | `~/.cursor/mcp.json`, `<proj>/.cursor/mcp.json` | same | `%USERPROFILE%\.cursor\mcp.json` | JSON | `mcpServers` |
| **Windsurf** | `~/.codeium/windsurf/mcp_config.json` | same | `%USERPROFILE%\.codeium\windsurf\mcp_config.json` | JSON | `mcpServers` |
| **VS Code** | `~/Library/Application Support/Code/User/mcp.json` (editor), `~/.copilot/mcp-config.json` (native) | `~/.config/Code/User/mcp.json` | `%APPDATA%\Code\User\mcp.json` | JSON(C) | `servers` (editor) / `mcpServers` (native) |
| **Zed** | `~/.config/zed/settings.json`, `<proj>/.zed/settings.json` | same | `%APPDATA%\Zed\settings.json` | **JSONC** | `context_servers` |
| **Cline** | `~/.cline/data/settings/cline_mcp_settings.json` (CLI); `~/Library/Application Support/Code/User/globalStorage/saoudrizwan.claude-dev/settings/cline_mcp_settings.json` (ext) | `~/.config/Code/User/globalStorage/…` | `%APPDATA%\Code\User\globalStorage\…` | JSON | `mcpServers` |
| **Continue** | `~/.continue/config.yaml` or `config.json`; `<proj>/.continue/mcpServers/<n>.yaml` | same | `%USERPROFILE%\.continue\config.yaml` | **YAML** (legacy JSON) | `mcpServers` (**array**) |
| **Gemini CLI** | `~/.gemini/settings.json`, `<proj>/.gemini/settings.json`; sys `/Library/Application Support/GeminiCli/settings.json` | `…`; sys `/etc/gemini-cli/settings.json` | `%USERPROFILE%\.gemini\settings.json`; sys `C:\ProgramData\gemini-cli\settings.json` | JSON | `mcpServers` |
| **Hermes** (Nous Research) | `~/.hermes/config.yaml` | same | `%USERPROFILE%\.hermes\config.yaml` | **YAML** | `mcp_servers` (mapping) |
| **Antigravity** (Google) | `~/.gemini/config/mcp_config.json`, `<ws>/.agents/mcp_config.json` | same | `%USERPROFILE%\.gemini\config\mcp_config.json` | JSON | `mcpServers` |

Sources for the table, row by row: Claude Code scopes/paths —
[Claude Code MCP quickstart](https://code.claude.com/docs/en/mcp-quickstart);
Claude Desktop three-OS paths (VS Code's own discovery table, dated 2026-11-18) —
[VS Code MCP configuration reference](https://code.visualstudio.com/docs/agents/reference/mcp-configuration);
OpenCode —
[OpenCode MCP docs](https://opencode.ai/docs/mcp-servers.md) plus the per-client path table in
[Wolfram AgentTools `mcp-clients.md`](https://github.com/WolframResearch/AgentTools/blob/main/docs/mcp-clients.md);
Codex —
[OpenAI Codex MCP docs (archived 2025-12-30)](https://web.archive.org/web/20251230203753/https:/developers.openai.com/codex/mcp) and
[Codex config sample](http://developers.openai.com/codex/config-sample);
Cursor — [Cursor MCP docs](https://cursor.com/docs/mcp) and
[Cursor help: customization/MCP](https://cursor.com/help/customization/mcp);
Windsurf — [IONOS Windsurf setup](https://docs.ionos.com/cloud/ai/mcp-server/connect-to-an-ai-client/windsurf) and
[Grafana Cloud Windsurf client page](https://grafana.com/docs/grafana-cloud/ai-tools/mcp-servers/oss-mcp/clients/windsurf/);
VS Code — [MCP configuration reference](https://code.visualstudio.com/docs/agents/reference/mcp-configuration);
Zed — [Zed MCP docs](https://zed.dev/docs/ai/mcp);
Cline — [Cline MCP overview](https://docs.cline.bot/mcp/mcp-overview);
Continue — [Continue MCP deep dive](https://docs.continue.dev/customize/deep-dives/mcp);
Gemini — [Gemini CLI MCP servers](https://geminicli.com/docs/tools/mcp-server/) and
[settings docs](https://geminicli.com/docs/cli/settings/);
Hermes — [Hermes MCP feature docs](https://hermes-agent.nousresearch.com/docs/user-guide/features/mcp);
Antigravity — [Antigravity MCP docs](https://antigravity.google/docs/mcp/).

---

### Per-client schema detail

#### Claude Code — JSON, three scopes, CLI is the sanctioned path

Exact stdio entry (`.mcp.json`, `~/.claude.json`, or `claude mcp add-json`):

```json
{"mcpServers": {"paper": {"type": "stdio", "command": "/abs/path/paper", "args": ["serve", "--mcp"], "env": {}}}}
```

- Scopes: `local` (default) writes into `~/.claude.json` **under the project path key**;
  `project` writes `<repo>/.mcp.json`; `user` writes the **top-level** `mcpServers` of
  `~/.claude.json`. "Run `claude mcp get <name>` to see which scope holds a server."
  ([quickstart](https://code.claude.com/docs/en/mcp-quickstart))
- CLI: `claude mcp add [--scope user|project|local] [--transport stdio] <name> -- <cmd> <args…>`,
  `claude mcp add-json <name> '<json>' [--scope …]`, `claude mcp list|get|remove`,
  `claude mcp reset-project-choices`. Piping JSON via `-` requires v2.1.327+
  ([Claude Code MCP](https://code.claude.com/docs/en/mcp)).
- Env expansion `${VAR}` / `${VAR:-default}` works in `command`, `args`, `env`, `url`,
  `headers` — so an installer *can* write `${HOME}`-relative commands, but absolute paths
  are still safer.
- Precedence is local > project > user > plugins > claude.ai connectors; duplicates are
  matched **by name**, and "the entire server entry from that source is used; fields are
  not merged across scopes."
- `CLAUDE_CONFIG_DIR` relocates `.claude.json`.
- **Schema trap:** an entry with a `url` but no `type` is treated as stdio and rejected —
  "add `type: http`". A `type: stdio` entry is always required for us.
- Project scope triggers an approval prompt in interactive sessions, so project-scope
  registration is *not* silent for the user.
- Docs explicitly warn that `~/.claude/.mcp.json`, `~/.claude/config/mcp.json`,
  `~/.claude/mcp.json` and `%APPDATA%\Claude\mcp.json` are **not read** — path guessing
  fails silently here.

#### Claude Desktop — JSON, plus `.mcpb` as the new preferred channel

- Same `mcpServers` + `command`/`args`/`env` shape as Claude Code; no `type` needed.
- JSON path per OS as in the master table; the file is reachable from
  **Settings → Developer → Edit Config**, which creates it if missing
  ([Claude support](https://support.claude.com/en/articles/10949351-getting-started-with-local-mcp-servers-on-claude-desktop)).
- Since 2025 the *preferred* distribution is a **Desktop Extension** (`.mcpb`, formerly
  DXT): a bundle with a `manifest.json` + payload that installs with one click, gets
  automatic updates, and can ship its own Node runtime. "Claude for macOS and Windows uses
  the code in this repository to enable single-click installation of local MCP servers …
  such as automatic updates" ([modelcontextprotocol/mcpb](https://github.com/modelcontextprotocol/mcpb));
  see also [Anthropic's launch post](https://anthropic.com/research/desktop-extensions) and
  [Build a desktop extension with MCPB](https://claude.com/docs/connectors/building/mcpb).
- Practical consequence: writing `claude_desktop_config.json` still works and is still
  documented, but it is now the *manual/developer* path. Enterprise admins can disable
  public extensions and upload their own; extension state can also be account-managed,
  which can override a local edit.

#### OpenCode — JSONC, and a **V1/V2 schema split** that will bite you

V2 (current) — note `mcp.servers`, `command` as an **array**, and `environment` (not `env`):

```jsonc
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "servers": {
      "paper": {
        "type": "local",
        "command": ["/abs/path/paper", "serve", "--mcp"],
        "cwd": ".",
        "environment": {"PAPER_HOME": "/Users/me/.paper"},
        "disabled": false
      }
    }
  }
}
```

- "V2 does not place server names directly under `mcp`"; V1 used `mcp.<name>` with
  `command` as an array and `enabled: true`. V2 replaced `enabled` with **`disabled`**.
- Fields: `type` (required, `"local"`), `command` (required, exe **+ args in one array**),
  `cwd` (relative resolves from workspace), `environment`, `disabled`, `codemode`,
  `timeout` (object: `startup`/`catalog`/`execution`, ms), `protocol`, `pool`.
- Env substitution is `{env:NAME}`; "Shell expressions such as `$NAME` are not expanded in
  JSON strings." (Contrast with Gemini/Claude Code — a shared config generator that emits
  `${VAR}` will break here.)
- CLI: `opencode mcp add <name> [--global] [--env K=V] -- <cmd> <args…>`,
  `opencode mcp list`, `opencode mcp auth|logout`. **There is no `opencode mcp remove`** —
  "To remove a server, delete its entry from the project or global configuration."
- Files are `.json` **or `.jsonc`** — comments are legal, so a naive `serde_json` round-trip
  will silently delete user comments.
- Source: [OpenCode MCP docs](https://opencode.ai/docs/mcp-servers.md).

#### Codex CLI — TOML, and the CLI rewrites your whole file

```toml
[mcp_servers.paper]
command = "/abs/path/paper"
args = ["serve", "--mcp"]
enabled = true
startup_timeout_sec = 30      # default 10s; raise for slow first-run indexing
tool_timeout_sec = 120        # default 60s

[mcp_servers.paper.env]
PAPER_HOME = "/Users/me/.paper"
```

- Global `~/.codex/config.toml`; per-project `.codex/config.toml` only "for projects you've
  marked as trusted" ([Codex MCP](https://dev.to/vibeidedev/codex-mcp-add-configure-and-debug-mcp-servers-in-codex-cli-1j1p)).
- CLI: `codex mcp add`, `codex mcp list`, `codex mcp remove`.
- **Critical:** `codex mcp add / mcp remove` "rewrite every `[mcp_servers.*]` entry, silently
  dropping comments and unknown keys" — the bug report shows inline comments, `enabled`,
  and a forward-compat `future_unknown_key` all lost
  ([openai/codex#45432](https://github.com/openai/codex/issues/45432)). For Codex, a
  text-preserving TOML append is strictly safer than the vendor CLI.
- Timeouts are the usual first failure: startup defaults to 10s and the error tells you to
  "add or adjust `startup_timeout_sec`" ([openai/codex#17024](https://github.com/openai/codex/issues/17024)).
  `startup_timeout_ms` is also accepted; the seconds key wins if both are set.
- `enabled_tools` / `disabled_tools` exist (disabled applied after enabled). `CODEX_HOME`
  appears to relocate `~/.codex` (reported by third-party installers, not verified in
  OpenAI docs).

#### Cursor — JSON, global + project merged, plus deep links

- Global `~/.cursor/mcp.json` ("Create `~/.cursor/mcp.json` in your home directory for tools
  available everywhere"), project `.cursor/mcp.json`; "Both files are merged. If the same
  server name appears in both, the [project wins]"
  ([Cursor docs](https://cursor.com/docs/mcp), [Cursor help](https://cursor.com/help/customization/mcp)).
- Schema is Claude-Desktop-compatible `mcpServers` with `command`/`args`/`env`.
- No documented MCP-registration CLI. Registration is via **Settings → Tools & MCP → New MCP
  Server** or the file.
- One-click install is a URL: `cursor://anysphere.cursor-deeplink/mcp/install?name=<n>&config=<base64(JSON)>`
  (also surfaced as `https://cursor.com/en-US/install-mcp?…`). "The config is a
  base64-encoded JSON holding the server address."
  ([write-up](https://dev.to/eugeniya_ivanova_4a58eadc/we-shipped-a-one-click-add-to-cursor-button-the-hard-part-wasnt-the-button-141m),
  [Cursor docs](https://cursor.com/docs/mcp)).
- Windows: `%USERPROFILE%\.cursor\mcp.json`; a known bug makes Cursor's own "open global
  mcp.json" link fail on Windows even though the path is correct
  ([forum](https://forum.cursor.com/t/windows-clicking-global-mcp-json-path-in-chat-fails-unable-to-resolve-resource/164879)).
  Also reported: global `mcp.json` occasionally ignored while project-level works
  ([forum](https://forum.cursor.com/t/global-mcp-json-ignored-project-level-works-fine/152706)) —
  i.e. Cursor's global config has real reliability warts.
- Cursor is a VS Code fork, so a *third* candidate path
  (`%APPDATA%\Cursor\User\globalStorage\mcp.json`) circulates in blog posts; it is not in
  Cursor's docs and should not be written.

#### Windsurf — JSON, Claude Desktop schema, but path churn post-acquisition

- `~/.codeium/windsurf/mcp_config.json` on macOS/Linux,
  `%USERPROFILE%\.codeium\windsurf\mcp_config.json` on Windows; "The file follows the same
  JSON schema as Claude Desktop" ([IONOS](https://docs.ionos.com/cloud/ai/mcp-server/connect-to-an-ai-client/windsurf),
  [Grafana](https://grafana.com/docs/grafana-cloud/ai-tools/mcp-servers/oss-mcp/clients/windsurf/)).
- **Conflicting evidence on the current path.** Older/other docs give
  `~/.codeium/mcp_config.json` (no `windsurf/` segment), and a 2026 Windsurf/Cascade doc
  says the in-app "Open MCP config file" action now opens the Cortex-managed file at
  `~/.config/devin/mcp_config.json` on macOS/Linux
  ([docs.devin.ai](https://docs.devin.ai/windsurf/plugins/cascade/mcp)) — consistent with
  the Cognition/Devin ownership of Windsurf. Treat Windsurf as **two possible paths**;
  prefer whichever already exists, and do not create `~/.config/devin/`.
- No documented CLI. No project-scope config in any source I found.

#### VS Code — the biggest 2026 change: two schemas, one of them deprecated

Late-2026 VS Code has **two** config surfaces, and the docs are blunt about it:

1. **Native Copilot config (new, current):** top-level `mcpServers`, file
   `$COPILOT_HOME/mcp-config.json` or `~/.copilot/mcp-config.json`, and workspace-root
   `.mcp.json`. Fields: `type: "stdio"`, `command`, `args`, `env`, `cwd`, `tools: ["*"]`,
   `${NAME}` / `${NAME:-default}` references. "For new Copilot configurations, use these
   native destinations, **not** `.vscode/mcp.json` or profile `mcp.json`."
2. **Editor-managed config (deprecated "Local harness", removal February 2027):** top-level
   `servers`, in `.vscode/mcp.json` (workspace) or a profile `mcp.json` (user: macOS
   `~/Library/Application Support/Code/User/mcp.json`, Windows `%APPDATA%\Code\User\mcp.json`,
   Linux `~/.config/Code/User/mcp.json`). Fields: `type`, `command`, `args`, `cwd`, `env`,
   `envFile`, `dev`, `sandboxEnabled`, `sandbox`, and `${input:…}` prompts.
   Env references here are `${env:NAME}`, not `${NAME}`.
   All from [VS Code MCP configuration reference, 2026-11-18](https://code.visualstudio.com/docs/agents/reference/mcp-configuration).

- CLI: `code --add-mcp '{"name":"paper","type":"stdio","command":"/abs/paper","args":["serve","--mcp"]}'`.
  This "adds a server to the VS Code user-profile configuration" and "uses the VS Code
  format, not the portable Copilot user file" — i.e. it writes the **deprecated** surface.
- VS Code can auto-discover other clients' servers via `chat.mcp.discovery.enabled`
  (sources: `claude-desktop`, `copilot`, `cursor-global`, `cursor-workspace`, `windsurf`;
  only Copilot CLI is on by default). This is both a threat (double registration) and an
  opportunity (register once in Claude Desktop, get VS Code free).
- Because the user's request names `.vscode/mcp.json`, the honest answer is: writing it
  still works today, but it targets a surface slated for removal in Feb 2027; write the
  native `mcpServers` file on new installs and optionally both.

#### Zed — JSONC `context_servers`, no CLI

Current documented form is **flat** (`command` is a string, args separate):

```json
{"context_servers": {"paper": {"command": "/abs/path/paper", "args": ["serve", "--mcp"], "env": {}}}}
```

"[Settings → AI → MCP Servers] … will create entries in your settings file … similar to the
ones below" ([Zed MCP docs](https://zed.dev/docs/ai/mcp)).

- Remote form is `{"url": …, "headers": {…}}` under the same key; without an
  `Authorization` header Zed runs the MCP OAuth flow.
- Paths: `~/.config/zed/settings.json` (macOS/Linux), `%APPDATA%\Zed\settings.json`
  (Windows), plus `.zed/settings.json` per project
  ([Wolfram AgentTools table](https://github.com/WolframResearch/AgentTools/blob/main/docs/mcp-clients.md)).
- **Schema ambiguity to flag:** several 2026 vendor guides show a *nested* form
  `{"source": "custom", "command": {"path": "npx", "args": [], "env": {}}}`
  ([IONOS](https://docs.ionos.com/cloud/ai/mcp-server/connect-to-an-ai-client/zed),
  [Grafana](https://grafana.com/docs/grafana/latest/developer-resources/mcp/clients/zed)).
  The authoritative docs page and Grafana's Zed page both show flat `command` + `args`, so
  the nested form is almost certainly the older draft schema; do **not** emit it.
- No registration CLI. Extensions are the vendor-blessed distribution channel for popular
  servers; arbitrary servers go through the settings file.
- `settings.json` is JSONC (Zed documents commented defaults), so comments must survive.

#### Cline — JSON in VS Code globalStorage, plus a real CLI

```json
{"mcpServers": {"paper": {"command": "/abs/paper", "args": ["serve","--mcp"], "env": {}, "disabled": false, "autoApprove": []}}}
```

- Two different files depending on surface
  ([Cline MCP overview](https://docs.cline.bot/mcp/mcp-overview)):
  - CLI: `~/.cline/data/settings/cline_mcp_settings.json`, overridable with
    `CLINE_MCP_SETTINGS_PATH` (also `CLINE_DIR` / `--config` → `<config-dir>/data/settings/…`).
  - VS Code extension: VS Code `globalStorage` for extension id `saoudrizwan.claude-dev`,
    i.e. macOS `~/Library/Application Support/Code/User/globalStorage/saoudrizwan.claude-dev/settings/cline_mcp_settings.json`,
    Windows `%APPDATA%\Code\User\globalStorage\…`, Linux `~/.config/Code/User/globalStorage/…`.
- Cline adds `disabled: false` and `autoApprove: []`; remote entries use
  `"type": "streamableHttp"` and omitting `type` defaults to **legacy SSE** (easy mistake).
- CLI: `cline mcp add <name> --yes -- <cmd> <args…>`, `cline mcp add <url> --transport http
  --header … --yes`, `cline mcp remove <name>`, `cline config mcp --json`.
- `autoApprove` is a security footgun: an installer must write `[]`, never `["*"]`.

#### Continue — YAML, and `mcpServers` is an **array**, not an object

```yaml
name: Local Config
version: 1.0.0
schema: v1
mcpServers:
  - name: paper
    type: stdio
    command: /abs/path/paper
    args: ["serve", "--mcp"]
    env: {}
```

- `~/.continue/config.yaml` (current), `~/.continue/config.json` (legacy; Continue publishes
  a [YAML migration guide](https://docs.continue.dev/reference/yaml-migration)), and
  per-server block files at `<proj>/.continue/mcpServers/<name>.yaml` which the IDE
  extensions auto-discover.
- Because it's an array, upsert is keyed on the **`name` field inside each element**, and
  duplicate `name` entries are possible in a way they are not in map-shaped clients.
- Top-level `name`/`version`/`schema: v1` are required in *every* config.yaml; without them
  "Continue's CLI and IDE plugin silently reject the file and fall back to 'Default Config'
  with no MCP servers visible"
  ([Wolfram AgentTools](https://github.com/WolframResearch/AgentTools/blob/main/docs/mcp-clients.md)).
- The `cn` CLI reads a **Hub-hosted** config by default, not your local file — you must pass
  `cn --config ~/.continue/config.yaml`. No `cn mcp add` is documented.
- MCP is only active in Continue's **agent mode**.

#### Gemini CLI — JSON, rich schema, and a deprecation clock

```json
{"mcpServers": {"paper": {"command": "/abs/paper", "args": ["serve","--mcp"], "env": {}, "cwd": "/abs", "timeout": 30000, "trust": false}}}
```

- `~/.gemini/settings.json` (user) or `.gemini/settings.json` (workspace); system-level
  files exist at `/etc/gemini-cli/settings.json` (Linux),
  `C:\ProgramData\gemini-cli\settings.json` (Windows),
  `/Library/Application Support/GeminiCli/settings.json` (macOS) and **outrank** user
  settings.
- `timeout` is **milliseconds**, default 600000 (10 min); `trust: true` bypasses all tool
  confirmation; `includeTools`/`excludeTools` filter; `cwd` supported.
- CLI: `gemini mcp add [-s user|project] [-t stdio|sse|http] [-e K=V] [--timeout ms]
  [--trust] <name> <command> [args…]`. **Default scope is `project`**, not user — the
  opposite default from Claude Code. `--scope user` writes `~/.gemini/settings.json`.
- Env expansion in `env`: `$VAR`, `${VAR}`, `%VAR%` (Windows). Gemini also *sanitizes* the
  inherited environment, redacting `*TOKEN*`, `*SECRET*`, `*KEY*`, `*AUTH*`,
  `*CREDENTIAL*` — so a server needing a secret must have it declared explicitly in `env`.
- `MCPServerConfig` in the settings schema is declared `additionalProperties: false`
  ([settingsSchema.ts](https://github.com/google-gemini/gemini-cli/blob/ce84b3cb/packages/cli/src/config/settingsSchema.ts)),
  which means **adding an installer marker key (e.g. `"_managedBy": "paper"`) inside the
  server entry will invalidate the entry.** Ownership must be tracked out-of-band.
- **Deprecation:** "Google is retiring Gemini CLI for free / consumer tiers on **June 18,
  2026** and unifying its CLI offering under Antigravity CLI", per the migration guide
  ([antigravity.google/docs/cli/gcli-migration](https://antigravity.google/docs/cli/gcli-migration))
  as summarized by Wolfram's client table, which links
  [developers.googleblog.com](https://developers.googleblog.com/an-important-update-transitioning-gemini-cli-to-antigravity-cli/).
  I did not fetch the Google blog post itself; the geminicli.com docs are still live and
  detailed as of this writing. **Treat Gemini CLI as "still present on many machines,
  schema documented, but on a migration path to Antigravity"** — register into it if its
  config/data dir exists, and do not treat it as the future.

#### Hermes — YAML `mcp_servers` mapping (Nous Research's agent)

**What it is:** "Hermes Agent" from Nous Research, a terminal/desktop/messaging coding agent
with its own MCP *client* and also an MCP *server* (`hermes mcp serve`). It is not the
Hermes JS engine, and not a Meta model.

```yaml
mcp_servers:
  paper:
    command: "/abs/path/paper"
    args: ["serve", "--mcp"]
    env: {}
    cwd: "/Users/me/project"      # optional; defaults to session cwd
    timeout: 120                  # tool call timeout, seconds
    connect_timeout: 60           # also bounds the initialize handshake
    enabled: true
    lazy: false
```

- Path: `~/.hermes/config.yaml`, key `mcp_servers` (snake_case; the docs explicitly map
  Claude's `mcpServers` → `mcp_servers`, and `hermes import-agent claude-code` performs that
  migration automatically).
- Extra per-server knobs: `lazy` (register from schema cache, spawn on first call),
  `idle_timeout_seconds`, `max_lifetime_seconds`, `supports_parallel_tool_calls`,
  `tools.include`/`tools.exclude` (fnmatch globs), `sampling`, `elicitation`.
- Registration mechanisms: interactive `hermes mcp` picker, `hermes mcp install <catalog-name>`,
  `hermes mcp add <name> --preset <preset>` (only presets like `codex` documented),
  `hermes mcp configure|login`, `/reload-mcp` at runtime.
- `${VAR}` substitution from the environment **including `~/.hermes/.env`**, plus
  Cursor-style `${userHome}`, `${workspaceFolder}`, `${workspaceFolderBasename}`,
  `${pathSeparator}`.
- Security note relevant to us: for stdio servers Hermes "does not blindly pass your full
  shell environment. Only explicitly configured `env` plus a safe baseline are passed
  through" — so anything Paper needs at startup (e.g. `PAPER_HOME`) must be explicit.
- Source: [Hermes MCP docs](https://hermes-agent.nousresearch.com/docs/user-guide/features/mcp).

#### Antigravity — JSON `mcpServers` with its own field names (Google)

**What it is:** Google Antigravity is the 2026 agentic IDE + desktop app + CLI
(Antigravity App / Antigravity IDE / Antigravity CLI), the successor surface to Gemini CLI.
All three "share a central MCP configuration in the file `~/.gemini/config/mcp_config.json`"
([Google Codelab](https://codelabs.developers.google.com/google-workspace-mcp-antigravity)).

```json
{"mcpServers": {"paper": {"command": "/abs/paper", "args": ["serve","--mcp"], "env": {}, "cwd": "/abs", "timeoutSeconds": 600}}}
```

- Global `~/.gemini/config/mcp_config.json`; workspace `.agents/mcp_config.json`; plugin
  `…/mcp_config.json`. Pre-2.0 installs used `~/.gemini/antigravity/mcp_config.json` and are
  migrated when a zero-byte marker `~/.gemini/config/.migrated` appears. Do **not** write
  `~/.gemini/antigravity-cli/mcp_config.json` — that directory holds skills/cache/settings,
  not MCP config.
- Field names diverge from everyone else: `serverUrl` (and `url` as alias) for remote;
  legacy `httpUrl` and HTTP+SSE are **not supported**; `timeoutSeconds` (seconds, integer,
  default 180/tool call and 300/connect, `-1` disables); `disabled`;
  `enabledTools`/`disabledTools`; per-tool `tools.<name>.eager` / `tools.<name>.background`;
  `oauth`; `authProviderType: "google_credentials"`.
- Registration: no documented `antigravity mcp add` subcommand. In the App it's
  Customizations → Installed → MCP Servers; in the IDE it's `…` → MCP Servers → Manage MCP
  Servers → View raw config; in the CLI it's the interactive `/mcp` manager. **Direct file
  write is the only automatable path**, which raises the risk profile.
- Known hazards, all from Google's forum/Wolfram's field notes: (a) the app caches config at
  launch and "reload window" is not always enough — full quit/relaunch; (b) macOS GUI launch
  doesn't inherit the login shell `$PATH`, so MCP servers that rely on PATH "crash silently"
  — pass an absolute command path; (c) cloud-synced settings "may overwrite your local
  `mcp_config.json` on next launch"; (d) a Windows 2.0 installer directory conflict can make
  the wrong `app.asar` launch. Sources:
  [Antigravity MCP docs](https://antigravity.google/docs/mcp/),
  [Antigravity settings docs](https://antigravity.google/docs/settings.md),
  [forum thread on the v2.7/v2.32 config move](https://discuss.ai.google.dev/t/custom-mcp-setting-not-visible-in-settings/200461).

### Cited Findings (cross-client)

- Eight clients ship a registration CLI: Claude Code, OpenCode, Codex, Cline, Gemini CLI,
  Hermes, plus VS Code (`code --add-mcp`) and Continue's `cn --config` (config *selection*,
  not registration) — [Claude Code](https://code.claude.com/docs/en/mcp),
  [OpenCode](https://opencode.ai/docs/mcp-servers.md),
  [Codex](https://dev.to/vibeidedev/codex-mcp-add-configure-and-debug-mcp-servers-in-codex-cli-1j1p),
  [Cline](https://docs.cline.bot/mcp/mcp-overview),
  [Gemini](https://geminicli.com/docs/tools/mcp-server/),
  [Hermes](https://hermes-agent.nousresearch.com/docs/user-guide/features/mcp),
  [VS Code](https://code.visualstudio.com/docs/agents/reference/mcp-configuration).
- Five have no registration CLI at all: Claude Desktop, Cursor, Windsurf, Zed, Antigravity.
- Exactly one client (so far) documents a first-class *bundle* distribution:
  Claude Desktop `.mcpb`.
- Two clients silently couple the config file to running application state: Claude Code's
  `~/.claude.json` (a large state file holding projects, approvals and history) and VS Code's
  profile `mcp.json`. Writing either while the app is running is a lost-update hazard.
- One client's schema forbids extra keys (`additionalProperties: false` on Gemini's
  `MCPServerConfig`), one is JSONC (OpenCode, Zed), two are YAML (Continue, Hermes), one is
  TOML (Codex).
- Three clients are in documented migration: Gemini CLI → Antigravity (consumer, June 2026);
  OpenCode V1 → V2 (`mcp.<name>` → `mcp.servers.<name>`); VS Code Local harness → Copilot
  harness (removal Feb 2027, `.vscode/mcp.json` deprecated).

### Inferences

- A single canonical entry cannot be templated as a string. You need one *serializer per
  format* (JSON, JSONC-preserving, TOML, YAML, YAML-array) and one *adapter per client*
  that maps `{command, args, env}` → the client's fields (`command` array for OpenCode,
  `timeoutSeconds` for Antigravity, `type: local` vs `type: stdio`, `environment` vs `env`).
- Because Gemini rejects unknown keys and Codex's CLI drops unknown keys, **ownership must
  live outside the client config**. A sidecar state file (e.g.
  `$PAPER_HOME/clients.json` recording `{client, path, key, entryHash, installedAt,
  installerVersion}`) is the only mechanism that survives all thirteen formats.
- Absolute `command` paths are mandatory, not a nicety: three independent failure reports
  (Antigravity GUI PATH, Cursor Windows path bugs, macOS GUI apps not inheriting shell PATH)
  all reduce to "the client may not have your shell environment".
- Because several clients *auto-discover* each other's configs (VS Code's
  `chat.mcp.discovery.enabled`, Cursor's global+project merge), registering everywhere can
  produce duplicate tool surfaces. Deduplication is the client's job, but the installer
  should warn or default to "one primary client + explicit opt-in".

### Gaps

- **Windsurf's actual current path** is contradictory across sources
  (`~/.codeium/windsurf/mcp_config.json` vs `~/.codeium/mcp_config.json` vs
  `~/.config/devin/mcp_config.json`). No primary Windsurf doc page was retrievable; the
  Devin-owned docs suggest a migration that no changelog confirms.
- **Zed's nested `command: {path, args, env}` form** appears in several 2026 vendor guides
  but not in Zed's own current docs. Which versions accept it, and whether it is still
  accepted, is unverified.
- **Codex `cwd`**: I found no evidence of a per-server working-directory field in
  `[mcp_servers.*]`. Assume unsupported.
- **Claude Code per-server `timeout`**: not in the documented schema (only env vars such as
  `MCP_TIMEOUT` are referenced in community material). Unverified.
- **VS Code per-server timeout**: not documented in either schema.
- **OpenCode V1 → V2 version detection**: docs describe the V2 shape but do not state a
  version boundary or a feature flag. `opencode --version` is the only signal, and the
  mapping from version to schema is unverified.
- **Hermes `hermes mcp add` for arbitrary (non-preset) stdio servers**: only `--preset` and
  catalog installs are documented; whether a bare `hermes mcp add paper -- /abs/paper serve
  --mcp` works is unverified.
- **Antigravity CLI/App config precedence** when both `.agents/mcp_config.json` and the
  global file define the same name: docs say "the server that is already running keeps it",
  which is order-dependent and not something an installer can reason about.
- **Gemini CLI's post-June-2026 status** on existing installs: the docs remain live, but
  whether free-tier users are hard-blocked, soft-migrated, or unaffected in practice is
  unverified (I did not read Google's blog post directly).

---

## Detection strategy: which clients are actually installed, and how to avoid registering everywhere

### Takeaway

Config-file existence alone is the *documented* detection primitive used by at least one
shipping multi-client installer, but it under-detects first-run clients (no file yet) and
over-detects stale uninstalls (file left behind). The safe combination is: **strong signal
(app bundle / binary on PATH / vendor data directory) → weak signal (config file exists) →
explicit user flag**, and a strict rule of never creating a vendor's config directory that
did not already exist.

### Cited Findings

- Prior art exists in exactly this shape: Wolfram's `InstallMCPServer` ships a
  `$SupportedClients` registry with per-OS `"InstallLocation"`, and
  `DetectedMCPClients[]` "returns the subset of `$SupportedMCPClients` whose user-scope MCP
  config file exists on the current machine … Detection is based purely on the existence of
  each client's `InstallLocation` config file for the current `$OperatingSystem`; project-scope
  config files are not checked."
  ([AgentTools mcp-clients.md](https://github.com/WolframResearch/AgentTools/blob/main/docs/mcp-clients.md))
- VS Code normalizes the same probe set for its own importer, naming the exact locations it
  reads for Claude Desktop, Copilot CLI, Cursor (global + workspace) and Windsurf, gated by
  `chat.mcp.discovery.enabled` with only `copilot` on by default
  ([VS Code reference](https://code.visualstudio.com/docs/agents/reference/mcp-configuration)).
- Relocatable homes must be honored or detection lies: `CLAUDE_CONFIG_DIR` (Claude Code
  `.claude.json`), `CODEX_HOME` (reported for `~/.codex`), `COPILOT_HOME` (VS Code native
  config), `CLINE_MCP_SETTINGS_PATH` / `CLINE_DIR` (Cline), `XDG_CONFIG_HOME` (Claude
  Desktop Linux, Zed/OpenCode on Linux), `PAPER_HOME` (our own).
- Clients share directories in ways that break naive "one dir = one client" logic: Cline,
  Roo/Kilo-style forks and Augment all live under VS Code's `globalStorage`; Zed, Codex and
  Goose all read `~/.agents/skills`; Antigravity's `.agents/` is per-workspace.

### Inferences

- Detection tiers worth implementing (highest confidence first):
  1. **Explicit `--client=<id>` / `--no-client=<id>`** — always wins, always available.
  2. **Process/binary probe**: `which claude`, `which codex`, `which opencode`, `which gemini`,
     `which hermes`, `which cline`, `which cn`, `which zed`, `which cursor`, `which windsurf`
     (respecting `PATHEXT` on Windows).
  3. **macOS app bundle**: `/Applications/<App>.app`, `~/Applications/<App>.app` — e.g.
     `Claude.app`, `Cursor.app`, `Windsurf.app`, `Zed.app`, `Visual Studio Code.app`,
     `Antigravity.app`.
  4. **Vendor data directory exists** (covers first-run clients that haven't written a config
     yet): `~/.claude`, `~/.codex`, `~/.cursor`, `~/.config/opencode`, `~/.continue`,
     `~/.gemini`, `~/.codeium`, `~/.cline`, `~/.hermes`, `~/.copilot`.
  5. **Config file exists** (the Wolfram/VS Code primitive) — necessary but not sufficient.
- **Never `mkdir -p` a vendor's config directory.** If `~/.continue` does not exist, the user
  does not use Continue; creating it produces a stale, permanently-detected ghost that
  every future install run will "detect". Exceptions where creating the *file* in an existing
  dir is normal: `~/.codex/config.toml`, `~/.cursor/mcp.json`, `~/.cline/…`,
  `~/.hermes/config.yaml`.
- Default posture should be **opt-in per detected client with a printed plan** (`--dry-run`
  is the same code path), because the failure mode of over-registration is not cosmetic: a
  half-configured client shows a red "server failed to start" indicator on every launch, and
  in Cline/Gemini a `trust`/`autoApprove` mistake widens the agent's permission surface.
- Stale detection is unavoidable; mitigate by (a) writing a sidecar record of what we wrote,
  (b) on re-run, verifying our entry still matches the hash we recorded, and (c) offering
  `paper uninstall-client` that removes only entries whose hash still matches.

### Gaps

- No source documents how any of these clients behave when their config file is deleted but
  the app remains installed (recreate-on-launch vs. silent defaults). This matters for
  "register once, user deletes the file, re-run installer" idempotency.
- Whether Antigravity's `.migrated` marker is guaranteed to exist on all 2.x installs, or
  only on upgraded ones, is not documented; Wolfram's heuristic (marker → new path, else old
  path) is the best available but unverified against a fresh 2.x install.

---

## Safe merging: preserving user edits and comments, atomic writes, backups, locking, duplicates, ordering, Windows escaping

### Takeaway

Prefer a **text-preserving, append-only merge** over parse-and-reserialize for every format,
because the highest-risk clients are exactly the ones whose files carry comments (Codex
TOML, OpenCode/Zed JSONC) or user state (Claude Code `~/.claude.json`). Use the vendor CLI
only when three conditions hold: it exists, it is documented as additive, and the target app
is not running. Write atomically via temp-file + rename in the same directory, keep a
timestamped backup, and take an OS-level lock on a sidecar `.lock` file rather than the
config file itself.

### Cited Findings

- **Vendor CLIs are not always safe.** `codex mcp add` / `mcp remove` "rewrite every
  `[mcp_servers.*]` entry, silently dropping comments and unknown keys" — a bug report shows
  inline comments, `enabled = false`, and an unknown forward-compat key all destroyed
  ([openai/codex#45432](https://github.com/openai/codex/issues/45432)).
- **Vendor CLIs sometimes are:** Claude Code documents `add`, `add-json`, `get`, `list`,
  `remove`, and scope flags as first-class, and returns a `File modified:` line naming the
  file it wrote ([Claude Code MCP](https://code.claude.com/docs/en/mcp)).
- **Schema strictness kills marker keys.** Gemini's `MCPServerConfig` sets
  `additionalProperties: false` ([settingsSchema.ts](https://github.com/google-gemini/gemini-cli/blob/ce84b3cb/packages/cli/src/config/settingsSchema.ts)),
  and Codex drops unknown keys on CLI rewrite — so "owner" metadata cannot live inside the
  entry.
- **Continue rejects the whole file** if top-level `name`/`version`/`schema` are missing or
  if the YAML is malformed: it "silently reject[s] the file and fall back[s] to 'Default
  Config' with no MCP servers visible"
  ([Wolfram AgentTools](https://github.com/WolframResearch/AgentTools/blob/main/docs/mcp-clients.md)).
  For YAML this is worse than JSON: a bad edit doesn't error, it silently disables everything.
- **Windows shell-invocation quirk is real for some clients.** Augment Code's CLI
  shell-invokes the command, so a `/abs/path with spaces/paper.exe` fails with
  `'C:\Program' is not recognized…`; the known workaround is the 8.3 short-path form
  (`C:\PROGRA~1\…`) ([Wolfram AgentTools](https://github.com/WolframResearch/AgentTools/blob/main/docs/mcp-clients.md)).
  Most clients `exec` directly and are fine with spaces in `command`.
- **Apps overwrite their own files.** Antigravity "cloud-syncs some settings … If the cloud
  copy doesn't list your server, Antigravity may overwrite your local `mcp_config.json` on
  next launch" ([AgentTools](https://github.com/WolframResearch/AgentTools/blob/main/docs/mcp-clients.md)).
  Claude Desktop and Cline likewise rewrite config on in-app toggles.
- **Duplicate detection semantics differ by client.** Claude Code dedupes by **name** with
  strict scope precedence and "the entire server entry from that source is used; fields are
  not merged". Cursor merges global + project and lets project win. Gemini merges
  system > workspace > user by name. OpenCode: "A higher-precedence project config replaces
  the entire server object with the same name." Continue has no key at all — duplicates are
  separate array elements.

### Inferences — recommended algorithm

```
register(client, entry):
  target  = resolve_path(client)            # env vars first, then per-OS table
  if not detection_ok(client, target): return Skipped(reason)
  if is_running(client): warn_later_restart(client)   # never shell out to a running CLI

  raw = read(target) or None
  if raw is None:
      if not create_allowed(client): return Skipped(no_config)
      doc = skeleton(client)                # only for clients where a bare file is valid
  else:
      backup = write_backup(target, raw)    # <file>.paper-bak-<iso8601>
      doc = parse_preserving(raw)           # CST: taplo/jsonc-parser/rowan-style, not a DOM
      if parse_failed: return Skipped(unparseable, backup)

  existing = lookup(doc, client.server_key, "paper")
  if existing:
      if normalize(existing) == normalize(entry): return NoOp(already_current)
      if not force: return Conflict(diff(existing, entry), backup)
      replace(existing, entry)              # in place, preserving key order
  else:
      insert_at_end(doc, "paper", entry)    # append: minimal diff, no reordering

  if client == Continue: upsert_array_element(name == "paper")
  if client == Codex:    append "[mcp_servers.paper]" + "[mcp_servers.paper.env]" tables at EOF

  validate(entry)                           # command exists, is executable, --version runs
  atomic_write(target, serialize(doc))      # tmp in same dir → fsync → rename → fsync(dir)
  record_state(client, target, sha256(entry))
```

Specific decisions and why:

- **Append at the end, never sort.** For TOML, a new `[mcp_servers.paper]` table appended at
  EOF is valid regardless of what came before, and produces a one-hunk diff. Sorting or
  re-emitting tables risks reordering `[[…]]` arrays and breaking the user's file. For JSON
  objects, append the key last for the same reason.
- **Delete TOML sub-tables too on uninstall.** Removing `[mcp_servers.paper]` but leaving
  `[mcp_servers.paper.env]` yields a file that still "works" but keeps a dangling table; both
  must go (and any `[mcp_servers.paper.*]`).
- **Normalize before comparing, but never mutate the user's entry.** Compare on
  `{basename(command), args, env-subset}` rather than a raw string diff, so a user who changed
  `paper` → `/usr/local/bin/paper` is not treated as "already current" or as a hard conflict;
  report it as a *modification* and leave it alone unless `--force`.
- **Hash-based ownership.** Record `sha256(canonical_entry)` in the sidecar. On uninstall,
  only delete the entry if the current hash matches (user didn't edit it). On re-run, if it
  differs, that is a user edit → skip by default.
- **Atomicity.** Temp file must be in the **same directory** (`rename(2)` is only atomic
  within a filesystem), then `fsync` the temp, `rename`, then `fsync` the directory. Preserve
  mode/ownership; on macOS preserve the file's ACL/xattrs if trivial (`fcopyfile`-style) to
  avoid breaking apps that store extra metadata.
- **Locking.** Take an advisory lock on a sibling `<file>.paper.lock`
  (`flock`/`LOCK_EX` on Unix, `LockFileEx` on Windows) for the read-modify-write window.
  Do not lock the config file itself: on Windows a locked file cannot be renamed over, and
  some apps hold the config open for reading. On Windows, `rename` over an existing file
  fails if any process has it open — retry with backoff and fall back to a clear error rather
  than a truncated write.
- **Running-app detection.** Cheap heuristics: `pgrep -f "/Claude.app"` /
  `tasklist /FI "IMAGENAME eq Claude.exe"`. If running: still merge (files are only read at
  startup by most clients) but warn "restart <client> fully — closing the window is not
  enough" (documented Antigravity/Claude Desktop behavior).
- **Windows path escaping.** Never build JSON by string concatenation. Serialize with a real
  JSON writer so backslashes become `\\` automatically. Prefer `%LOCALAPPDATA%`-based
  per-user install paths (`C:\Users\<u>\AppData\Local\Programs\paper\paper.exe`) over
  `C:\Program Files\…` — no spaces-in-path class of bugs, no admin rights, and no 8.3
  short-path workaround. Include the `.exe` suffix. Do not wrap the command in `cmd /c`
  unless the target is a `.bat`/`.cmd` shim; a native `.exe` should be exec'd directly.
- **Forward slashes on Windows?** JSON allows `C:/Users/…`; most clients pass the string
  straight to `CreateProcess`, which accepts forward slashes for paths in practice but not
  universally. Use backslashes (correct escaping) — it is the least surprising.
- **`cwd` handling.** Only emit `cwd` for clients that document it (OpenCode, VS Code, Gemini,
  Antigravity, Hermes). Emitting unknown keys is fatal in Gemini and dropped in Codex. For
  Paper specifically, `cwd` should be omitted: the server indexes user-configured roots, not
  the current directory.

### Gaps

- No source states whether Claude Desktop rewrites `claude_desktop_config.json` wholesale on
  launch (which would discard comments/formatting even for a valid merge). Empirically it is
  a plain JSON read, but unverified in 2026 builds.
- TOML comment preservation depends on the chosen crate; `toml_edit` preserves formatting and
  comments, whereas `toml` + `serde` does not. No source-level verification of `toml_edit`'s
  handling of unusual constructs (dotted keys, inline tables) was performed here.
- Windows `LockFileEx` + `MoveFileEx(MOVEFILE_REPLACE_EXISTING)` interaction with
  antivirus/OneDrive-synced config dirs (`%APPDATA%` under OneDrive redirection) is a known
  real-world failure class that I found no specific documentation for.

---

## Should we prefer each client's own CLI? Tradeoffs

### Takeaway

Yes, but only for the three clients where the CLI is documented as additive and the file is
app state (Claude Code, Cline, Gemini) and even then only when the app is not running. For
everything else, file merging with a preserving parser is safer, because the CLIs either do
not exist, are not scriptable, or are known to rewrite the whole document.

### Cited Findings

| Client | CLI for stdio registration | Evidence of safety |
|---|---|---|
| Claude Code | `claude mcp add [--scope …] name -- cmd args` | Documented, prints `File modified:`; but writes into a live state file |
| OpenCode | `opencode mcp add name [--global] -- cmd args` | Documented; **no `remove`** — deletion is "delete its entry" |
| Codex | `codex mcp add` | **Destroys comments and unknown keys** ([#45432](https://github.com/openai/codex/issues/45432)) |
| Cline | `cline mcp add name --yes -- cmd args` | Documented scriptable form with `--yes` |
| Gemini CLI | `gemini mcp add -s user -t stdio …` | Documented; note default scope is **project** |
| Hermes | `hermes mcp add … --preset` / `hermes mcp install` | Preset/catalog oriented; arbitrary stdio unverified |
| VS Code | `code --add-mcp '<json>'` | Works, but writes the **deprecated** editor-managed profile |
| Continue | `cn --config <file>` only | No add/remove subcommand |
| Claude Desktop, Cursor, Windsurf, Zed, Antigravity | none | file only |

### Inferences

- CLI wins when: (a) the file is a state blob with internal invariants the CLI maintains
  (`~/.claude.json` holds `projects`, approvals, history — hand-editing risks clobbering a
  concurrent write), (b) the client validates/normalizes on load and needs its own ordering,
  (c) the CLI is idempotent by name.
- CLI loses when: it rewrites the document (Codex), when it targets a deprecated surface
  (`code --add-mcp`), when it has no `remove` (OpenCode), when it is interactive-first
  (Hermes, Cline without `--yes`), or when its version doesn't match the installed app's
  config schema (an `opencode` on PATH may be V2 while the running app is V1).
- **Hybrid is the correct answer:** try the CLI, then verify by re-reading the file and
  confirming (i) our entry exists, (ii) every pre-existing entry still exists, (iii) the
  byte-level comment count did not drop. If verification fails, restore the backup and fall
  back to a file merge. This gives vendor-correct writes with a safety net, and the
  verification step is cheap.
- Never run the CLI while the app is running and never run it with the app's config file open
  in an editor (Claude Code, Claude Desktop and Cline all hold the file).

### Gaps

- None of the CLIs document exit codes, machine-readable output, or idempotency guarantees.
  Only Claude Code's `File modified:` line and `claude mcp get` are documented as verifiable
  signals. Everything else must be verified by re-reading the file.
- Whether `gemini mcp add -s user` preserves unrelated `settings.json` keys (models, theme,
  `mcp.allowed`) is not stated in the docs.

---

## Uninstall / cleanup: removing only our entry

### Takeaway

Uninstall must be keyed on *our recorded entry*, not on the name alone, and must delete the
narrowest possible syntactic unit: one object key in JSON/JSONC, one table plus its
sub-tables in TOML, one array element in Continue, one mapping key in Hermes/YAML. Files we
created may be deleted only if they now contain nothing else; files we merely edited must
always be left in place.

### Cited Findings

- Per-client documented removal paths: Claude Code `claude mcp remove <name> [--scope …]`;
  Gemini `gemini mcp remove`; Cline `cline mcp remove <name>`; OpenCode has **no** remove —
  "delete its entry from the project or global configuration"; Claude Desktop / Cursor /
  Windsurf / Zed / Antigravity have no documented CLI removal (delete the key).
- Prior art for name-keyed uninstall: Wolfram's `UninstallMCPServer["ClaudeDesktop",
  "Wolfram"]` ("Remove specific server") and `UninstallMCPServer[myServerObject]` ("Remove
  from all locations"), with `"MCPServerName"` used as the same key at install and uninstall
  time ([AgentTools](https://github.com/WolframResearch/AgentTools/blob/main/docs/mcp-clients.md)).
- Continue's array shape means removal is by the element's `name` field, not by key
  ([Continue docs](https://docs.continue.dev/customize/deep-dives/mcp)).

### Inferences

- Refuse to delete if the recorded `sha256` no longer matches: the user edited the entry, so
  it is now theirs. Print the diff and require `--force`.
- After removal, do **not** rewrite the file from a DOM (that would reformat and drop
  comments). Apply a surgical text edit to the original bytes using the CST span, then
  re-serialize only the touched node.
- Empty-file policy: delete the file only if (a) we created it, (b) it parses to an empty
  server container, and (c) no other top-level keys exist. Otherwise leave `{"mcpServers": {}}`
  in place — an empty container is harmless and much safer than removing a file the app may
  expect.
- Do not "restore the backup" as part of uninstall. The user may have edited the file after
  install; restoring an old snapshot silently reverts their work. Offer `--restore-backup
  <timestamp>` as an explicit, separate operation.
- Remove the sidecar state entry last, after the config edit succeeds, so a crash mid-way
  leaves us able to find and finish the job.
- Windows: after an uninstall the OS may still hold the old binary path in PATH or in a
  per-user install dir; registration entries pointing at a deleted binary are the most common
  "broken MCP server" report. Uninstall should run *before* binary removal, and a
  `paper doctor` should flag registered paths that no longer exist.

### Gaps

- No documentation on whether Zed or Antigravity tolerate a stale `context_servers` /
  `mcpServers` entry pointing at a missing binary (error indicator vs. crash vs. silent skip).
- Whether VS Code's `chat.mcp.discovery` keeps a cached copy of a discovered server after the
  source entry is deleted is undocumented.

---

## Prior art: how other MCP servers and installers do this, and official distribution guidance

### Takeaway

The ecosystem converged on three mechanisms — (1) **bundle + one-click** (`.mcpb` for Claude
Desktop, Zed extensions, VS Code MCP manifests/store), (2) **deep link with base64 config**
(Cursor, VS Code), and (3) **a multi-client installer library** with a per-OS path registry,
per-client schema converters, and a name-keyed uninstall (Wolfram AgentTools is the most
complete public example). The "just write the JSON" approach is what everyone falls back to,
and it is where all the compatibility bugs live.

### Cited Findings

- **`.mcpb` (Desktop Extensions, formerly DXT):** a bundle installable with one click into
  Claude Desktop, with automatic updates and an optional bundled Node runtime; Anthropic runs
  a reviewed directory, and admins can enable/disable public extensions or upload private
  ones ([modelcontextprotocol/mcpb](https://github.com/modelcontextprotocol/mcpb),
  [Anthropic announcement](https://anthropic.com/research/desktop-extensions),
  [Claude support](https://support.claude.com/en/articles/10949351-getting-started-with-local-mcp-servers-on-claude-desktop),
  [Build a desktop extension](https://claude.com/docs/connectors/building/mcpb)).
  Signing/verification and a `manifest.json` are part of the format.
- **Cursor deep link:** `cursor://anysphere.cursor-deeplink/mcp/install?name=<n>&config=<base64
  JSON>`; the marketplace "Add to Cursor" button uses the same mechanism
  ([Cursor docs](https://cursor.com/docs/mcp),
  [implementation write-up](https://dev.to/eugeniya_ivanova_4a58eadc/we-shipped-a-one-click-add-to-cursor-button-the-hard-part-wasnt-the-button-141m)).
- **VS Code equivalents:** `code --add-mcp '<json>'` (editor-managed user profile),
  **MCP: Install Server from Manifest**, **MCP: Browse MCP Servers**, and a link to the
  GitHub-hosted MCP registry ([MCP configuration reference](https://code.visualstudio.com/docs/agents/reference/mcp-configuration)).
  A `vscode.dev/redirect?url=vscode:mcp/install?…` pattern is used by third parties, but I
  could not verify its exact parameter shape in official docs.
- **Multi-client installer library (closest prior art to what Paper needs):**
  Wolfram's `InstallMCPServer` / `UninstallMCPServer` / `DetectedMCPClients[]` implements a
  client registry (`$supportedClients`) with fields `ConfigFormat`, `ConfigKey`,
  `InstallLocation` (per-OS), `ProjectPath`, `ServerConverter`, and ships adapters for
  **Claude Code, Claude Desktop, Cline, Continue, Codex, Cursor, Gemini CLI, OpenCode,
  VS Code, Windsurf, Zed and Antigravity** — i.e. almost exactly our client list. Notable
  details: per-client converters add `disabled: false` + `autoApprove: []` for Cline and
  `tools: ["*"]` for Copilot CLI; OpenCode's command/args are joined into one array;
  Antigravity gets a `.migrated`-marker-aware path choice; Augment gets an 8.3 short-path
  rewrite on Windows.
  ([AgentTools mcp-clients.md](https://github.com/WolframResearch/AgentTools/blob/main/docs/mcp-clients.md))
- **Vendor curation is now normal:** Claude Desktop has a reviewed extension directory, Zed
  has an extension registry filtered by `context-servers`, VS Code has an MCP gallery/store
  and manifest install, Cursor has a marketplace. For a general-purpose local tool like
  Paper, the realistic guidance is "publish an `.mcpb` for Claude Desktop and a deeplink for
  Cursor/VS Code, and install configs directly for the rest".
- **Anthropic's official local-server guidance** still documents the JSON path
  (`Settings → Developer → Edit Config`) as the manual route alongside extensions
  ([Claude support](https://support.claude.com/en/articles/10949351-getting-started-with-local-mcp-servers-on-claude-desktop)).
- **MCP itself has no install/registration spec.** The transport spec only defines the
  stdio framing (newline-delimited JSON-RPC, stdout reserved, stderr for logs) —
  any "how do I register" convention is per-client.

### Inferences

- A `paper mcp install` should ship a **client registry table in the binary** (like
  `$supportedClients`) rather than hard-coding paths in the installer script, so a new
  client is a data change and `paper mcp detect` / `paper mcp status` can share it.
- The `.mcpb` route is worth doing *in addition to* config writing for Claude Desktop: it is
  the only channel that survives the extensions-first UI and gives auto-update, and it
  sidesteps the "user must restart Claude Desktop" step. It does not help any other client.
- Bundling is a liability for a single-binary Rust tool: `.mcpb` bundles are expected to
  carry their runtime, whereas Paper just needs `command: /abs/paper`. The cheap version is
  a `.mcpb` whose manifest points at an absolute path and whose install step verifies the
  binary exists — legitimate, but it forfeits the "no dependencies to manage" benefit that
  Anthropic advertises.
- Deep links are attractive for docs ("Add to Cursor") but are **per-client and
  unauthenticated**: anyone can craft one, so a product should treat them as a convenience
  affordance, never as the only path, and should not assume the receiving client validates
  the payload.

### Gaps

- I found no official statement of `.mcpb` **signing** requirements or whether unsigned
  side-loaded `.mcpb` files will still install in late-2026 Claude Desktop builds.
- The exact VS Code deeplink format (`vscode:mcp/install?…` vs `vscode.dev/redirect?…`) is
  used widely but not documented in the VS Code MCP reference I read; treat as unverified.
- No public source describes an official "MCP server installer" API/SDK (analogous to
  `InstallMCPServer`) from Anthropic, Microsoft, or the MCP project; the closest thing is the
  registry + per-client manifests.
- `mcp-installer` (the community npm package that registers servers into Claude Desktop) was
  not retrievable in this session; its behavior is therefore not cited here.

---

## Cross-cutting risk register (clients where writing config directly is unsupported or risky)

### Takeaway

Direct file writes are *supported* for roughly half the list and merely *undocumented* for
the rest — but four clients carry real, sourced risk: Antigravity (cloud sync + PATH +
launch caching), Claude Code (live state file), VS Code (writing a deprecated surface by
default), and Continue (silent whole-file rejection on malformed YAML). Codex deserves a
special mention because the *vendor CLI* is the risky path there, not the file.

### Cited Findings / Inferences

| Client | Direct write | Risk | Mitigation |
|---|---|---|---|
| Claude Code | Supported but discouraged ("You don't need to edit these files directly") | `~/.claude.json` is a large live state file | Prefer `claude mcp add --scope user`; never write while `claude` runs |
| Claude Desktop | Supported (Settings → Developer → Edit Config) | Extensions/admin policy may override; app restart required | Also ship `.mcpb`; warn "quit fully, not just close the window" |
| OpenCode | Supported (`opencode.json` is the documented config) | V1 vs V2 key path; JSONC comments | Detect version; preserve comments; append only |
| Codex | Supported | **Vendor CLI destroys comments**; 10s startup default | Append TOML text; set `startup_timeout_sec`; do not use `codex mcp add` |
| Cursor | Undocumented but universal in practice | Global file reported unreliable; Windows path-open bug | Write both global and project on request; verify by re-read |
| Windsurf | Undocumented | Path ambiguity + Devin/Cortex migration | Write only an existing path; never create `~/.config/devin` |
| VS Code | Supported | `.vscode/mcp.json` deprecated for the Copilot harness (removal Feb 2027); two schemas | Write native `mcpServers` by default; `.vscode/mcp.json` only on explicit request |
| Zed | Undocumented but documented-in-effect (Settings UI writes it) | JSONC; two competing schemas across vendor guides | Flat schema only; preserve comments |
| Cline | Supported (docs point at the file) | Extension vs CLI paths diverge; `autoApprove` footgun | Offer both paths; write `autoApprove: []` |
| Continue | Supported | YAML array; missing top-level metadata silently disables **all** MCP | Never rewrite the doc; insert only; validate after write |
| Gemini CLI | Supported | Consumer deprecation → Antigravity; `additionalProperties: false` | No marker keys; register only if `~/.gemini` exists |
| Hermes | Supported (it's the documented way, alongside catalog) | Only presets/catalog documented for CLI; YAML | Append mapping key; validate by parse |
| Antigravity | Undocumented (UI is the documented path) | Cloud sync overwrite, macOS PATH, launch-time caching, Windows installer dir | Absolute command path; full restart; verify after launch; idempotent re-run |

### Gaps

- No documentation confirms whether Cursor or Zed *officially* support third-party writes to
  their config files (as opposed to tolerating them). Both UI-write the same file, which is
  strong circumstantial evidence.
