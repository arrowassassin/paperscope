# Cross-platform setup, discovery and integration — design

Date: 2026-11-22
Status: proposed (awaiting approval)
Research: `reports/Cross platform install and MCP registration.md` and
`research_notes/Cross platform install and MCP registration/`

## Intent

Paper currently behaves differently on each platform and does its setup at the
wrong time: the daemon exists only on macOS and Linux, it guesses its roots by
joining `$HOME` with English folder names, it re-prompts for macOS permissions
forever because an ad-hoc binary cannot hold a TCC grant, and it downloads
models lazily on the first ordinary command while an editor waits. The goal is
one product: the same install experience, the same discovered roots, the same
background indexing and the same MCP registration on Windows, Linux and macOS,
with provisioning starting at install time rather than at first use.

## Goals

1. `paper setup` runs at install time on every channel that permits it, and is
   safe to run at any time afterwards.
2. One background indexer per user session on all three platforms, driven by
   launchd, systemd `--user` or a logon Scheduled Task, with an identical
   control protocol.
3. Roots discovered from the OS's own answer (Known Folders, XDG user dirs,
   `NSFileManager`), then ranked and filtered; volume roots are opt-in.
4. Model download starts immediately, embedder first; indexing starts as soon
   as the embedder is ready; the reranker downloads in parallel.
5. Paper registers itself in every installed agent client, idempotently, and
   removes exactly its own entry on uninstall.
6. Removals stick. Today `remove-root` is undone on the next daemon restart
   because discovery is unioned back in.

## Non-goals

- A GUI. The surface stays CLI + MCP.
- Running as root, as a system service, or registering anything that needs
  elevation on Windows.
- Indexing network shares, removable media or cloud-only placeholders by
  default; they are candidates, not defaults.
- Changing the retrieval design. Tier 0/1 semantics, RRF and the extractive
  answer contract stay as they are.
- Multi-user or shared-machine deployments.

## Anti-goals

- A first command that blocks on hundreds of megabytes of network I/O.
- A permission dialog that returns every five minutes.
- Silent modification of a user's editor configuration beyond one added entry.
- Scanning a whole volume because it scored non-zero.

## Constraints established by research

- Registration work is milliseconds and idempotent; provisioning is hundreds of
  megabytes and must be owned by exactly one long-lived process. Crossing these
  two lifecycles is the source of nearly every failure in the sources.
- `cargo-dist` installers cannot run custom logic at all (documented twice);
  `cargo install` has no hook; winget forbids script installers; Homebrew's
  `post_install_steps` may not run arbitrary code. Claude plugins and the MCP
  Registry are declarative and have no install hook (the lifecycle-hook request
  was closed `not_planned`).
- TCC stores a serialised code requirement, not a name. Ad-hoc signing yields
  `designated => cdhash`, so every rebuild is a new identity. A versioned
  install path breaks grants even for correctly signed bundles.
- Windows services need elevation and run in session 0; per-user parity comes
  from a logon Scheduled Task with an interactive token.
- The default Windows named-pipe ACL grants Everyone read access; an explicit
  owner+SYSTEM DACL and a client-SID check are both required.
- `hf-hub` does not send `Range` and does not hash what it writes.

## Approaches considered

**A. Fix discovery only.** Replace `home.join(...)` with `dirs::*`, add a
denylist. Cheap, but leaves the prompt loop, no Windows daemon, and no install
-time provisioning. Rejected: solves the symptom the user reported, not the
cause.

**B. Discovery plus discover-and-confirm.** A, plus candidate ranking, an
`auto_discover` switch, per-root denial state and backoff. Fixes the loop and
the revert bug, keeps Windows CLI-only. Rejected only because the user asked for
identical behaviour across platforms.

**C. Chosen.** B, plus a real Windows supervisor and IPC, plus install-time
provisioning and MCP registration. Largest change, and the only one that matches
"the way the library works the same across Windows, Linux and macOS".

## Design

### 1. Two lifecycles, one binary

