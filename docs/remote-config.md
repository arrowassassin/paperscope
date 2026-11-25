# Remote config (maintainer guide)

Installs follow the signed copy of `config/remote-config.toml` on the `remote-config` branch (fetched by the daemon only; search, CLI and MCP never touch the network). It is trusted only if `config/remote-config.toml.sig` verifies against a public key compiled into the binary (`config/keys/primary.pub` or `backup.pub`).

## Keys (once)

1. On your own machine: `scripts/gen-remote-config-keys.sh ~/paper-keys`. It writes `primary.pem` and `backup.pem` there and the hex public keys to `config/keys/*.pub`. It refuses to overwrite existing `.pem` files.
2. Keep the backup `.pem` offline. Commit only the `.pub` files. `*.pem` is gitignored; never commit a private key.
3. Public keys are 64 lowercase hex chars (raw Ed25519), one line. The `.sig` is base64 (no wrap) of the raw 64-byte signature over the exact bytes of the toml.

## One-time GitHub setup

1. Settings → Environments → New environment `config-signing`.
   - Required reviewers: yourself. Untick "Prevent self-review".
   - Deployment branches and tags: "Selected branches" → `main`.
   - Environment secret `REMOTE_CONFIG_SIGNING_KEY`: paste the whole contents of `~/paper-keys/primary.pem`, including the BEGIN/END lines.
2. Optional: a ruleset on `remote-config` that blocks deletion and force pushes. Don't restrict normal pushes: the signing workflow pushes there.

## Publishing a change

1. Edit `config/remote-config.toml` in a PR. Increase `version`, and push `expires` out. The PR check (`remote-config.yml`) confirms the file parses and the version went up.
2. Merge to main. `sign-remote-config.yml` starts and waits for your approval: Actions → the run → Review deployments → Approve.
3. After approval it signs with the environment key, verifies the result against `config/keys/*.pub`, and publishes the toml and `.sig` to the `remote-config` branch. That branch is what installs fetch. A merged but unapproved change is never served.

The workflow can also be started by hand (Actions → Sign remote config → Run workflow) to re-publish main's current file.

**Manual fallback** (for example while GitHub Actions is down): run `scripts/sign-remote-config.sh ~/paper-keys/primary.pem` on your machine, then push the toml and `.sig` to the `remote-config` branch. `scripts/verify-remote-config.sh` checks a signature locally, optionally with hex keys as arguments.

## Fields

- `schema`: must be `1`; anything else is ignored.
- `version`: integer; a config with a version lower than the cached one is ignored (no rollback).
- `expires`: `YYYY-MM-DD`; an expired config contributes nothing and never triggers or reverts a model switch.
- `[defaults]` `embedder`, `reranker`: model ids installs should use.
- `[search]` (optional): `rerank_top` 1..=100, `semantic_min_score` 0.0..=1.0.
- `[[models]]` (optional): extra models. `id` must not equal a built-in id; `arch` is `bert` (embedder, `dims = 384`) or `xlm-roberta-seq-cls` (reranker); the Hugging Face org must be `BAAI`, `intfloat`, `cross-encoder` or `sentence-transformers`; every file is pinned to a 40-hex commit and 64-hex sha256. Embedders may set `query_prefix` and `document_prefix` (each at most 64 chars); when absent they follow the Hugging Face org (`intfloat`: `query: ` / `passage: `; `BAAI`: the BGE query instruction and no document prefix).
- `[[deprecated]]` (optional): models to retire (`id`, `message`, optional `replacement`). Users who pinned a model locally keep it.

Any validation failure rejects the whole file and the previous good copy stays.

## Precedence and offline

Environment > local `config.toml` > remote > built-in. With `PAPER_OFFLINE` set the daemon never fetches; the cached or built-in values apply silently.

## Testing

Debug builds accept a public key from the env var `PAPER_REMOTE_CONFIG_TEST_PUBKEY` (64 hex). Release builds ignore it.
