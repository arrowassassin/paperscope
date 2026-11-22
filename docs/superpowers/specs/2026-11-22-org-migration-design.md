# Org migration design — kintsugi-tools

Date: 2026-11-22
Status: APPROVED — owner signed off on execution split and decisions
Owner: arrowassassin (org owner, `kintsugi-tools`)

## Intent and why

Three products (paperscope/"paper", kintsugi, oryn) live under the personal
`arrowassassin` account. The company brand is **Kintsugi** (display name),
the domain is `kintsugi.tools`, and a GitHub organization `kintsugi-tools`
now exists. This move consolidates all product repositories, the Homebrew
tap, and the website under the organization, so the products stop living in
a personal namespace and the website stops being entangled with a product
repo.

The user picked **Approach A — transfer & swap** in conversation.

## Goals

1. `paperscope`, `kintsugi`, `oryn`, `homebrew-tap` become org repos at
   `kintsugi-tools/*`, preserving stars, issues, releases, Actions history,
   and permanent GitHub redirects.
2. A new `kintsugi-tools/website` repo serves `kintsugi.tools` (GitHub
   Pages, custom domain moved from the `kintsugi` repo).
3. `site/` and `pages.yml` are deleted from both `paperscope` and
   `kintsugi` repos — no product repo carries the website.
4. A `kintsugi-tools/.github` repo provides the org profile README and
   default community files (SECURITY.md with the new contact email,
   CONTRIBUTING.md, issue templates).
5. Contact email everywhere becomes `kintsugi.tools@gmail.com`.
6. Every user-facing install path keeps working: `brew install
   arrowassassin/tap/paper` (via transfer redirects), the
   `releases/latest/download/...` asset URLs, the Claude plugin
   marketplace, and `kintsugi.tools/paper.sh`.
7. Paperscope **0.2.11** and a kintsugi metadata release ship as part of
   this migration, so crates.io metadata, the plugin/marketplace manifests,
   and the binaries' built-in URLs (paper's model downloads, kintsugi's
   self-updater) point at `kintsugi-tools` without waiting for
   feature-driven releases.

## Non-goals

- The website **redesign** (an "opendesign" flow the user will run
  separately). This migration seeds the website repo with the *current*
  site; the redesign later lands as a content-only swap.
- Moving `naadi` or any other personal repos.
- Renaming repositories (`paperscope` stays `paperscope`, etc.).
- Changing product code behavior. The only source-code edits are string
  constants (URLs, emails).
- Incorporating the business, trademark filing, Google Workspace.

## Anti-goals (failure even if "it works")

- `kintsugi.tools` going dark for longer than the single swap sitting, and
  any dark period without a rollback path.
- Any existing install breaking: brew tap redirects, installer one-liner,
  model download URLs (crates.io 0.2.10 metadata points at
  `kintsugi.tools/paperscope.html`; released binaries build
  `arrowassassin/paperscope` release URLs — those must keep resolving via
  redirects until the next crate release updates them).
- Losing release history or git history by re-creating repos instead of
  transferring.

## Constraints

- GitHub allows a custom domain on only **one** Pages site at a time: the
  `kintsugi.tools` domain must be removed from the `kintsugi` repo's Pages
  settings before being added to the `website` repo.
- DNS is hosted at **Squarespace** (`nsc1-4.squarespacedns.com`); the org
  owner holds access. Apex records do not change. Two DNS edits are in scope,
  both made by the owner during the swap sitting: (a) `www.kintsugi.tools`
  CNAME repoints from `arrowassassin.github.io` to
  `kintsugi-tools.github.io` — www currently 301s to the apex and must keep
  working; (b) a one-time ownership-verification TXT
  (`_github-pages-challenge-kintsugi-tools.kintsugi.tools`), because the
  domain is currently claimed by the personal account and GitHub will
  challenge the org's first claim.
- Org-level domain verification (org Settings → Pages → verified domains)
  is pre-staged **before** the repo-level swap, so the sitting itself has no
  DNS dependency left.
