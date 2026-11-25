# Remote config (maintainer guide)

Installs follow `config/remote-config.toml` (fetched by the daemon only; search, CLI and MCP never touch the network). It is trusted only if `config/remote-config.toml.sig` verifies against a public key compiled into the binary (`config/keys/primary.pub` or `backup.pub`).

## Keys (once)

1. On your own machine: `scripts/gen-remote-config-keys.sh ~/paper-keys`. It writes `primary.pem` and `backup.pem` there and the hex public keys to `config/keys/*.pub`. It refuses to overwrite existing `.pem` files.
2. Keep the backup `.pem` offline. Commit only the `.pub` files. `*.pem` is gitignored; never commit a private key.
3. Public keys are 64 lowercase hex chars (raw Ed25519), one line. The `.sig` is base64 (no wrap) of the raw 64-byte signature over the exact bytes of the toml.

## Publishing a change

1. Edit `config/remote-config.toml`, increase `version`, push `expires` out.
2. `scripts/sign-remote-config.sh ~/paper-keys/primary.pem` (also verifies the result).
3. Commit the toml and `.sig` together and push to main. CI (`remote-config.yml`) runs `scripts/verify-remote-config.sh`: it passes with a notice while both `.pub` files are empty, and fails if a key exists and the `.sig` is missing or does not verify. Run the same script locally, optionally with hex keys as arguments.

## Fields

- `schema`: must be `1`; anything else is ignored.
- `version`: integer; a config with a version lower than the cached one is ignored (no rollback).
- `expires`: `YYYY-MM-DD`; an expired config contributes nothing and never triggers or reverts a model switch.
- `[defaults]` `embedder`, `reranker`: model ids installs should use.
- `[search]` (optional): `rerank_top` 1..=100, `semantic_min_score` 0.0..=1.0.
- `[[models]]` (optional): extra models. `id` must not equal a built-in id; `arch` is `bert` (embedder, `dims = 384`) or `xlm-roberta-seq-cls` (reranker); the Hugging Face org must be `BAAI`, `intfloat`, `cross-encoder` or `sentence-transformers`; every file is pinned to a 40-hex commit and 64-hex sha256.
- `[[deprecated]]` (optional): models to retire (`id`, `message`, optional `replacement`). Users who pinned a model locally keep it.

Any validation failure rejects the whole file and the previous good copy stays.

## Precedence and offline

Environment > local `config.toml` > remote > built-in. With `PAPER_OFFLINE` set the daemon never fetches; the cached or built-in values apply silently.

## Testing

Debug builds accept a public key from the env var `PAPER_REMOTE_CONFIG_TEST_PUBKEY` (64 hex). Release builds ignore it.