`paper setup` is registration only: create directories, write and load the
supervisor unit, merge MCP configs, write the setup journal. Target under
150 ms, no network, no model work, safe to run concurrently.

The daemon owns all provisioning: model download, first index, watching. It is
started by `setup` and supervised thereafter.

```
paper setup [--detach|--foreground] [--no-service] [--no-models]
            [--clients=a,b] [--status] [--json] [--force]
paper daemon-control {install,uninstall,start,stop,restart,status}
paper doctor [--tcc] [--json]
```

`setup --detach` replaces the current `bootstrap::maybe_autostart` path. It
writes the supervisor and returns; the daemon does everything else.

**Journal.** `$PAPER_HOME/setup/journal.json`, one atomic record per phase with
`{schema, binary_version, completed_at, artefact_digest}`:

| Phase | Re-run on upgrade | Keyed by |
|---|---|---|
| `dirs` | if layout version changed | schema |
| `service` | always (the executable path may have moved) | binary_version |
| `start` | always | binary_version |
| `integrations` | if MCP schema or binary path changed | binary_version + client schema |
| `models` | never | `(name, sha256)` |
| `index` | never | db |

A single "done" flag cannot express this: a binary upgrade must re-verify the
unit but must not invalidate models. Model artefacts are keyed by
`(name, sha256)`, never by binary version — the current model directory scheme
would re-download 1.13 GB on every release.

**Locks.** Replace the `create_new` + 15-minute stale-takeover lock with a
kernel advisory lock (`fs4`: `flock` on Unix, `LockFileEx` on Windows), held out
of band of the journal. Kernel locks release on process death, so the staleness
heuristic disappears. `paper serve` never takes the lock: it `try_lock`s, logs
one line, and answers keyword-only immediately. `serve` joins `SKIP_COMMANDS`.

**Concurrency target:** five concurrent `paper serve` plus one `setup
--foreground` must produce exactly one provisioning run and five sub-100 ms
serves. This is the first test to write.

### 2. Three supervisors, one contract

Parity target is per login, not per boot.

| | macOS | Linux | Windows |
|---|---|---|---|
| Artefact | `~/Library/LaunchAgents/tools.kintsugi.paper.plist` | `~/.config/systemd/user/paper.service` | Scheduled Task `Paper` (XML) |
| Trigger | `RunAtLoad` | `WantedBy=default.target` | `LogonTrigger`, interactive token |
| Restart | `KeepAlive{SuccessfulExit:false}` | `Restart=on-failure` | `RestartOnFailure` PT1M ×3 |
| Singleton | socket bind | socket bind | `IgnoreNew` + `first_pipe_instance` |
| Status | `launchctl print` | `systemctl --user status` | `schtasks /query /tn Paper /xml` |
| Stop | IPC then `bootout` | IPC then `systemctl stop` | IPC then `/end` |
| Uninstall | `bootout` + delete | disable + delete | `/end` + `/delete /f` |

`KeepAlive=false` today is a real parity gap: the daemon is not restarted after
a crash, and a superseded-binary exit would leave nothing running.

Windows gets `render_task_scheduler_xml(exe, sid) -> String` next to the
existing pure plist/unit renderers, snapshot-tested on all platforms, registered
with `schtasks /create /tn Paper /xml <file> /f`. The XML must be UTF-16LE with
BOM. `schtasks` flags cannot express restart-on-failure or instance policy, so
XML is mandatory rather than optional.

If task registration fails with access-denied or policy, degrade:
`HKCU\...\Run` value plus an immediate spawn, recorded as
`ServiceState::Degraded` and explained by `paper doctor`. High-confidence but
not officially worded: standard users may register tasks that run as
themselves. Verify on hardware before relying on it.

### 3. IPC abstraction

One `ControlRequest`/`ControlResponse` enum, JSON lines, two transports:

