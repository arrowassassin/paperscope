# Distribution-Channel Hooks: cargo-dist installers and Claude Code plugin/MCP mechanics

Scope note: everything below was fetched on 2026-11-22. "Verified" means the statement appears in official docs, source, or a GitHub issue/PR that I read directly. "Inferred" means I reasoned from those facts. No benchmark or third-party blog is used as a primary source.

## Q1 — cargo-dist / dist installers (2026): post-install hooks, stable paths, signing

### Takeaway

There is **no post-install hook, no custom-installer escape hatch, and no arbitrary install-time script** in `dist` (cargo-dist) as of the 0.34.x line. The only install-time knob is `install-success-msg`, a static string printed on success by the shell and PowerShell installers. Script installers do place the binary at a stable, configured path (default `CARGO_HOME` → `$CARGO_HOME/bin` or `~/.cargo/bin`), and they deliberately discard every archive file except the binary. Windows code signing is a first-class, documented feature; macOS code signing exists but is experimental, workspace-only, and effectively undocumented, and there is no notarization step in the generated workflow.

### Cited Findings

**No custom install logic — verified, stated twice in the book**

- The shell installer's documented caveats include "Cannot run any kind of custom install logic" — [cargo-dist book, shell installer](https://axodotdev.github.io/cargo-dist/book/installers/shell.html)
- The PowerShell installer page carries the identical caveat — [cargo-dist book, powershell installer](https://axodotdev.github.io/cargo-dist/book/installers/powershell.html)
- The installer list is closed: "Currently supported installers include: shell, powershell, npm, homebrew, msi", and the config reference enumerates exactly those five values for `installers` — [installers index](https://axodotdev.github.io/cargo-dist/book/installers/index.html), [config reference `installers`](https://axodotdev.github.io/cargo-dist/book/reference/config.html#installers)
- The generated installer templates in the source tree are only `installer.sh.j2`, `installer.ps1.j2`, `homebrew.rb.j2`, and an `npm/` directory plus `package.json`; there is no plugin/hook/custom-script mechanism in the tree listing — [cargo-dist repo `cargo-dist/templates/installer/`](https://github.com/axodotdev/cargo-dist/tree/main/cargo-dist/templates/installer)

**What does exist: one static success message**

- `install-success-msg` is a documented key under "shell and powershell installer settings": "The message to display on success in the shell and powershell installers" — [config reference `install-success-msg`](https://axodotdev.github.io/cargo-dist/book/reference/config.html#install-success-msg)
- It was added by PR "feat(installer): custom success msg" — [cargo-dist PR #1102](https://github.com/axodotdev/cargo-dist/pull/1102)
- Richer output (branded hello, success, failure) is still an **open** request: "allow user to customize the installer terminal output … it'd be cool to add: branded hello, success, failure" — [cargo-dist issue #314](https://github.com/axodotdev/cargo-dist/issues/314) (open, filed by a maintainer)
- The same issue cites a hand-rolled installer as the pattern people use instead: `tursodatabase/turso-install/blob/main/install.sh`, i.e. projects bypass dist's installer when they need custom behaviour — [cargo-dist issue #314](https://github.com/axodotdev/cargo-dist/issues/314)

**Install path and its stability**

- `install-path` default is `"CARGO_HOME"`: "installs as if `cargo install` did it (tries `$CARGO_HOME/bin/`, but if `$CARGO_HOME` isn't set uses `$HOME/.cargo/bin/`)" — [config reference `install-path`](https://axodotdev.github.io/cargo-dist/book/reference/config.html#install-path)
- Three syntaxes: `"CARGO_HOME"`, `"~/subdir/"`, `"$VAR/subdir"`; since 0.14.0 an array forms a cascade (`install-path = ["$MY_APP_HOME/bin", "~/.my-app/bin"]`) — [config reference `install-path`](https://axodotdev.github.io/cargo-dist/book/reference/config.html#install-path)
- The path is a *config* value resolved at install time, not a per-version path: re-running a newer installer with unchanged config writes to the same directory. `CARGO_HOME` is a special case with "a slightly different install dir structure from the others" and cannot be mixed into a cascade — [config reference `install-path`](https://axodotdev.github.io/cargo-dist/book/reference/config.html#install-path), [cargo-dist issue #934](https://github.com/axodotdev/cargo-dist/issues/934)
- PowerShell uses the same `install-path` setting; it only differs in PATH mechanics: it reads/writes `HKCU:\Environment` "Path" as `REG_EXPAND_SZ`, prepends the dir, and broadcasts `WM_SETTINGCHANGE` — [cargo-dist book, powershell installer](https://axodotdev.github.io/cargo-dist/book/installers/powershell.html)
- The shell installer writes `$install-path/env`, appends `. $env-path` to `$HOME/.profile` only (deliberately not `.zshrc`/`.bashrc`), and prompts the user to `source` it or restart the shell — [cargo-dist book, shell installer](https://axodotdev.github.io/cargo-dist/book/installers/shell.html)
- Important for anything asset-bearing: both script installers "throw out all files except for the binary, so the binary can't rely on assets included in the archive" — linked from [shell](https://axodotdev.github.io/cargo-dist/book/installers/shell.html) and [powershell](https://axodotdev.github.io/cargo-dist/book/installers/powershell.html) caveats to [cargo-dist issue #307](https://github.com/axodotdev/cargo-dist/issues/307)
- `install-updater = true` additionally installs `yourpackage-update`, a standalone self-updater, for shell/PowerShell installs — [config reference `install-updater`](https://axodotdev.github.io/cargo-dist/book/reference/config.html#install-updater)

**Signing / notarization**

- Documented supply-chain signing is Windows-only: the book's signing index lists "Windows Codesigning" as the only available page and marks "🔜 macOS Codesigning" with a link to issue #1121 — [cargo-dist book, supply-chain security](https://axodotdev.github.io/cargo-dist/book/supplychain-security/index.html)
- The config reference has exactly two signing keys, both Windows: `ssldotcom-windows-sign` and `azure-windows-sign`; no macOS or notarization keys appear anywhere in the reference — [config reference](https://axodotdev.github.io/cargo-dist/book/reference/config.html)
- macOS code signing *does* exist in the codebase as an experimental, workspace-only option named `macos-sign` (`pub macos_sign: Option<bool>`), with a warning if set per-package: "package.metadata.dist.macos-sign is set, but this is only accepted in workspace.metadata (value is being ignored)" — [cargo-dist `cargo-dist/src/config/v0.rs`](https://github.com/axodotdev/cargo-dist/blob/main/cargo-dist/src/config/v0.rs)
- The generated GitHub workflow wires it to three secrets, and nothing else: `CODESIGN_CERTIFICATE`, `CODESIGN_CERTIFICATE_PASSWORD`, `CODESIGN_IDENTITY`, gated on `{% if macos_sign %}`; the template contains no `notarytool`/`notarize` invocation — [release.yml.j2](https://github.com/axodotdev/cargo-dist/blob/main/cargo-dist/templates/ci/github/release.yml.j2)
- `macos-sign` was implemented by "feat: macOS codesigning" — [PR #1361](https://github.com/axodotdev/cargo-dist/pull/1361) (merged 2024-08-27; closes [issue #1121](https://github.com/axodotdev/cargo-dist/issues/1121), whose `state_reason` is `completed`)
- It accepts a `--options` value through the `CODESIGN_OPTIONS` environment variable ("When using the experimental macOS codesigning feature…") — [PR #1743](https://github.com/axodotdev/cargo-dist/pull/1743)
- 0.30.x included "fix: macOS codesigning configuration not being respected" — [PR #2214](https://github.com/axodotdev/cargo-dist/pull/2214)
- Contradiction to flag: the book still lists macOS codesigning as *not yet available* (🔜, pointing at a closed-as-completed issue) and the config reference omits `macos-sign` entirely, while the source and changelog say it shipped in 2024. The book page is stale, or the feature is intentionally undocumented.
- Latest release line visible on the releases page is 0.34.0 (followed by 0.33.1, 0.33.0, 0.32.0) — [cargo-dist releases](https://github.com/axodotdev/cargo-dist/releases)

### Inferences

- Because no installer hook exists, a `paper setup` step cannot be triggered by dist itself. The three viable patterns, in descending order of how well they match dist's design: (1) `install-success-msg` prints the exact `paper setup` command and the binary self-initialises/validates state on first run; (2) ship a separate `install.sh`/`install.ps1` wrapper that curls dist's installer and then runs `paper setup` (this is what issue #314's example project does); (3) rely on the `install-updater`-style "binary owns its lifecycle" model and never install-time anything. Only (2) can guarantee setup ran, and only at the cost of abandoning dist's installer as the entry point.
- The "installer throws out everything except the binary" behaviour (#307) means a model-bearing archive cannot be delivered through the shell installer at all — which is consistent with fetching models at first run instead.
- A stable install path is achievable but is a *config promise*, not an installer guarantee: if `install-path` is later changed, or if users install via npm (`npx`, per-version package cache) or Homebrew (Cellar version dirs), the resolved binary path differs. Any TCC-sensitive design should therefore not assume "same path forever" across channels.
- `macos-sign` being workspace-only and secret-driven means it is usable in CI today, but its absence from the config reference makes it a de-facto unsupported surface; and because there is no notarization step, signing alone will not satisfy Gatekeeper on first download of a quarantined binary (stapling/notary is the part that is missing).

### Gaps

- No official cargo-dist documentation of a post-install hook exists, and I found no issue explicitly titled "post-install hook" — the closest artefacts are #314 (custom installer output, open) and #379 ("release lifecycle plugins", closed; I did not read its resolution, so I make no claim about it).
- I could not verify whether `macos-sign` is honoured for *all* archive types (tarball only vs zip) or whether it signs the `.app`/binary only; the config reference does not document it and I did not read the signing implementation.
- I did not find a project that documents an official "run setup after install" workaround in dist's docs; the wrapper-script pattern is evidenced only by the example linked in issue #314, not by policy text.

## Q2 — Claude Code plugins, MCP registration, MCP Registry and .mcpb install flow

### Takeaway

A Claude Code plugin is a directory (manifest optional) whose components include skills, agents, hooks, MCP servers, LSP servers, monitors and a `bin/` on PATH; it is cached at `~/.claude/plugins/cache/<marketplace>/<plugin>/<version>/` and its MCP servers are registered **automatically** when the plugin is enabled — no `claude mcp add`. But there is **no install-time command**: the only install-time interaction is a `userConfig` dialog, plugin lifecycle hooks were requested and closed as not planned, and Claude Code's only automatic install step (npm dependency install) explicitly runs with `--ignore-scripts` so no plugin code executes. The MCP Registry publishes metadata only, and `.mcpb`/DXT bundles are declarative: the *client* generates the MCP config; the bundle cannot run an install command.

### Cited Findings

**What a plugin is and where it lives**

- "A Claude Code plugin is a directory of skills, agents, hooks, MCP servers, or other components that Claude Code installs and loads as one unit"; the manifest is `.claude-plugin/plugin.json` and is optional — [Plugins overview](https://code.claude.com/docs/en/plugins)
- Standard layout: `.claude-plugin/plugin.json`, `skills/`, `commands/`, `agents/`, `hooks/hooks.json`, `.mcp.json`, `.lsp.json`, `bin/`, `monitors/`, `themes/`, `output-styles/`, `workflows/` — [manifest reference, Standard layout](https://code.claude.com/docs/en/plugins/manifest-reference#standard-layout)
- On-disk: plugins root is `~/.claude/plugins` (override `CLAUDE_CODE_PLUGIN_CACHE_DIR`); `cache/<marketplace>/<plugin>/<version>/` holds one directory per installed version and is what `${CLAUDE_PLUGIN_ROOT}` points at; `installed_plugins.json` records `scope`, `installPath`, `version`; `data/<plugin-id>/` is the persistent `${CLAUDE_PLUGIN_DATA}` — [Plugin loading reference, Find plugins on disk](https://code.claude.com/docs/en/plugins/loading#find-plugins-on-disk)
- "Because `${CLAUDE_PLUGIN_ROOT}` points at a version directory, a plugin's root path changes with every version. Keep a plugin's durable files in `${CLAUDE_PLUGIN_DATA}` instead." — [Plugin loading reference](https://code.claude.com/docs/en/plugins/loading#find-plugins-on-disk)
- Marketplace plugins are **copied** into the cache at install; files outside the plugin directory are not copied, and component paths that escape the plugin root (including via symlink, or containing a backslash on macOS/Linux) are rejected — [Plugin loading reference, In-place and copied plugins / Paths that escape](https://code.claude.com/docs/en/plugins/loading#in-place-and-copied-plugins)

**MCP servers in plugins: declarative and auto-registered**

- Declare in `.mcp.json` at the plugin root, in the same shape as a project `.mcp.json`; on load it appears in `/mcp` as `plugin:<plugin>:<server>` and tools are named `mcp__plugin_<plugin>_<server>__<tool>` — [Plugin components, MCP servers](https://code.claude.com/docs/en/plugins/components#mcp-servers)
- The `mcpServers` manifest key accepts an inline map, a `.json` file path, a `.mcpb`/`.dxt` bundle path, or an `https://` bundle URL; `.mcp.json` loads first and later declarations replace same-named servers — [manifest reference, `mcpServers`](https://code.claude.com/docs/en/plugins/manifest-reference#mcpservers)
- Server processes receive `CLAUDE_PLUGIN_ROOT`, `CLAUDE_PLUGIN_DATA`, `CLAUDE_PROJECT_DIR` in their environment; `${...}` is substituted in `command`, `args`, `env` — [manifest reference, Environment variables](https://code.claude.com/docs/en/plugins/manifest-reference#environment-variables)
- For project-scope plugins (checked into a repo), plugin-declared MCP servers "go through the same per-server approval as a project `.mcp.json`", and servers declared as `.mcpb`/`.dxt` bundles or from a file outside the plugin directory "are skipped" — [Plugin loading reference, Trust check and component limits](https://code.claude.com/docs/en/plugins/loading#trust-check-and-component-limits-for-repository-plugins)
- A local stdio server runs in Claude Code and in a local Cowork session, "but not on claude.ai" — remote `https://` servers are offered as connectors there instead — [Plugin components, MCP servers](https://code.claude.com/docs/en/plugins/components#mcp-servers)

**No install-time command — verified from three angles**

1. The manifest has no install/post-install field; its fields are metadata, component paths, `userConfig`, `channels`, `dependencies`, `engines`, `network` — [manifest reference, Fields](https://code.claude.com/docs/en/plugins/manifest-reference#fields)
2. The only install-time UI is configuration: "The dialog … opens for any option that isn't set yet" when the user installs or enables the plugin; `claude plugin install` "never prompts for `userConfig` values" and instead takes `--config KEY=VALUE` — [Plugin components, Ask the user for configuration values](https://code.claude.com/docs/en/plugins/components#ask-the-user-for-configuration-values)
3. The one automatic install step is dependency installation, and it is explicitly sandboxed against code execution: registry packages only, exact lockfile pins, `--ignore-scripts` so "`preinstall`, `install`, and `postinstall` scripts" never run, no overrides/patches, separate install folder, 60-second timeout — [Plugin loading reference, Limits on the dependency install](https://code.claude.com/docs/en/plugins/loading#limits-on-the-dependency-install)
- Earlier this behaviour was undocumented and reported as a surprise ("plugin installation silently runs `bun install`/`npm ci`") — [anthropics/claude-code issue #84939](https://github.com/anthropics/claude-code/issues/84939) (closed)
- Plugin **lifecycle** hooks (`PluginInstall`, `PluginUpdate`, `PluginUninstall`) were requested and closed `not_planned` on 2026-06-25 ("Closing for now — inactive for too long"), with a later comment arguing the need was still live — [anthropics/claude-code issue #48986](https://github.com/anthropics/claude-code/issues/48986)
- The documented substitute for a setup step is a `SessionStart` hook that installs dependencies into `${CLAUDE_PLUGIN_DATA}` on first run and again after `package.json` changes — [Plugin components, Install dependencies into the data directory](https://code.claude.com/docs/en/plugins/components#install-dependencies-into-the-data-directory)
- Shipping a binary is supported, but only as a PATH entry while enabled: files in `bin/` are on the Bash tool's `PATH`, and "Plugin `bin/` directories come after the user's own `PATH` entries, so a plugin can't shadow `git`, `ls`, or another system command" — [Plugin components, Executables](https://code.claude.com/docs/en/plugins/components#executables)
- Plugins that ship such a binary do not travel everywhere: "claude.ai and Cowork don't install a plugin that has a top-level `bin/` directory" — [manifest reference, Standard layout](https://code.claude.com/docs/en/plugins/manifest-reference#standard-layout)

**.mcpb / DXT (Desktop Extensions)**

- Bundle manifest is `manifest.json` (spec `manifest_version` 0.3, uv type at 0.4) with `server.type` ∈ `node`/`python`/`binary`/`uv`, `server.entry_point`, and `server.mcp_config` — [MCPB MANIFEST.md](https://github.com/modelcontextprotocol/mcpb/blob/main/MANIFEST.md)
- Registration is client-side and automatic: "When installing the extension, apps will automatically generate the appropriate MCP server configuration and add it to the user's settings, eliminating the need for manual JSON editing" — [MCPB MANIFEST.md](https://github.com/modelcontextprotocol/mcpb/blob/main/MANIFEST.md)
- The spec's install-time surface is declarative only: `user_config` (string/number/boolean/directory/file, `sensitive`, `default`, `multiple`, `min`/`max`) plus `${user_config.KEY}` substitution into `mcp_config`; there is no install/post-install hook field anywhere in the schema — [MCPB MANIFEST.md](https://github.com/modelcontextprotocol/mcpb/blob/main/MANIFEST.md)
- Path rewriting is explicit and `${__dirname}`-based: `${__dirname}`, `${HOME}`, `${DESKTOP}`, `${DOCUMENTS}`, `${DOWNLOADS}`, `${pathSeparator}`; `platform_overrides` gives per-OS `command`/`args`/`env`, and "for binaries, apps will automatically append `.exe` on Windows" — [MCPB MANIFEST.md](https://github.com/modelcontextprotocol/mcpb/blob/main/MANIFEST.md)
- In Claude Code, a declared bundle is "extracted into `mcpb/` under the plugins root"; a bundle's own `user_config` prompts at install (`--config <server>.<key>=<value>`, v2.1.285+), and an unset required setting yields `Bundled MCP server "<name>" was not started: it needs configuration` — [Plugin components, Include a packaged MCPB server](https://code.claude.com/docs/en/plugins/components#include-a-packaged-mcpb-server), [Plugin loading reference, Find plugins on disk](https://code.claude.com/docs/en/plugins/loading#find-plugins-on-disk)
- A bundle path or URL must end in `.mcpb` or `.dxt`; any other extension fails validation — [manifest reference, `mcpServers`](https://code.claude.com/docs/en/plugins/manifest-reference#mcpservers)

**MCP Registry**

- The Registry is a catalogue: "provides MCP clients with a list of MCP servers, like an app store"; publishing happens from the server's own project with `mcp-publisher` (`init`, `login`, `publish`, `status`, `validate`), and "will not publish or list your server" via a PR to the registry repo — [modelcontextprotocol/registry README](https://github.com/modelcontextprotocol/registry)
- "The MCP Registry only hosts metadata, not artifacts" — the package must already be on npm/PyPI/NuGet/OCI/MCPB — [Registry quickstart](https://github.com/modelcontextprotocol/registry/blob/main/docs/modelcontextprotocol-io/quickstart.mdx)
- `server.json` carries `packages[]` (`registryType`, `identifier`, `version`, `transport`, `environmentVariables`) or `remotes[]` (`streamable-http` URL); there is no install-command/hook field, so a registry entry cannot execute anything at install — [Registry quickstart](https://github.com/modelcontextprotocol/registry/blob/main/docs/modelcontextprotocol-io/quickstart.mdx)
- Ownership verification is a marker inside the package (`mcpName` in `package.json`, or `mcp-name:` in a PyPI/NuGet README) and the registry additionally rejects versions listed as malware by OSV.dev — [Registry quickstart](https://github.com/modelcontextprotocol/registry/blob/main/docs/modelcontextprotocol-io/quickstart.mdx)
- Registry status: preview launched 2025-09-08; API freeze (v0.1) announced 2025-10-24, with v1/GA still pending — [modelcontextprotocol/registry README](https://github.com/modelcontextprotocol/registry)

**Documented constraints / sandboxing**

- Plugin components are rejected if they escape the plugin root; symlinks out, `..`, and (on macOS/Linux) any backslash in a path all fail — [Plugin loading reference, Paths that escape](https://code.claude.com/docs/en/plugins/loading#paths-that-escape-the-plugin-directory)
- Plugins load at session start "from `installed_plugins.json` and the cache **without using the network**"; marketplace refreshes happen afterwards in the background — [Plugin loading reference](https://code.claude.com/docs/en/plugins/loading#plugins-and-marketplaces-that-arent-on-disk-at-session-start)
- Hooks, monitors and MCP servers run as the user, not in the sandbox: a monitor "runs with your full user permissions and outside the sandbox"; a sandboxed PowerShell command does not see plugin `bin/` on PATH (an unsandboxed one does) — [Plugin components, Monitors](https://code.claude.com/docs/en/plugins/components#monitors), [Plugin components, Executables](https://code.claude.com/docs/en/plugins/components#executables)
- Windows path handling: most substituted paths use forward slashes in Claude Code (`C:/Users/...`) but keep backslashes in exec-form hooks and PowerShell hooks — [manifest reference, Quoting and path separators](https://code.claude.com/docs/en/plugins/manifest-reference#quoting-and-path-separators)

### Inferences

- A Claude Code plugin is a **distribution wrapper**, not an installer: it can ship a binary in `bin/` and declare it as an MCP stdio server with `${CLAUDE_PLUGIN_ROOT}` in `command`, and enabling the plugin registers that server automatically. It cannot run `paper setup`, cannot register a launchd/LaunchAgent service, and cannot prompt for anything beyond `userConfig` values.
- The only plugin-native place a setup step can live is a `SessionStart` hook keyed on "marker file missing in `${CLAUDE_PLUGIN_DATA}`" — which is the documented dependency-install pattern generalised. That runs at *session* start, i.e. after TCC-relevant GUI context may already be established, and it runs once per session rather than once per install unless guarded.
- For TCC, `bin/` is the wrong home for the binary anyway: the plugin's cache path changes per version and is a copy, so the executable's identity (`cdhash`/path) changes on every update. Registering the plugin's bundled binary as the server would re-trigger TCC prompts on each plugin update, whereas a fixed `~/.cargo/bin/paper` (or `~/.local/bin/paper`) registered by `paper setup` would not.
- `.mcpb`/DXT and the MCP Registry both shift configuration to the client but neither executes anything at install; they are metadata-and-config formats, so they can replace "paste this JSON into claude_desktop_config.json" but not "run this setup step".
- Because the Registry hosts no artifacts, a Paper entry there can only point at npm/PyPI/OCI/MCPB or a remote URL — meaning the Registry is a discovery layer over an install channel that must already exist, and cannot itself carry the signed macOS binary.

### Gaps

- I did not verify whether Claude Code's `userConfig` dialog can be triggered *before* the plugin's MCP server is first spawned in every surface (docs describe install/enable in `/plugin`, the VS Code Manage-plugins form, and `--config` in the CLI, but not the desktop app's local sessions).
- No official statement was found on whether a plugin-declared stdio MCP server inherits the terminal's TCC identity or a new one; the docs only say servers "run alongside each session" as the user. This is the crux for the durable-grant argument and remains unverified here.
- I did not read the `.mcpb` CLI (`mcpb pack`/`mcpb validate`) documentation, so I cannot state whether the bundler enforces platform-specific binaries or notarization for `server.type: "binary"`.
- The Registry quickstart mentions MCPB as a `registryType` for package publishing, but I did not verify how a client maps a registry MCPB entry to a local install, or whether that path preserves bundle `user_config`.
- Issue #48986's closure was `not_planned` due to staleness, not an explicit product decision; a newer duplicate may exist that I did not find.
