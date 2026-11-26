# Remote config (maintainer guide)

Installs follow the signed copy of `config/remote-config.toml` on the `remote-config` branch (fetched by the daemon only; search, CLI and MCP never touch the network). It is trusted only if `config/remote-config.toml.sig` verifies against a public key compiled into the binary (`config/keys/primary.pub` or `backup.pub`).

## Keys (once)

1. On your own machine: `scripts/gen-remote-config-keys.sh ~/paper-keys`. It writes `primary.pem` and `backup.pem` there and the hex public keys to `config/keys/*.pub`. It refuses to overwrite existing `.pem` files.
2. Keep the backup `.pem` offline. Commit only the `.pub` files. `*.pem` is gitignored; never commit a private key.
3. Public keys are 64 lowercase hex chars (raw Ed25519), one line. The `.sig` is base64 (no wrap) of the raw 64-byte signature over the exact bytes of the toml.

## One-time GitHub setup

Release secrets: `HOMEBREW_TAP_DEPLOY_KEY` (repository secret: the private half of an SSH deploy key added with write access under `kintsugi-tools/homebrew-tap` → Settings → Deploy keys; `scripts/patch-release-ci.sh` wires it into the generated workflow) and `CARGO_REGISTRY_TOKEN` (`config-signing` environment secret; crates.io token scoped to publishing `paperscope`).

1. Settings → Environments → New environment `config-signing`.
   - Required reviewers: yourself. Untick "Prevent self-review".
   - Deployment branches and tags: "Selected branches" → `main`.
   - Environment secret `REMOTE_CONFIG_SIGNING_KEY`: paste the whole contents of `~/paper-keys/primary.pem`, including the BEGIN/END lines.
2. Optional: a ruleset on `remote-config` that blocks deletion and force pushes. Don't restrict normal pushes: the signing workflow pushes there.

## Publishing a change

1. Edit `config/remote-config.toml` in a PR. Increase `version`, and push `expires` out. The PR check (`remote-config.yml`) confirms the file parses and the version went up.
2. Merge to main. `sign-remote-config.yml` starts and waits for your approval: Actions → the run → Review deployments → Approve.
3. After approval it signs with the environment key, verifies the result against `config/keys/*.pub`, and publishes the toml and `.sig` to the `remote-config` branch. That branch is what installs fetch. A merged but unapproved change is never served.

### Per-release flow

1. Tag the release from a terminal: `git tag -a vX.Y.Z origin/main -m "Paper vX.Y.Z" && git push origin vX.Y.Z`. Do not create the release in the GitHub UI: `release.yml` creates it and fails if one already exists. `release.yml` builds and publishes the GitHub Release and the Homebrew formula; `publish-crate.yml` then waits for your approval (same `config-signing` environment) and publishes the crate to crates.io.
2. Once the release is live everywhere, open a PR that sets `[update] version` to the new release and raises `version` in `config/remote-config.toml`. Merge it and approve the signing run. Installs only update to the version named here, so nothing rolls out before you approve it.

The workflow can also be started by hand (Actions → Sign remote config → Run workflow) to re-publish main's current file.

**Manual fallback** (for example while GitHub Actions is down): run `scripts/sign-remote-config.sh ~/paper-keys/primary.pem` on your machine, then push the toml and `.sig` to the `remote-config` branch. `scripts/verify-remote-config.sh` checks a signature locally, optionally with hex keys as arguments.

## Fields

- `schema`: must be `1`; anything else is ignored.
- `version`: integer; a config with a version lower than the cached one is ignored (no rollback).
- `expires`: `YYYY-MM-DD`; an expired config contributes nothing and never triggers or reverts a model switch.
- `[defaults]` `embedder`, `reranker`: model ids installs should use.
- `[search]` (optional): `rerank_top` 1..=100, `semantic_min_score` 0.0..=1.0.
- `[[models]]` (optional): extra models. `id` must not equal a built-in id; `arch` is `bert` (embedder, `dims = 384`) or `xlm-roberta-seq-cls` (reranker); the Hugging Face org must be `BAAI`, `intfloat`, `cross-encoder` or `sentence-transformers`; every file is pinned to a 40-hex commit and 64-hex sha256. Embedders may set `query_prefix` and `document_prefix` (each at most 64 chars); when absent they follow the Hugging Face org (`intfloat`: `query: ` / `passage: `; `BAAI`: the BGE query instruction and no document prefix).
- `[update]` (optional): `version` (`MAJOR.MINOR.PATCH`, digits only) is the release you approve for auto-update: the daemon installs exactly this version through the install method, and only when it is newer than the running one. Optional `notes` (at most 200 chars) is shown with the update.
- `[[deprecated]]` (optional): models to retire (`id`, `message`, optional `replacement`). Users who pinned a model locally keep it.

Any validation failure rejects the whole file and the previous good copy stays.

## Precedence and offline

Environment > local `config.toml` > remote > built-in. With `PAPER_OFFLINE` set the daemon never fetches; the cached or built-in values apply silently.

## Testing

Debug builds accept a public key from the env var `PAPER_REMOTE_CONFIG_TEST_PUBKEY` (64 hex). Release builds ignore it.