- Unix: existing socket, plus `SO_PEERCRED`/`getpeereid` UID check.
- Windows: `\\.\pipe\paper-<user-SID>` via
  `tokio::net::windows::named_pipe`, byte mode, `first_pipe_instance(true)` as
  the singleton, `create_with_security_attributes_raw` with a DACL granting
  `GENERIC_ALL` to the user SID and SYSTEM (`PROTECTED_DACL_SECURITY_INFORMATION`),
  then `GetNamedPipeClientProcessId` → `OpenProcessToken` → `TokenUser` SID
  comparison after connect.

Byte mode is required: message mode returns `ERROR_MORE_DATA` for a line longer
than the buffer and breaks `AsyncBufReadExt::lines()`. Pipes remove
stale-socket cleanup entirely; the Windows path must not port
`remove_file(socket_cleanup)`.

Console: spawn the daemon with `CREATE_NO_WINDOW`; attempt
`CREATE_BREAKAWAY_FROM_JOB`, retry without it on `ERROR_ACCESS_DENIED` (the
parent's job must set `BREAKAWAY_OK`), and rely on the registered task for
durability. Do not flip the binary to the GUI subsystem — it detaches stdout and
breaks both the CLI and MCP stdio.

### 4. Discovery

Ask the OS, then distrust the answer.

| OS | API | Notes |
|---|---|---|
| Windows | `SHGetKnownFolderPath(FOLDERID_Documents/Desktop/Downloads)` | KFM-aware; `hToken = NULL` for current user |
| Windows extra | `known-folders`: `LocalDocuments`, `OneDrive`, `PublicDocuments` | IDs `dirs` does not expose |
| Linux | `$XDG_*_DIR`, then `$XDG_CONFIG_HOME/user-dirs.dirs` | literal localised path; `"$HOME"` means disabled |
| macOS | `NSFileManager.urls(for:in:)` equivalent | `dirs` already matches for unsandboxed use |

Dependencies: keep `dirs`; add `known-folders` under `cfg(windows)`; add
`fs4` for locks; add `walkdir` usage stays. Do not add `objc2` until Paper ships
as a `.app`.

Volumes: `FindFirstVolumeW` → `GetVolumePathNamesForVolumeNameW` →
`GetDriveTypeW` on Windows (not `GetLogicalDrives`, which misses letterless
mount points); `/proc/self/mountinfo` on Linux (not `/proc/mounts` — mount ID,
parent, `major:minor`, root and per-mount options are needed for bind-mount and
btrfs dedupe); `getfsstat(2)` on macOS. Allow
`ext4/xfs/btrfs/zfs/vfat/exfat/ntfs3/apfs`; deny
`proc/sysfs/tmpfs/cgroup/overlay/squashfs/erofs/autofs`; put
`nfs/cifs/sshfs/9p` and removable media behind opt-in.

Candidate ranking (advisory, never automatic for low ranks):

| Rank | Candidate | Default |
|---|---|---|
| 1 | explicit `--root` / config / MCP `add_root` | selected |
| 2 | Documents | selected |
| 3 | Desktop | selected |
| 4 | Downloads | selected |
| 5 | cloud containers (OneDrive, iCloud, CloudStorage) | selected if local-only content dominates |
| 6 | other known folders | offered |
| 7 | home | offered |
| 8 | fixed volume root | opt-in |
| 9 | removable | opt-in |
| 10 | network | opt-in |

Modifiers: `+40` exists and is a directory, `+30` readable, `−200` inside a
system/tooling directory, `−150` volume root with no `Documents`/`Users`/
`Desktop` child, `−300` macOS TCC-protected without consent, `−100` high
cloud-only ratio (sampled, never recursively). Dedupe by canonical path,
ancestor relationship, Windows volume GUID and Linux `(major:minor, root)`.

`Candidate { kind, source, exists, readable: Option<bool>, fs_kind, cloud,
cloud_only_ratio, requires_consent, score, default_selected }`. `readable`
stays `None` when not probed, because probing on macOS can raise the dialog.

Config gains:

```toml
[discovery]
auto_discover = true          # false => only explicit roots
exclude = []                  # paths never indexed, wins over auto_discover
include_volumes = false
include_cloud_only = false
```

The daemon's root set becomes `explicit ∪ (auto_discover ? discovered −
exclude : ∅)`. This is what makes `remove-root` stick: the command adds to
`roots` and `exclude` in one transaction.

**Walking.** Keep `walkdir`; `follow_links(false)`, `same_file_system(true)`.
Budgets: depth 12–16 for known folders, 3–4 for volume roots; 250k–1M entries
per root; 50–100 MB per file; 60–300 s startup slice with continuation. One
worker per physical device, not per root. Persist per-root crawl state (last
complete mtime, cursor, file identity) so a multi-hour scan resumes. Index key
becomes file identity (volume serial + file index; inode + dev) rather than the
display path.

Skip, without opening: `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS`,
`RECALL_ON_OPEN`, `OFFLINE`, and use `FIND_FIRST_EX_ON_DISK_ENTRIES_ONLY`.
Junctions and volume mount points are boundaries. On macOS do not add both
`~/Documents` and iCloud Drive's `Documents` entry (it is a symlink back).

**Permission state.** On `ErrorKind::PermissionDenied`, mark the root `denied`
with the diagnostic (`EPERM` = TCC on macOS, `EACCES` = POSIX,
`ERROR_ACCESS_DENIED` = ACL/BitLocker/CFA, `ERROR_NOT_READY` = locked volume),
stop retrying it, and surface one actionable line. This is the fix for the
observed 1,070-failed-reads-per-5-minutes loop. Negative-cache unreadable
files separately, so one bad file does not re-trigger either.

**Reconciler.** `ReadDirectoryChangesW` overflow returns success and discards
the buffer; `notify` warns of missed events. The existing five-minute tick
becomes the reconciler: keep it, and force a stat-walk on any watcher error or
watched-root removal.

### 5. macOS identity and path

The grant requires two stable things, and neither can be retrofitted.

- **Identity**: sign every shipped binary with a fixed `--identifier`. Either a
  Developer ID Application certificate (US$99/yr, notarizable, required for a
  cask) or a self-signed certificate (free; corroborated by several
  independent reports and the yabai/AeroSpace precedent, but with no primary
  Apple source — the 20-minute experiment in the notes must confirm it before
  committing). Apple Development certificates are wrong: they expire and
  re-provision. Ad-hoc can never work.
- **Path**: never versioned. A bare Mach-O at `…/<version>/paper` accumulates
  one orphaned TCC row per release, and this is why Claude Desktop, signed and
  notarized, still re-prompts.

Consequences:

- The `brew` **formula** channel cannot deliver a durable grant: Homebrew builds
  the bottle and installs into `Cellar/paper/<version>/`, so the signature is
  Homebrew's. Support the **cask** (CI-built, signed, stable path) instead.
- If a formula must exist, the LaunchAgent points at a Paper-managed signed copy
  under `~/.local/share/paper/bin/paper`, never the `bin/paper` symlink.
- `cargo install` re-signs ad-hoc on the user's machine, so grants break there
  by construction; that channel is documented as degraded.
- The editor plugin must register the LaunchAgent and let launchd own the
  process. An editor-spawned child is attributed to the editor, so the prompt
  says VS Code wants Documents and Paper never appears in Privacy & Security.

`paper doctor --tcc` runs `codesign -d -r-` (must show `identifier` +
`certificate leaf`, not `cdhash`), resolves the real path (must be unversioned),
queries `access` for `client like '%paper%'`, and streams tccd decisions for a
live verdict. Full Disk Access stays documented as a fallback, not a default:
it is the same requirement problem with a mail/messages/safari blast radius.

Signing in CI: `cargo-dist`'s macOS signing is experimental, workspace-only and
has no `notarytool` step, so the workflow gets explicit `codesign` +
`notarytool` + `stapler` steps and a CI assertion on the requirement string.

### 6. Provisioning

```
setup ──spawns──▶ daemon ──▶ ModelProvisioner ──▶ Indexer
                               │
                               ├─ embedder: priority, index starts here
                               └─ reranker: parallel, no gate
