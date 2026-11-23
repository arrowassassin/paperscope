# Code signing and permission persistence for an independently distributed Rust CLI (macOS TCC, Windows, Linux)

Scope note: this covers the grant-durability problem for a single-binary Rust CLI that installs a per-user background agent and reads `~/Documents`, `~/Downloads`, `~/Desktop`. Evidence is drawn from Apple's (sparse) documentation, Howard Oakley's log-level reverse engineering, and several 2026 bug threads from shipping products that hit exactly this failure (Claude Desktop/CLI, Orca, Buzz, 1Password CLI). Where a claim rests on practitioner blogs rather than Apple documentation, that is stated explicitly.

## How does tccd construct and evaluate a code requirement (csreq) for a client, and why is `DB Action:None` returned?

### Takeaway
TCC does not store "app X is allowed". It stores a serialized **code requirement** (`csreq`), and at each access it recomputes the caller's current code identity and asks whether that identity *satisfies* the stored requirement. For an ad-hoc/linker-signed binary the auto-generated designated requirement collapses to `cdhash H"…"`, so every rebuild produces a new identity and the old row becomes an orphan. `DB Action:None` means tccd evaluated the request but neither applied nor wrote a database decision — it is a symptom of an identity/attribution problem, not of the user's answer.

### Cited Findings
- TCC keeps per-user grants in `~/Library/Application Support/com.apple.TCC/TCC.db` and system-wide grants (including Full Disk Access) in `/Library/Application Support/com.apple.TCC/TCC.db`; the `access` table has `service`, `client`, `client_type`, `auth_value`, `auth_reason`, `csreq`, `flags`, `last_modified` — [HackTricks](https://hacktricks.wiki/en/macos-hardening/macos-security-and-privilege-escalation/macos-security-protections/macos-tcc/index.html); [Nick Liu](https://nick-liu.com/posts/tcc-cdhash-trap/).
- `client_type` is `0` for a bundle identifier and `1` for an absolute path — [Nick Liu](https://nick-liu.com/posts/tcc-cdhash-trap/). A bare `/bin/bash` is logged as `IDENTITY_ATTRIBUTION: identifier = /bin/bash, type 1` — [build log gist](https://gist.github.com/TatsuShino/c4d02258272f08bfab856a86f4d5ec32).
- The `csreq` is a binary-serialized code requirement; it can be decoded with `csreq -r- -t` and produced from a requirement string with `csreq -r- -b` — [Stack Overflow](https://stackoverflow.com/questions/52706542/how-to-get-csreq-of-macos-application-on-command-line).
- Ad-hoc signing (`codesign -s -`) yields `designated => cdhash H"…"`; any byte change alters the cdhash, so the stored requirement no longer matches and the grant is orphaned even though the Privacy pane still shows the entry enabled — [Nick Liu](https://nick-liu.com/posts/tcc-cdhash-trap/); [aidrivenlife](https://www.aidrivenlife.com/blog/01-adhoc-codesign-orphans-tcc); [Barkeep](https://barkeepmac.com/macos-permissions-lost-after-rebuild).
- Signing with a certificate (even self-signed) changes the designated requirement to `identifier "…" and certificate leaf = H"…"`, which is stable across rebuilds — [Nick Liu](https://nick-liu.com/posts/tcc-cdhash-trap/); [evoleinik](https://evoleinik.com/posts/macos-dev-signing-preserve-permissions/).
- The auto-generated Developer ID designated requirement has the shape `identifier "com.x.y" and anchor apple generic and certificate 1[field.1.2.840.113635.100.6.2.6] exists and certificate leaf[field.1.2.840.113635.100.6.1.13] exists and certificate leaf[subject.OU] = TEAMID` — [Santa TCC profile docs](https://northpole.dev/deployment/profile-tcc/). Note this binds to the **Team ID (OU)**, not to a specific leaf certificate, so annual certificate renewal does not change it.
- A bare Mach-O installed at a version-numbered path accumulates one path-keyed (`client_type=1`) TCC identity (and one orphaned row) per release: rows were recorded for `…/versions/2.1.214`, `…/2.1.215`, `…/2.1.219` … through `…/2.1.233` — [claude-code #87144](https://github.com/anthropics/claude-code/issues/87144).
- Path instability breaks grants even for correctly signed and notarized bundles: Claude Desktop's embedded engine at `…/claude-code/2.1.246/claude.app`, signed `Developer ID Application: Anthropic PBC, team Q6L2SF6YDW`, still produced `ReqResult(Auth Right: Denied (Service Policy), promptType: 1, DB Action:None)`; the reporter's diagnosis is "a path-stability problem, not a code-signing one" — [claude-code #91854](https://github.com/anthropics/claude-code/issues/91854).
- `DB Action:None` appears repeatedly where the *subject* and the *responsible process* disagree: `from Sub:{xyz.block.buzz.app} Resp:{… binary_path=/opt/homebrew/Caskroom/1password-cli/2.32.0/op}, ReqResult(… DB Action:None)`, with every sidecar signed under its own identifier rather than the app's — [block/buzz #2783](https://github.com/block/buzz/issues/2783).
- On macOS 26, `kTCCServiceSystemPolicyAppData` consent is issued as `authorization_right=Session` and "Tahoe never persists this grant", so consent re-fires on every spawn — [Orca #12971](https://github.com/stablyai/orca/issues/12971). A related thread notes a `Create` event was logged on Allow yet later lookups still returned `DB Action:None`, and that `tccutil reset` did not fix it — [claude-code #83841](https://github.com/anthropics/claude-code/issues/83841).
- TCC runs an attribution chain (`accessing` / `requesting` / `responsible`) before deciding, and checks Full Disk Access (`kTCCServiceSystemPolicyAllFiles`) *before* the per-folder service (`kTCCServiceSystemPolicyDocumentsFolder`) — [Howard Oakley, log extracts](https://eclecticlight.co/2026/04/08/privacy-files-folders-or-full-disk-access/).

### Inferences
- For Paper's exact log line, the most probable reading of `DB Action:None` is that tccd could not form a durable, matchable client identity for the request and therefore granted only for the session. Candidate causes, in order of likelihood for a linker-signed CLI run by a LaunchAgent: (a) the binary's requirement is `cdhash`-only and the path is not the stable path the agent was granted against (e.g. Homebrew's `bin/paper` symlink resolving into `Cellar/paper/<version>/…`); (b) the request is attributed to a responsible process other than the binary; (c) the service is being treated as session-scoped on macOS 26+, as observed for AppData. These are distinguishable in one sitting with the commands below — do not guess.
- `matches platform requirements: No` is almost certainly the `anchor apple` / platform-binary test: it says the client is not Apple-signed. It is expected for *every* independent developer's binary, including notarized Developer ID ones, and is not by itself a reason to refuse persistence. Treat it as noise unless a test shows Developer ID-signed binaries also fail to persist.
- Because the Developer ID requirement binds to Team ID rather than the leaf certificate, a Developer ID grant should survive certificate renewal; a self-signed grant binds to the leaf hash and therefore survives rebuilds but **not** certificate rotation.
- Re-signing the *same* binary with the original identity makes the old grant match again without a new prompt — reported explicitly by [aidrivenlife](https://www.aidrivenlife.com/blog/01-adhoc-codesign-orphans-tcc).

### Gaps
- No public documentation of the exact `ID Attribution … matches platform requirements` string or of tccd's rule for when it writes vs. skips the DB row. Apple's TCC internals are undocumented; Apple Developer Forums thread 118508 was not retrievable (bot check), so no DTS statement is cited here.
- Apple publishes no specification of `client_type`, `auth_reason`, or "session" authorization rights; all of the above is reverse-engineered by third parties.

### Verification commands (use these to settle Paper's case)
```bash
# What identity will TCC see?
codesign -dvvv --entitlements :- "$(command -v paper)"
codesign -d -r- "$(command -v paper)"          # expect: designated => cdhash H"..." today
readlink -f "$(command -v paper)"              # is the real path versioned?

# What is actually stored?
sqlite3 "$HOME/Library/Application Support/com.apple.TCC/TCC.db" \
  "select service, client, client_type, auth_value, auth_reason, datetime(last_modified,'unixepoch')
     from access where client like '%paper%';"

# What does tccd decide, live?
log stream --info --debug --predicate 'subsystem == "com.apple.TCC"' | grep -i -E 'paper|DB Action|platform requirements'
```

## Which identity does TCC use for a CLI launched by launchd — the binary, its responsible process, or an app bundle?

### Takeaway
TCC evaluates the **responsible process** at the head of the process's attribution chain. A LaunchAgent has no application ancestor, so the job's own executable is the subject, and a bare Mach-O gets a path-keyed identity. A process spawned by an Electron editor inherits the **editor** as responsible process, so the grant lands on the editor, not on Paper. The only way a child escapes that inheritance is `responsibility_spawnattrs_setdisclaim()` at `posix_spawn` time — a private API used by Terminal.app, iTerm2 and Homebrew.

### Cited Findings
- TCC logs an attribution chain of `accessing`, `requesting` and `responsible` processes and evaluates the app "at the head of the attribution chain" — [Howard Oakley](https://eclecticlight.co/2026/04/08/privacy-files-folders-or-full-disk-access/).
- When a tool is spawned by an Electron app, tccd names the app as responsible even though `binary_path` points at the tool: `Sub:{com.stablyai.orca} Resp:{… responsible_path=/Applications/Orca.app/Contents/MacOS/Orca, binary_path=/opt/homebrew/Caskroom/1password-cli/2.38.1/op}` — [Orca #12971](https://github.com/stablyai/orca/issues/12971); same pattern for Buzz — [block/buzz #2783](https://github.com/block/buzz/issues/2783); and for the 1Password CLI under cmux — [Orca #6996](https://github.com/stablyai/orca/issues/6996).
- Wrapping children in `login -flpq <user>` does **not** break responsibility inheritance on macOS 26; the mechanism terminals actually use is `responsibility_spawnattrs_setdisclaim(attrs, 1)` at spawn time — [Orca #12971](https://github.com/stablyai/orca/issues/12971).
- `responsibility_spawnattrs_setdisclaim` is a private API that gives a spawned subprocess "its own permissions"; Terminal.app and iTerm2 use it — [steipete, Making AppleScript Work in macOS CLI Tools](https://steipete.me/posts/2025/applescript-cli-macos-complete-guide).
- Homebrew uses the same technique: "Homebrew attempts to disclaim TCC responsibility when launching sandboxed commands for formulae and casks, preventing them from inheriting the launching application's privacy permissions, such as Full Disk Access… This additional protection uses a private macOS API because there is no documented public equivalent for command-line workers." — [Homebrew Security and Supply Chain](https://docs.brew.sh/Homebrew-Security-and-Supply-Chain).
- A properly bundled macOS app is identified by its bundle ID; embedding an `Info.plist` in a CLI and signing it with a stable identifier is the documented community recipe for giving a CLI a TCC identity — [steipete](https://steipete.me/posts/2025/applescript-cli-macos-complete-guide).
- A daemon reparented to launchd still had its AppData accesses attributed to the owning app because the daemon lived inside the app bundle — [Orca #12971, comment](https://github.com/stablyai/orca/issues/12971).

### Inferences
- **LaunchAgent path (Paper's current design):** subject = `Paper` binary, `client_type=1`, keyed on its resolved absolute path. This is the *good* case for a durable grant, because there is no third-party app to blame — but it is exactly the case where an ad-hoc cdhash requirement is fragile.
- **Editor-plugin path:** subject = the editor (VS Code/Cursor/Zed), so "Allow" writes a grant for the *editor*. Consequences: Paper never appears in Privacy & Security; the prompt reads "Code would like to access your Documents"; a denial affects every tool the editor spawns. This is actually a cheap way to stop re-prompting, but it is user-hostile and untestable from Paper's side.
- The plugin cannot fix this itself: `setdisclaim` must be applied by the process doing the `posix_spawn`, i.e. the editor, unless the plugin ships its own native launcher that spawns Paper with disclaim. That launcher is a viable but non-trivial workaround.
- Therefore the durable design is: **editor plugin installs/registers the LaunchAgent and lets launchd own the process**, rather than the plugin running the indexer as its child.

### Gaps
- No Apple documentation states the responsible-process rule; `responsibility_spawnattrs_setdisclaim` is private and unversioned, so relying on it in a plugin is a compatibility risk.
- Whether a LaunchAgent's responsible process is the job executable or `launchd` itself is not documented. Behaviour is inferred from the fact that menu-bar agents and third-party daemons do receive their own TCC prompts. This should be confirmed empirically with `log stream` on the Paper agent.
- TCC prompts require a GUI (Aqua) session; a LaunchAgent firing at login before WindowServer may be unable to prompt. No source found that documents this; flagged as a risk to test (first-run indexing should be deferred to a user-visible action).

## Options and verdicts: certificate, notarization, bundle, ad-hoc

### Takeaway
Only a **stable code identity plus a stable install path** produces a durable grant. There are exactly two affordable ways to get one: a Developer ID Application certificate (US$99/year, notarizable, required for Homebrew cask) or a self-signed code-signing certificate with a fixed `--identifier` (free, works for cargo/curl/plugin channels, cannot be notarized). Ad-hoc cannot work. Apple Development certificates work technically but are the wrong tool. Bundling in a `.app` improves the UX and the FDA story but does not by itself fix cdhash pinning.

### Cited Findings
- Apple Developer Program membership is US$99/year and is the prerequisite for a Developer ID certificate — [Apple Developer Program](https://developer.apple.com/programs/enroll/); Developer ID is the certificate class for distribution outside the App Store — [Apple Developer ID](https://developer.apple.com/developer-id/).
- Notarization is Apple's automated malware/signature scan; the ticket is stapled to the artifact and can be revoked — [Apple: Notarizing macOS software before distribution](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution); [Homebrew's summary of the three layers](https://docs.brew.sh/Homebrew-Security-and-Supply-Chain).
- Gatekeeper checks developer identity, notarization ticket and signature on first open of quarantined software — [Apple Platform Security: Gatekeeper](https://support.apple.com/guide/security/gatekeeper-and-runtime-protection-sec5599b66df/web).
- A self-signed code-signing certificate in the login keychain produces a stable designated requirement and grants "survive rebuilds"; `codesign` itself does not require the certificate to be trusted — [Nick Liu](https://nick-liu.com/posts/tcc-cdhash-trap/); independently: [evoleinik](https://evoleinik.com/posts/macos-dev-signing-preserve-permissions/), [aidrivenlife](https://www.aidrivenlife.com/blog/01-adhoc-codesign-orphans-tcc), [Barkeep](https://barkeepmac.com/macos-permissions-lost-after-rebuild).
- This is the long-standing distribution model for yabai, skhd and AeroSpace: users create a local certificate and re-sign the installed binary — [yabai wiki, "Codesigning yabai"](https://github.com/koekeishiya/yabai/wiki/Installing-yabai-(latest-release)#codesigning-yabai); corroborated by [Nick Liu](https://nick-liu.com/posts/tcc-cdhash-trap/).
- Changing an app's Team ID invalidates users' existing TCC grants across all classes — observed when Orca changed Team ID from `2DC432GLL2` to `6CX3WHS9HZ` — [Orca #12971](https://github.com/stablyai/orca/issues/12971).
- `security find-identity -v -p codesigning` hides self-signed roots (`CSSMERR_TP_NOT_TRUSTED`); drop `-v` to detect them — [Nick Liu](https://nick-liu.com/posts/tcc-cdhash-trap/).

### Verdict matrix

| Option | Durable TCC grant? | Survives rebuild (new cdhash)? | Survives upgrade? | What the user sees |
|---|---|---|---|---|
| **Developer ID Application + notarization**, fixed path, stable `--identifier` | **Yes** — requirement keyed on `identifier` + Team ID (OU) | Yes | Yes, provided install path is unversioned and Team ID is unchanged | One normal dialog ("Paper would like to access your Documents folder"); entry in Privacy & Security; no Gatekeeper warning |
| **Developer ID, not notarized** | Yes (TCC ignores notarization) | Yes | Yes | Same dialog, but Gatekeeper blocks a quarantined download until right-click-open / `xattr -d com.apple.quarantine` |
| **Self-signed code-signing cert**, fixed `--identifier`, fixed path | **Yes** per multiple independent reports; requirements are evaluated as hashes, not trust chains | Yes (same cert) | Yes, until the cert is rotated or lost | Same dialog; identical UX to Developer ID except Gatekeeper/quarantine and Homebrew cask audit |
| **Apple Development cert** | Technically yes (stable DR with Team ID) | Yes | Not after expiry/re-provisioning churn; unusable for distribution or notarization | Gatekeeper rejects on other machines; wrong tool when Developer ID costs the same |
| **Signed `.app` bundle + helper, ad-hoc** | No | No | No | Entry appears under a friendly name but silently stops matching after each build |
| **Signed `.app` bundle + helper, Developer ID/self-signed** | Yes, and best UX | Yes | Yes, if the bundle path is unversioned | Proper app name/icon in Privacy & Security and in the FDA picker |
| **Status quo: ad-hoc, linker-signed** | No — and Paper's `DB Action:None` says not even a row is being written | No | No | Prompt on every launch/index; user loses trust in the product |

### Inferences
- Notarization is orthogonal to TCC. It matters for Gatekeeper and for Homebrew's cask audit, not for grant persistence. A Developer ID signature without notarization should already fix Paper's re-prompt loop; notarization is then required to make the *installer* acceptable.
- The cheapest correct fix for Paper's stated goal ("a grant that survives rebuilds and upgrades") is a self-signed certificate **if** macOS never asks for a trust anchor. Four independent reports plus the yabai/AeroSpace precedent say it does not. The 20-minute experiment below settles it for Paper's exact service before committing.
- Self-signed has three operational failure modes that must be designed for: (1) losing the private key orphans every user's grant forever; (2) rotating the certificate does the same; (3) if the binary is ever rebuilt on the user's machine (cargo install), the build system will re-sign ad-hoc and silently undo the grant.
- Because `cargo install` and `brew install` build/link on the user's machine, those channels **cannot** deliver your signature. Only a prebuilt artifact from your own CI (curl installer, cask, plugin download) can. This is an architectural constraint, not a packaging detail.

### Gaps
- No primary Apple source states that a self-signed (untrusted) certificate is sufficient for a persistent TCC grant; the claim is well corroborated by practitioners but is not contractual. Apple could change the platform-requirement check.
- No source quantifies whether macOS 26/27 adds extra restrictions for non-notarized clients beyond what is reported.
- Cost of a Developer ID outside the Apple Developer Program does not exist; $99/year is the only route, but regional pricing and the requirement for a legal entity vs. individual were not verified.

### Experiment that decides self-signed vs Developer ID in ~20 minutes
```bash
# 1. Create a self-signed code-signing cert (Keychain Access → Certificate Assistant,
#    Identity Type: Self Signed Root, Certificate Type: Code Signing), or:
openssl req -x509 -newkey rsa:2048 -days 3650 -keyout dev.key -out dev.crt -nodes \
  -subj "/CN=Paper Local Signing" \
  -addext "keyUsage=critical,digitalSignature" \
  -addext "extendedKeyUsage=codeSigning"
openssl pkcs12 -export -legacy -in dev.crt -inkey dev.key -out dev.p12 -pass pass:dev
security import dev.p12 -k ~/Library/Keychains/login.keychain-db -P dev -T /usr/bin/codesign

# 2. Sign with a stable identifier, install at a stable path
codesign --force --sign "Paper Local Signing" --identifier com.kintsugi.paper \
  --timestamp=none target/release/paper
codesign -d -r- target/release/paper     # must show identifier + certificate leaf, NOT cdhash

# 3. Run the agent, allow the prompt, then confirm a row exists:
sqlite3 "$HOME/Library/Application Support/com.apple.TCC/TCC.db" \
  "select service, client, client_type, auth_value, csreq is not null from access where client like '%paper%';"

# 4. Rebuild (cdhash changes), rerun, confirm NO new prompt.
# 5. Repeat with a Developer ID cert to confirm parity before paying.
```

## Full Disk Access: storage model, survival, and whether it is the pragmatic answer

### Takeaway
FDA is `kTCCServiceSystemPolicyAllFiles`, stored in the **system** TCC database, and it short-circuits every Files & Folders check — Oakley's logs show the AllFiles request being evaluated *before* the per-folder service. It is still a code-identity grant, so it obeys the same csreq rules: a stable signature plus a stable path is what makes it durable. It is pragmatic as a documented escape hatch, but it over-grants massively and is a poor default for a document search tool.

### Cited Findings
- TCC first checks `kTCCServiceSystemPolicyAllFiles`; if granted, the sandbox is told to allow access. Only if not granted does it consider `kTCCServiceSystemPolicyDocumentsFolder` — [Howard Oakley](https://eclecticlight.co/2026/04/08/privacy-files-folders-or-full-disk-access/).
- Disabling FDA "doesn't deny access, it just doesn't enable it", whereas disabling a Files & Folders entry actively denies that location — a semantic difference users and support docs must get right — [Howard Oakley](https://eclecticlight.co/2026/04/08/privacy-files-folders-or-full-disk-access/).
- FDA is stored in the system DB (`/Library/Application Support/com.apple.TCC/TCC.db`), not the per-user one — [HackTricks](https://hacktricks.wiki/en/macos-hardening/macos-security-and-privilege-escalation/macos-security-protections/macos-tcc/index.html).
- FDA grants for an app are invalidated by updates in practice: "macOS TCC grants (Full Disk Access / Desktop) keep getting invalidated after Orca updates" — the remedies users found were toggling FDA off/on or rebooting — [Orca #13921](https://github.com/stablyai/orca/issues/13921).
- Granting FDA to the *host* stops child-process prompts because the AllFiles check runs before the more specific service check — the Orca workaround thread documents exactly this trade-off, at the cost of giving everything in the host access to Mail, Messages and other apps' data — [Orca #12971, comment](https://github.com/stablyai/orca/issues/12971).
- FDA to a shell script's interpreter, not the script, is what matters for launchd-run scripts (the classic "grant Full Disk Access to `/bin/bash`" workaround) — [kith.org](https://www.kith.org/jed/2022/02/15/launchctl-scheduling-shell-scripts-on-macos-and-full-disk-access/); an `fdautil exec` wrapper exists for the same reason — [nunn.au](https://nunn.au/2023/11/28/tcc-launchd-woes).
- `tccutil reset All <bundle-id>` clears that client's rows; Apple documents `tccutil reset <service>` and `tccutil reset All` — [Apple: Resetting access to protected resources](https://developer.apple.com/documentation/xcode/resetting-access-to-protected-resources-in-macos); Jamf documents `sudo tccutil reset All <bundle-id>` — [Jamf](https://learn.jamf.com/r/en-US/technical-articles/Resetting_Transparency_Consent_and_Control_Prompts_on_macOS).
- Removing an app's Files & Folders entry may require a logout or restart to take effect — [Howard Oakley](https://eclecticlight.co/2026/04/08/privacy-files-folders-or-full-disk-access/).

### Inferences
- FDA is not a way *around* the csreq problem; it is the same problem with a bigger hammer. A cdhash-pinned FDA entry breaks on the next build exactly like a Documents-folder entry.
- FDA's practical advantage is that it can be granted to a **stable stub** that never changes (e.g. a tiny launcher, or even `/bin/bash`), and every child of that stub inherits practical access. That is a legitimate architecture: keep a frozen, stably-signed access broker and let the versioned indexer talk to it.
- FDA's practical disadvantage for `paper`: it grants read access to Mail, Messages, Safari history, other apps' containers, and every other protected location. For a tool whose stated value proposition is "local-first, no telemetry", asking for FDA is a trust liability and will read as malware behaviour to reviewers.
- `tccutil reset` cannot target a bare binary by path (it takes a bundle ID), so users cannot cleanly reset a path-keyed CLI grant except with `tccutil reset All` — which nukes every app's grants. That is a support-cost argument for shipping a bundle ID.

### Gaps
- No source confirms whether a path-keyed FDA entry survives replacing the file at the same path when the signature is unchanged vs. ad-hoc. Inferred that it follows the csreq rule; needs a 10-minute test (grant FDA to a signed stub, rebuild, observe).
- Apple does not document the "add a bare binary to FDA" flow or how the picker resolves symlinks; behaviour with Homebrew's `bin/` symlinks is unverified.

## Security-scoped bookmarks and `NSFileCoordinator`: can a CLI get persistent folder access without TCC prompts?

### Takeaway
No. Security-scoped bookmarks are an **App Sandbox** mechanism for persisting user intent *inside* the sandbox; they do not exempt a process from TCC, and they require AppKit/Foundation plus entitlements. `NSFileCoordinator` is about coordinated access (iCloud/file presenters), not authorization. For a non-sandboxed CLI, the only "no prompt" paths are: inherit a grant from a responsible parent (the editor), hold FDA, or have the user pick the folder in an Open panel so TCC records intent.

### Cited Findings
- Security-scoped bookmarks exist so a sandboxed app can regain access to a user-selected file/folder across launches; resolving a stored bookmark does **not** automatically extend the sandbox — the app must call `startAccessingSecurityScopedResource()` — [Apple: Accessing files from the macOS App Sandbox](https://developer.apple.com/documentation/security/accessing-files-from-the-macos-app-sandbox).
- App-scoped bookmarks require `com.apple.security.files.bookmarks.app-scope` plus `com.apple.security.files.user-selected.read-write`, i.e. an App Sandbox entitlement set — [AppCoda](https://www.appcoda.com/mac-apps-user-intent/).
- A security-scoped bookmark is how an app that has access to a specific file "saves that access and regains it" later — [Mothersruin, URL Bookmarks and Security-scoping](https://www.mothersruin.com/software/Archaeology/reverse/bookmarks.html).
- TCC's per-folder checks are bypassed only by user intent expressed through the Open/Save panel: Oakley observed an Open-panel selection producing `Auth Right: Unknown (None), promptType: 1, DB Action:None` yet `authValue=1, authReason=5` and access granted — [Howard Oakley](https://eclecticlight.co/2026/04/08/privacy-files-folders-or-full-disk-access/).
- That Open-panel intent persisted across reboots and even survived deleting and reinstalling the app, and was **invisible** in Files & Folders; Oakley calls it a privacy flaw and the only way to rescind it was `tccutil reset All <bundle-id>` plus a restart — [Howard Oakley, comments](https://eclecticlight.co/2026/04/08/privacy-files-folders-or-full-disk-access/).

### Inferences
- A CLI *could* ship a tiny helper that shows an `NSOpenPanel`, has the user select `~/Documents`, and thereby obtain a persistent, prompt-free grant for that folder — this is the only "magic" no-prompt path. But it is undocumented, recorded under an unrelated service (`ScreenCapture` in Oakley's logs), invisible to the user in Settings, unrecoverable by `tccutil reset <service>`, and would need three panels (Documents, Downloads, Desktop). Not a foundation for a product; at best a fallback onboarding trick.
- Security-scoped bookmarks would only become relevant if Paper adopted App Sandbox (e.g. for Mac App Store distribution). They are then the correct mechanism for remembering user-selected folders, but they still require the initial user selection, so they do not remove the first prompt.
- `NSFileCoordinator` is irrelevant to the permission question; adding it would only add complexity.

### Gaps
- No source tests whether a *non-sandboxed* helper's `NSOpenPanel` selection produces the same durable, invisible grant as Oakley's sandboxed Insent. Plausible but unverified.
- Apple documents none of the intent-grant persistence behaviour.

## Distribution channels: Homebrew, cargo-dist, `rcodesign`

### Takeaway
Homebrew effectively mandates Developer ID + notarization for **casks**: it applies quarantine, audits executable artefacts for Gatekeeper compliance, and deprecates/removes casks that fail. For **formulae**, Homebrew builds bottles itself, so the installed binary is signed by Homebrew (ad-hoc), not by you — meaning a formula/bottle install can never carry your stable TCC identity. `cargo-dist` and `rcodesign` can produce the signature/notarization on any CI, including Linux.

### Cited Findings
- "Homebrew applies macOS quarantine attributes to Cask downloads so Gatekeeper performs these checks… Homebrew also audits apps, installers and other executable artefacts in official macOS casks that Gatekeeper can assess and requires them to pass on a default macOS configuration… Homebrew therefore deprecates, disables and eventually removes casks that fail Gatekeeper checks rather than normalising a security bypass." — [Homebrew Security and Supply Chain](https://docs.brew.sh/Homebrew-Security-and-Supply-Chain).
- Casks "install prebuilt applications and installers supplied directly by the upstream developer"; their `sha256` proves byte integrity but not authorship — [Homebrew Security and Supply Chain](https://docs.brew.sh/Homebrew-Security-and-Supply-Chain).
- Formulae are built from source by Homebrew's own sandboxed CI, and "all formula installations run without `sudo`" — [Homebrew Security and Supply Chain](https://docs.brew.sh/Homebrew-Security-and-Supply-Chain).
- Homebrew deliberately disclaims TCC responsibility for package-launched commands using a private API, so package code does not inherit the user's Full Disk Access — [Homebrew Security and Supply Chain](https://docs.brew.sh/Homebrew-Security-and-Supply-Chain). Community discussion frames granting FDA/App Management to a terminal as equivalent to disabling TCC — [Homebrew discussion #5378](https://github.com/orgs/Homebrew/discussions/5378).
- `cargo-dist` has an open design issue for first-class Apple code signing — [axodotdev/cargo-dist #1121](https://github.com/axodotdev/cargo-dist/issues/1121); current releases expose a `macos-sign` switch in `dist-workspace.toml` — [example config](https://github.com/s2-streamstore/tailsurf/blob/main/dist-workspace.toml).
- `rcodesign` (crate `apple-codesign`) signs Mach-O binaries, signs/notarizes/staples bundles and DMGs, and runs on non-Apple hosts including Linux and Windows — [Apple Codesign docs](https://gregoryszorc.com/docs/apple-codesign/stable/); [crates.io](https://crates.io/crates/apple-codesign).
- The standard CI shape is `codesign --force --options runtime --timestamp --identifier …` then `xcrun notarytool submit --wait` then `xcrun stapler staple` — [Dennis Babkin](https://dennisbabkin.com/blog/?t=how-to-get-certificate-code-sign-notarize-macos-binaries-outside-apple-app-store); [notarization troubleshooting](https://developer.apple.com/documentation/security/resolving-common-notarization-issues).

### Inferences
- For Paper, the channel determines whether a durable grant is even possible:
  - **curl installer** (downloads a CI-built, Developer ID/self-signed binary to a fixed path): durable. Best channel for the macOS grant.
  - **Homebrew cask** (`.app` or pkg in `/Applications`, notarized): durable, best UX, highest signing cost — and effectively mandatory to notarize because of the Gatekeeper audit.
  - **Homebrew formula/bottle**: not durable; the artifact is Homebrew's, and every `brew upgrade` replaces it. If Paper must support `brew install paper`, have the formula install a **stable launcher** (or point the LaunchAgent at a `paper`-managed copy under `~/.local/share/paper/`) that is signed/re-signed stably, rather than running the Cellar binary directly.
  - **`cargo install`**: always ad-hoc on the user's machine; cannot be durable unless the post-install step re-signs with a cert the user has. Document this honestly, or provide `paper setup --sign` (yabai-style).
  - **editor plugin**: see the responsible-process section; either install the agent under launchd or accept the editor's grant.
- Because Homebrew's own bottle is ad-hoc and versioned under `Cellar/paper/<version>/`, a LaunchAgent whose `Program` points at `$(brew --prefix)/bin/paper` will resolve through a symlink into a versioned path. That alone is sufficient to explain a grant that never matches after an upgrade, and is a likely contributor to Paper's `DB Action:None`.
- `rcodesign` makes the Developer ID route viable from a Linux GitHub runner, removing the "you need a Mac in CI" objection. It does not remove the $99/year and the notary credentials.

### Gaps
- Homebrew's cask docs do not state an explicit rule about apps that *require* TCC grants (only the Gatekeeper audit requirement). No source found prohibiting a cask whose app prompts for Documents access; many do.
- Exact current `cargo-dist` configuration keys for notarization were not verified (the book's signing page 404'd); the `macos-sign` flag is confirmed only by an example config. Verify against the current book before relying on it.
- `rcodesign`'s notarization support depends on an App Store Connect API key; the exact feature parity with `notarytool`/`stapler` on macOS 26 was not verified.

## Windows: Authenticode, SmartScreen reputation, self-signed acceptance, and unsigned background binaries

### Takeaway
Authenticode is necessary but no longer sufficient: since 2024, EV certificates no longer bypass SmartScreen, and every new signed file starts at "unrecognized" until reputation accumulates. The cheap, legitimate route for an individual is Azure Artifact Signing (~US$9.99/month, US/Canada individuals only) or an OV certificate (US$150–300/year, HSM/token required since June 2023). Self-signed is explicitly equivalent to unsigned for public users. winget does not require a signature to accept a manifest, but its validation pipeline runs Defender and SmartScreen checks, so an unsigned binary reliably causes friction.

### Cited Findings
- SmartScreen evaluates two signals: publisher reputation (is it signed, is the certificate known) and file-hash reputation. "When a file is not signed, SmartScreen reputation must build for each new version of your files, starting with zero reputation. Reputation cannot transfer from previous versions unless both were signed using the same publisher identity." — [Microsoft Learn](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation).
- Certificate behaviour on first download: Microsoft Store → no warning; valid OV/EV → warning ("unrecognized") with the verified publisher name shown; no signature → "Windows protected your PC", user must choose "Run anyway"; **self-signed → same behaviour as no signature** — [Microsoft Learn](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation).
- "EV certificates no longer bypass SmartScreen… Paying a premium for EV solely to avoid SmartScreen warnings is no longer justified." — [Microsoft Learn](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation); repeated in the signing-options table: EV is "same as OV since 2024" — [Microsoft Learn](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/code-signing-options).
- Cost and availability: Azure Artifact Signing ~US$9.99/month, no hardware token, identity validation required, individuals limited to USA and Canada; OV US$150–300/year with HSM/token mandatory since June 2023; EV US$400+/year — [Microsoft Learn](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/code-signing-options).
- Basics plan is $9.99/month for up to 5,000 signatures, $0.005 per additional; Premium $99.99/month for 100,000 — [Azure Artifact Signing pricing](https://azure.microsoft.com/en-us/products/artifact-signing).
- Smart App Control on Windows 11 "may supersede SmartScreen Application Reputation" and "will block execution of unsigned files unless the file has a positive reputation", for all executables, not just downloaded ones — [Microsoft Learn](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation).
- MSI/EXE submissions to the Store must be signed with a certificate chaining to the Microsoft Trusted Root Program; self-signed is not accepted — [Microsoft Learn](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/code-signing-options).
- winget does not sign anything: "WinGet still downloads/runs the underlying installer or portable code. A manifest does not sign it." — [microsoft/DataHub issue #77](https://github.com/microsoft/DataHub-App-for-Microsoft-Copilot-Agents-Cowork-and-Autopilots/issues/77). winget-pkgs validation includes Defender and SmartScreen checks against installer URLs (`Validation-Defender-Error`, `Validation-SmartScreen-Error`) — [msendpointmgr](https://msendpointmgr.com/2026/07/30/is-winget-enterprise-ready/).
- Open-source projects can get free OV-level signing through SignPath Foundation — [Microsoft Learn](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/code-signing-options).
- Microsoft's guidance now also states that if an app registers a packaged MCP server for Windows agents, production releases "must be signed with a certificate that is part of the Microsoft Trusted Root Program" — [Microsoft Learn](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/code-signing-options).

### Inferences
- For winget, a signed installer is not strictly required by the manifest schema, but it is required in practice: validation failures and the "unrecognized publisher" experience are recurring rejection/rollback causes. Signing with a Trusted Root-chaining cert is the only durable answer.
- Scoop and Chocolatey are far more permissive: both hash-check and will install unsigned binaries. Chocolatey's community repo moderation favours signed installers and flags "no checksum"/unsigned packages; Scoop does not verify signatures at all. Shipping unsigned there is possible but shifts the SmartScreen/Defender burden onto the user.
- A per-user background agent that (a) starts at login, (b) enumerates `Documents`, `Downloads`, `Desktop`, and (c) writes an index is behaviourally identical to commodity infostealers. Unsigned, that is a plausible Defender heuristic/ML trigger regardless of intent; a consistent publisher identity plus reputation is the only mitigation an individual can buy. This is inference from the threat model, not a documented Defender rule.
- Because reputation is per-publisher-identity, certificate rotation is expensive on Windows: it resets publisher reputation. Pick one identity and keep it for years. This is the opposite of the macOS self-signed model, where rotation only breaks local grants.
- Azure Artifact Signing's individual availability (US/Canada only) is a hard blocker for an individual developer elsewhere; OV with a token is the fallback, with a real hardware/HSM cost and worse CI ergonomics.

### Gaps
- No authoritative statement that winget *rejects* unsigned manifests outright; evidence points to validation warnings/errors and reputational friction rather than a hard schema rule. Verify against current winget-pkgs validation docs before relying on it.
- Defender/AV false-positive rates for unsigned Rust binaries that read user document folders were not measured; no source found.
- Chocolatey/Scoop moderation rules on unsigned packages were not verified from primary docs in this pass.

## Linux: what replaces signing, and does any channel require it?

### Takeaway
Nothing replaces signing at the OS level, because Linux has no equivalent of Gatekeeper/TCC for user-installed binaries. Integrity comes from the distribution channel (repo metadata signatures, Flatpak OSTree GPG signatures, Snap assertions) and confinement comes from systemd sandboxing plus opt-in AppArmor/SELinux. No mainstream channel requires the *upstream* binary to be signed; Flathub and the Snap Store require store-side signing/building instead.

### Cited Findings
- systemd user services can be confined with `ProtectSystem=`, `ProtectHome=`, `ReadOnlyPaths=`, `ReadWritePaths=`, `NoNewPrivileges=`, `PrivateTmp=`, `RestrictAddressFamilies=`, `SystemCallFilter=`, and audited with `systemd-analyze security` — [systemd.exec](https://www.freedesktop.org/software/systemd/man/latest/systemd.exec.html); practical summaries: [Red Hat](https://www.redhat.com/en/blog/systemd-secure-services), [SergeantBiggs](https://blog.sergeantbiggs.net/posts/hardening-applications-with-systemd/).
- Note the caveat that unprivileged per-user services cannot use all namespace/`Protect*` options — [systemd.exec](https://www.freedesktop.org/software/systemd/man/latest/systemd.exec.html).
- Flatpak/OSTree repositories are GPG-verified: "A signature is therefore required for every commit and for repository summary files" — [Flatpak: Distributing Applications](https://flatpak-testing.readthedocs.io/en/latest/distributing-applications.html).
- Snaps installed outside the store require `snap install --dangerous`; store-installed snaps are validated by store assertions — [ArchWiki: Snap](https://wiki.archlinux.org/title/Snap).
- There is no analogous requirement for a tarball, a `curl | sh` installer, a distro package, or a Homebrew-on-Linux bottle; distro packages are signed at the repository level, not per upstream binary. (General, widely documented; see the Flatpak/Snap contrast above.)

### Inferences
- On Linux, the TCC-equivalent problem simply does not exist: a user-level agent reading `$HOME/Documents` is governed by ordinary POSIX permissions. There is no grant to persist, hence no signing requirement for that purpose.
- The equivalent *trust* problem is packaging: an unsigned tarball is fine, but a systemd unit that runs as the user with full home access is a meaningful risk surface. The right deliverable is a unit with explicit `ReadOnlyPaths=%h/Documents %h/Downloads %h/Desktop`, `ProtectHome=read-only`, `ProtectSystem=strict`, `NoNewPrivileges=yes`, `PrivateTmp=yes`, and `RestrictAddressFamilies=AF_UNIX` (plus network only if needed).
- AppArmor/SELinux profiles are only worth shipping if Paper ever needs a root daemon. For a per-user agent they add packaging burden with little gain; distros rarely accept third-party profiles outside their own packaging.
- If Paper wants a "store" presence on Linux, Flatpak is the only channel that gives a real sandbox (and it needs portal-based file access, which is the Linux analogue of security-scoped bookmarks). Snap requires store-side signing and is Ubuntu-centric.

### Gaps
- No source was found that states a universal "no Linux channel requires upstream signatures" rule; it is inferred from the absence of such requirements in distro, Homebrew, Flathub and Snap documentation. Flatpak's requirement is on the *repository*, not the upstream binary, which is a meaningful distinction.
- AppArmor/SELinux enforcement specifics on mainstream distros (Ubuntu 24.04+, Fedora 40+) were not verified from primary docs in this pass.

## Consolidated recommendation (for the report writer)

1. **Do not attempt to persist a grant with the current ad-hoc, linker-signed binary.** Every rebuild produces `cdhash H"…"`; the requirement can never match after an upgrade. The observed `DB Action:None` is consistent with tccd declining to store a decision for an identity it cannot durably match, but confirm with the log/DB commands above before asserting causation.
2. **Fix the identity first, the packaging second.** Sign with a stable identifier using either a Developer ID Application certificate (US$99/year, notarizable, unlocks Homebrew cask) or a self-signed code-signing certificate (free, yabai-style, cargo/curl/plugin channels only). Both yield `identifier "com.kintsugi.paper" and certificate leaf = H"…"`, which survives rebuilds and upgrades.
3. **Keep the install path unversioned.** No `Cellar/<version>/`, no `versions/<semver>/`. `~/.local/share/paper/bin/paper` or `/Applications/Paper.app/Contents/MacOS/paper`. Point the LaunchAgent at the resolved path, not a Homebrew symlink. This is the failure mode that broke Claude Desktop even though it was Developer ID-signed and notarized.
4. **Ship a bundle ID, even for a CLI.** A minimal `.app` wrapper gives a friendly Privacy & Security entry, a usable FDA picker target, and a `tccutil reset` handle. It does not fix cdhash pinning on its own.
5. **Have the editor plugin install the LaunchAgent, not run the indexer as a child.** Otherwise the grant attaches to VS Code/Cursor and Paper never appears in Settings. Escaping that requires `responsibility_spawnattrs_setdisclaim`, a private API.
6. **Treat FDA as a documented fallback, not the default.** It is the same csreq problem with a much broader blast radius, and it makes Paper look like spyware. If used, grant it to a frozen, stably-signed access broker.
7. **On Windows, budget for one identity and keep it.** Azure Artifact Signing (~$120/year, US/Canada individuals) or OV ($150–300/year + token); EV buys nothing for SmartScreen since 2024. Self-signed is a non-starter for public distribution. Expect warnings for the first weeks regardless.
8. **On Linux, spend the effort on a hardened systemd unit instead of signing.** There is no grant to persist and no channel that requires an upstream signature; Flatpak/Snap are the only signed channels and they sign on their side.