- Repo transfer requires the destination org to permit repo creation by the
  transferring user (org owner — fine).
- crates.io metadata for already-published versions is immutable: the
  0.2.10 crate page will keep showing `arrowassassin/paperscope` (which
  redirects). The remedy is in scope: **Phase 6 releases 0.2.11** with
  org-owned `repository` and download URLs.
- `paper.sh` (the short installer URL `kintsugi.tools/paper.sh`) and
  `install.sh` live inside the current site and must continue to resolve —
  they move with the site into the website repo.
- The `kintsugi` repo has untracked local files (`site/` is tracked and
  clean; untracked design zips and `.zed/` are not part of this move and
  stay untouched).

## Approaches considered

- **A — Transfer & swap (picked).** GitHub-transfer all four repos;
  seed the new website repo with today's site content; move the custom
  domain in one sitting; delete `site/` from the product repos; add
  `.github`; sweep references and emails. Risky plumbing happens once,
  verified immediately; the later redesign is content-only.
- **B — Transfer repos, website repo waits for the new design.** One
  fewer domain swap, but `site/` lingers in the `kintsugi` product repo
  and the custom-domain migration collides with the day the user deploys
  their redesign — the worst possible moment for Pages errors.
- **C — Fresh repos, re-push content.** Rejected: loses stars, issues,
  releases, redirects; every existing brew install, plugin add, and
  crates.io link 404s.

## The how (decided)

Phase order minimizes the window in which anything can break.

### Phase 0 — org settings

- Org contact email → `kintsugi.tools@gmail.com` (org profile, via
  `gh api -X PATCH orgs/kintsugi-tools`).
- Require 2FA for org members (org settings; the user should confirm it
  was set at creation — if not, this phase turns it on).

### Phase 1 — transfers (no user-visible change)

```bash
gh api -X POST repos/arrowassassin/paperscope/transfer -f new_owner=kintsugi-tools
gh api -X POST repos/arrowassassin/kintsugi/transfer  -f new_owner=kintsugi-tools
gh api -X POST repos/arrowassassin/oryn/transfer      -f new_owner=kintsugi-tools
gh api -X POST repos/arrowassassin/homebrew-tap/transfer -f new_owner=kintsugi-tools
```

Pages config and the custom domain carry over with the `kintsugi` repo, so
`kintsugi.tools` keeps serving throughout this phase. Local clones get new
remotes (`git remote set-url origin
git@github.com:kintsugi-tools/<repo>.git`).

### Phase 2 — website repo takes the domain

1. **Pre-stage verification (no downtime exposure):** owner adds the org's
   Pages verification TXT in Squarespace DNS; verify the domain at org
   level. This removes the DNS dependency from the swap itself.
2. Create `kintsugi-tools/website` (public). Copy the current
   `kintsugi/site/*` (including `CNAME`, `index.html`, `paperscope.html`,
   `about.html`, `paper.sh`, `install.sh`, css, svgs) as the initial
   content. Add a `pages.yml` workflow (equivalent of the current one) and
   `README.md`.
3. Push, wait for Pages deploy on the default
   `kintsugi-tools.github.io/website` URL, verify content there.
4. **Swap sitting (owner present, DNS editable):** remove the custom domain
   from the `kintsugi` repo's Pages settings; immediately add
   `kintsugi.tools` **and** `www.kintsugi.tools` in the `website` repo Pages
   settings; owner repoints the `www` CNAME to
   `kintsugi-tools.github.io`; enable HTTPS and wait for cert provisioning
   (can exceed "minutes" — the sitting, not a stopwatch, is the limit).
5. **Rollback lever:** if the new claim stalls, re-attach the domain to the
   `kintsugi` repo — Phase 3 has not yet deleted its `site/`, so the old
   site is still deployable. Only proceed to Phase 3 once the apex serves
   from the `website` repo.
6. Verify `https://kintsugi.tools` and `https://www.kintsugi.tools` (and
   `/paperscope.html`, `/paper.sh`, `/install.sh`) all return 200 from the
   new origin.