```

**Gate**: only tier 1 waits for the embedder. Text is indexed immediately.

**Schema**: a side table, not a nullable column on the FTS5 external-content
table (churn would fire triggers and pollute the index).

```sql
CREATE TABLE chunk_embedding (
  chunk_id  INTEGER PRIMARY KEY REFERENCES chunks(id) ON DELETE CASCADE,
  model_key TEXT NOT NULL,          -- 'bge-small-en-v1.5@sha256:...:384'
  state     TEXT NOT NULL,          -- 'pending' | 'ready' | 'failed'
  attempts  INTEGER NOT NULL DEFAULT 0,
  updated_at INTEGER NOT NULL
);
CREATE INDEX chunk_embedding_pending ON chunk_embedding(state) WHERE state = 'pending';
```

The `pending` row is inserted in the same transaction as the chunk. The worker
takes 512 pending rows, embeds, writes `chunks_vec`, flips state. A model change
flips all rows to `pending` with one `UPDATE` because `model_key` carries the
weights hash. Search always serves tier 0; tier 1 joins ready rows only and
returns `{"degraded":["semantic"],"pending_chunks":N}`. Exit code stays 0 when
tier 0 found something, 3 only when `--semantic` was explicit.

**Downloader**: custom `reqwest` + `sha2`, not `hf-hub`.
Per artefact `<sha256>/file`, `.part`, `.part.json`, `.lock`; sidecar stores
`{url, etag, last_modified, total, sha256, updated_at}`. Resume with `Range`
**and** `If-Range`; on `200` or `416` to a resume, hash and either promote or
discard. Never trust a journalled offset — use `min(offset, file_len)`.
`Accept-Encoding: identity`. `fsync` file, rename, `fsync` directory. Preflight
`sum(remaining) * 1.10 + 128 MiB` against available space on the destination
filesystem; `ENOSPC` is terminal for that artefact.

Concurrency: global `Semaphore(8)` for HTTP, `Semaphore(3)` for artefacts, parts
`clamp(ceil(size / 32 MiB), 1, 8)`, embedder priority for the first 10 s. Start
at 2 connections and add while per-connection throughput exceeds ~1 MiB/s and
the file is over 64 MiB. A token bucket beats reducing concurrency.

**State**: `$PAPER_HOME/models/state.json` written atomically under an exclusive
lock, plus `state.jsonl` transitions. States
`Absent → Queued → Downloading → Verifying → Ready`, with `Paused` after process
death, `Failed{Checksum}` terminal for the session. The filesystem is truth; the
JSON is a view reconciled at startup. CLI and MCP read the same file, so
progress survives restarts and dual downloaders are arbitrated by the lock.

Offline: `PAPER_OFFLINE=1` skips the network, resolves from state and disk,
reports `Missing{offline}`, and never deletes resumable bytes. TLS uses native
roots, because `webpki-roots` only breaks corporate MITM laptops.

### 7. MCP registration

A registry table, one serialiser per format, one adapter per client.

| Client | Path (macOS / Linux / Windows) | Format | Key |
|---|---|---|---|
| Claude Code | `~/.claude.json`, `<proj>/.mcp.json` / same / `%USERPROFILE%\.claude.json` | JSON | `mcpServers` |
| Claude Desktop | `~/Library/Application Support/Claude/claude_desktop_config.json` / `$XDG_CONFIG_HOME/Claude/…` / `%APPDATA%\Claude\…` | JSON | `mcpServers` |
| OpenCode | `~/.config/opencode/opencode.json[c]` / same / `%USERPROFILE%\.config\opencode\…` | JSONC | `mcp.servers` (V2), `command` is an array, `environment` |
| Codex | `~/.codex/config.toml` / same / `%USERPROFILE%\.codex\config.toml` | TOML | `[mcp_servers.<name>]` |
| Cursor | `~/.cursor/mcp.json` / same / `%USERPROFILE%\.cursor\mcp.json` | JSON | `mcpServers` |
| Windsurf | `~/.codeium/windsurf/mcp_config.json` / same / `%USERPROFILE%\.codeium\…` | JSON | `mcpServers` |
| VS Code | `~/Library/Application Support/Code/User/mcp.json` / `~/.config/Code/User/mcp.json` / `%APPDATA%\Code\User\mcp.json` | JSONC | `servers` (native `mcpServers` in `~/.copilot/mcp-config.json`) |
| Zed | `~/.config/zed/settings.json` / same / `%APPDATA%\Zed\settings.json` | JSONC | `context_servers` |
| Cline | `~/.cline/data/settings/cline_mcp_settings.json` | JSON | `mcpServers` |
| Continue | `~/.continue/config.yaml` | YAML | `mcpServers` (array) |
| Gemini CLI | `~/.gemini/settings.json` | JSON | `mcpServers` |
| Hermes | `~/.hermes/config.yaml` | YAML | `mcp_servers` (mapping) |
| Antigravity | `~/.gemini/config/mcp_config.json` | JSON | `mcpServers`, `serverUrl`/`timeoutSeconds` |

Detection, in descending confidence: explicit `--clients`; binary probe on
`PATH` (respect `PATHEXT`); app bundle; vendor data directory; config file last.
Honour relocatable homes (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `COPILOT_HOME`,
`CLINE_DIR`, `XDG_CONFIG_HOME`). Never `mkdir -p` a vendor directory — creating
`~/.continue` fabricates a user. Creating a file inside an existing directory is
fine.

Merge: resolve → skip if undetected → warn (not block) if running → read →
back up `<file>.paper-bak-<iso8601>` → parse with a CST (`taplo` for TOML, a
JSONC-aware parser) → look up `paper` (normalise on `{basename(command), args,
env-subset}`) → no-op / conflict unless `--force` / append last → atomic write
(temp in same directory, `fsync`, rename, `fsync` dir) → record in
`$PAPER_HOME/clients.json` `{client, path, key, sha256(entry), installed_at,
installer_version}`. Lock a sibling `<file>.paper.lock`, never the config file:
Windows cannot rename over a held file.

Vendor CLIs only where documented-additive and verified by re-reading: Claude
Code `claude mcp add --scope user`, Cline `cline mcp add`, Gemini `gemini mcp add
-s user` (default scope is project). Never Codex's (drops comments), never
`code --add-mcp` (writes the deprecated surface slated for removal in Feb 2027).

Command paths are absolute everywhere. Windows uses
`%LOCALAPPDATA%\Programs\Paper\paper.exe` with the `.exe` suffix. Do not emit
`cwd`: fatal in Gemini, dropped in Codex, and meaningless for a root-indexed
tool.

Uninstall is keyed on the recorded entry: one JSON key, one TOML table *plus its
sub-tables*, one Continue array element, one Hermes mapping key. Refuse if the
recorded hash changed — the user edited it, so it is theirs; print a diff and
require `--force`. Delete a file only if Paper created it and it now holds an
empty container with no other keys. Run uninstall before binary removal.

### 8. Install channels

| Channel | Install-time action |
|---|---|
| our `install.sh` / `install.ps1` (primary) | download, verify, install to stable path, `paper setup --detach` |
| Homebrew cask | `postflight` → `paper setup --no-service`; LaunchAgent from setup |
| Homebrew formula | unsupported for durable TCC; `caveats` only |
| Scoop | `post_install` → `paper setup --detach` |
| npm | `postinstall` → fire-and-forget, always `exit 0` |
| winget | portable/zip only, no scripts; setup on first run |
| `cargo install` | first run (documented as degraded for macOS permissions) |
| Claude plugin | `.mcp.json` auto-registers the server; guarded `SessionStart` hook runs `paper setup --no-service`; `${CLAUDE_PLUGIN_DATA}` is durable, `${CLAUDE_PLUGIN_ROOT}` is not |
| MCP Registry | metadata only |

The primary channel is our own script, not cargo-dist's generated installer,
because the latter cannot call `paper setup`. cargo-dist still produces the
artifacts.

### 9. Data layout

`Paths::new()` currently takes data from `dirs::cache_dir()` and config from
`dirs::config_dir()`; on Windows the latter is the roaming profile, so
machine-specific roots would follow a user between machines. Move config beside
data under LocalAppData, with `PAPER_HOME` still authoritative.

```
$PAPER_HOME/            (macOS: ~/Library/Application Support/paper)
  chunks.db
  config.toml
  setup/journal.json
  models/state.json, state.jsonl, <sha256>/{file,.part,.part.json,.lock}
  clients.json
  logs/
```

### 10. Migration

Existing installs have no journal and no `[discovery]` section. On first run of
the new version: treat an existing `config.toml` as `auto_discover = false` if
it has explicit roots, otherwise `true`; write the missing journal phases as
complete once verified; leave `chunks.db` untouched except the additive
`chunk_embedding` table, and queue a backfill.

Removal of a protected root must also stop the loop for users who already have
one configured: on upgrade, any root that returns `PermissionDenied` is added to
`exclude` automatically and reported once.

## Decisions still owned by the human

1. **macOS certificate.** Self-signed (free, needs the 20-minute experiment to
   confirm TCC durability) or Developer ID + notarization (US$99/yr, also
   required for a cask and for Gatekeeper). Recommendation: run the experiment,
   ship self-signed, buy Developer ID before a public 1.0.
2. **Homebrew.** Cask only (recommended), or formula with a Paper-managed
   `~/.local/share/paper/bin/paper` and a caveat.
3. **Windows certificate.** Azure Artifact Signing (~US$120/yr, individuals in
   US/Canada only), OV certificate with hardware token, or unsigned with
   expected SmartScreen warnings and no Windows agent registration.
4. **Default roots above rank 5.** Recommendation: never select volume roots
   automatically, even on a machine with eight disks.

## Testing

Unit: ranking and dedupe with synthetic candidates per OS; scheduler XML/plist/
unit snapshots; journal phase selection across upgrades; TOML/JSONC/YAML merge
with comment preservation, duplicate detection and refusal-on-edit; downloader
resume against a mock server (206, 200-on-resume, 416, truncated body, ETag
change, `ENOSPC`); `chunk_embedding` transitions and a killed-mid-embed restart.

Integration: five concurrent `serve` plus one `setup` yields one provisioner
(measured on wall clock and on a counter); daemon restart after `remove-root`
does not re-add the path; a `PermissionDenied` root is attempted once then
skipped; `PAPER_OFFLINE` never deletes a `.part`.

Platform: `cargo test` on macOS, Linux and Windows CI (the latter two at least
compile-and-unit level); one manual pass on real Windows hardware for console
flash, task registration, logoff termination, rename-while-running and
`schtasks` encoding, which no documentation substitutes for.

## Rollout order

1. **Stop the bleeding** (small, independent): `{e:#}` logging, `PermissionDenied`
   backoff and exclusion, `serve` removed from bootstrapping, kernel lock
   replacing the racy one. Ships alone.
2. **Discovery** behind `[discovery]`, plus `paper roots --json` and
   `paper setup --status`.
3. **`paper setup` + journal + lock**, still Unix-only supervisors.
4. **Windows** supervisor, named-pipe IPC and `render_task_scheduler_xml`.
5. **Provisioning**: `chunk_embedding`, embedder gate, parallel reranker, custom
   downloader, `state.json`.
6. **Integrations**: registry, detection, merge, `clients.json`, uninstall.
7. **Signing and CI**: codesign + notarytool + requirement assertion, install
   scripts, cask, Scoop manifest.

Each step must leave `cargo check` warning-free and the 55-test suite green, and
update `docs/implementation-status.md` when behaviour, models or counts change.