### Phase 3 — remove the website from product repos

- `kintsugi` repo: delete `site/` and `.github/workflows/pages.yml`.
- `paperscope` repo: delete `site/` (the stale duplicate whose html still
  says `security@arrowassassin.dev`) and its `pages.yml`.
- One commit per repo, message explains the website moved to
  `kintsugi-tools/website`.

### Phase 4 — `.github` org repo

- `profile/README.md`: Kintsugi — local-first AI tools; links to the three
  products and `kintsugi.tools`.
- Default community files, none of which exist today:
  `SECURITY.md` (security contact: `kintsugi.tools@gmail.com`),
  `CONTRIBUTING.md`, `SUPPORT.md`, issue/PR templates, Code of Conduct
  (contributor covenant).

### Phase 5 — reference and email sweep (one commit per repo)

| Repo | Changes |
|---|---|
| paperscope | `Cargo.toml` `repository` (+ homepage unchanged, it's the domain); `marketplace.json` owner name/urls; `plugin.json` author url (add `email`: kintsugi.tools@gmail.com if the schema field exists); plugin README (brew line → `kintsugi-tools/tap/paper`, marketplace line → `/plugin marketplace add kintsugi-tools/paperscope`, zip URL unchanged — release redirects); main README (CI badge, brew, marketplace, zip links); `docs/readme.md` same; `docs/CLAUDE_PLUGIN.md` if it references the old owner; `src/core/models/specs.rs` `download_urls` owner → `kintsugi-tools` (source change; released binaries keep working via redirects, ships with next release) |
| kintsugi | `Cargo.toml` `repository` → org, **`homepage` → `https://kintsugi.tools`** (current homepage is `arrowassassin.github.io/kintsugi/`, which 404s the moment Phase 3 deletes `site/`); `crates/kintsugi-cli/src/main.rs` `UPDATE_REPO` → `kintsugi-tools/kintsugi` plus the release/issues URLs at lines ~638/~1109; crate READMEs' repo links; `plugin/aegis/.claude-plugin/plugin.json` author email → new Gmail; `.claude-plugin/marketplace.json` owner email + urls |
| oryn | `Cargo.toml` `repository`/`homepage` → org URLs; README repo links. No site, no updater, no email refs — lowest-touch repo of the three |
| homebrew-tap | `Formula/paper.rb` `homepage` **stays** `https://kintsugi.tools/paperscope.html` (domain URLs are unchanged by this migration); class stays `Paper`. The four per-platform `url`s move to `kintsugi-tools/...` **from the 0.2.11 entry onward**; older versions keep old-owner urls, which redirect and are checksum-stable. |
| website | seed is **not** byte-identical: the owner-URL sweep applies to site content too — `paper.sh` `RELEASE_BASE`, `install.sh` `REPO="arrowassassin/kintsugi"`, `paperscope.html` brew + marketplace + release links, `index.html` download buttons, `og.svg`. String-only; the redesign remains a separate content swap. (Emails: the kintsugi/site source is clean; the stale `security@arrowassassin.dev` copies die with the paperscope/site deletion.) |
| org profile | `.github` repo carries the new email in SECURITY.md |

### Phase 6 — metadata releases: paperscope 0.2.11 and kintsugi

Released binaries ≤0.2.10 build `arrowassassin/paperscope` URLs into
`ModelSpec::download_urls`, and crates.io metadata for ≤0.2.10 is
immutable. Redirects cover those users, but the migration itself ships the
correction: after the Phase 5 sweep, cut **0.2.11** following the
established runbook:

1. Bump `Cargo.toml` / `Cargo.lock` / `plugin.json` / `marketplace.json`
   to 0.2.11; CHANGELOG entry ("repository moved to kintsugi-tools;
   download URLs and metadata now org-owned").
2. Website version chip → 0.2.11 (one-line edit in the website repo).
3. Full local gates (fmt, clippy all variants, tests, NOTICE diff).
4. `cargo publish` → crates.io 0.2.11 metadata shows
   `repository: github.com/kintsugi-tools/paperscope`.
5. Tag `v0.2.11`, release workflow (SBOM-green since 0.2.10), attach
   `paperscope-plugin.zip`, bump the tap formula to 0.2.11.
6. Verify `cargo search paperscope` → 0.2.11 and a fresh
   `curl -fsSL https://kintsugi.tools/paper.sh | sh` installs it.

This release carries no feature changes — it exists because the org move
must be visible in the package index, not only in redirects.

**Kintsugi gets the same treatment (owner picked option (a)).** Its
already-published crate versions point their homepage at
`arrowassassin.github.io/kintsugi/`, which 404s after Phase 3, and its CLI
self-updater is hard-coded to the old owner. Immediately after the
paperscope release, cut a kintsugi release with no feature changes:
`Cargo.toml` repository → org, homepage → `https://kintsugi.tools`,
`UPDATE_REPO` → `kintsugi-tools/kintsugi`, CLI-printed release/issues URLs,
crate README links, plugin manifest emails; bump per its own runbook and
publish, so the crates.io Homepage link for the new version is live.

### Phase 7 — verification

- `git ls-remote` on old URLs redirects (transfer redirects work for git).
- `brew install arrowassassin/tap/paper` still resolves (tap redirect);
  new form `brew install kintsugi-tools/tap/paper` works.
- `curl -I https://kintsugi.tools/paperscope.html` → 200 from website repo.
- `curl -fsSL https://kintsugi.tools/paper.sh | sh -n` (syntax-check only
  in verification; a real install was already done at 0.2.10).
- `claude plugin validate` still passes; existing marketplace installs
  keep functioning via redirect, with a note to re-add
  `kintsugi-tools/paperscope` for future updates.
- `gh api repos/arrowassassin/paperscope` → redirects to
  `kintsugi-tools/paperscope` (HTTP 301 semantics).
- Model download URLs from the released binary still resolve via redirect.

## Execution model

The migration is **agent-executed** via the authenticated `gh` CLI
(scopes `repo`, `workflow`): repo transfers, website repo creation, pushes,
Pages settings via API where available, reference sweeps, releases, tap
bump, verification. Three actions are the owner's:

1. **Org contact email** — needs `admin:org`; owner either grants it once
   (`gh auth refresh -h github.com -s admin:org`) or sets Organization
   profile → Contact email in the UI.
2. **Squarespace DNS** — the verification TXT (pre-staged) and the `www`
   CNAME repoint, during the swap sitting.
3. **Presence during the swap sitting** — the rollback lever requires a
   human who can watch `kintsugi.tools` and call it.

## Builder check

A reviewer pass ran against this document and the live
domain/DNS/clones. Its findings are folded in: the `www` subdomain and its
CNAME, the Squarespace TXT challenge on the org's first domain claim, the
rollback lever, TLS-provisioning time being bounded by the sitting, the
site-content URL sweep, and the tap-homework correction.

## What's left to the builder

- Exact `pages.yml` content for the website repo (copy semantics from the
  kintsugi repo's current workflow).
- Whether `plugin.json`'s schema has an author `email` field to fill.
- Wording of the `.github` profile README and community files.
- The precise moment to remove the old custom domain (immediately before
  adding the new one, within the same sitting).

## Owner decisions (final)

1. **DNS and Pages domain (answered):** the owner — the only person with
   Squarespace access — removes the custom domain from the `kintsugi`
   repo's Pages settings and repoints the `www` CNAME themselves, after
   the agent completes the transfers and repo creation. The agent does
   every repo-side swap, creates all new repos, and notifies the owner at
   the exact moment their DNS step is due.
2. **LICENSE copyright lines (deferred by default):** keep "Copyright (c)
   2026 arrowassassin" until incorporation — copyright belongs to the human
   author and no legal entity exists to reassign it to. Revisit the day
   the company is registered.
