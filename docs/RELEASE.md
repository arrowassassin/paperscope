# Release process

This is the runbook for a Paper release. It describes the pipeline that exists
today; optional steps that are not wired up are marked as such.

## 1. Prepare

```bash
cargo +1.94.0 test                       # MSRV build
cargo +stable test                       # current stable
cargo +stable clippy --all-targets --all-features -- -D warnings
cargo +stable clippy --no-default-features --all-targets -- -D warnings
cargo deny check licenses bans sources
cargo audit
cargo publish --dry-run
scripts/regenerate-release-ci.sh         # only if the release workflow changed
actionlint
```

Then:

- bump `version` in `Cargo.toml`,
- update `CHANGELOG.md`,
- commit and push to `main`.

`CHANGELOG.md` is what the `announce` job turns into the release body, so a
missing entry means an empty release description.

## 2. Tag

```bash
git tag v0.1.0
git push origin v0.1.0
```

The tag must look like `v0.1.0` (the generated workflow matches
`**[0-9]+.[0-9]+.[0-9]+*`). A tag with a pre-release suffix
(`v0.1.0-prerelease.1`) is published as a GitHub prerelease.

Pushing the tag starts `.github/workflows/release.yml`:

| Job | What it does |
| --- | --- |
| `plan` | Installs the pinned `dist` (0.34.0), computes the build matrix, creates the GitHub Release. |
| `build-local-artifacts` | Builds each target on its runner and uploads the archives. |
| `build-global-artifacts` | Builds the shell, PowerShell, Homebrew and npm installers. |
| `host` | Uploads everything to the release and writes the artifact manifest. |
| `sbom` | Generates the CycloneDX + SPDX SBOMs and attaches them. |
| `announce` | Sets the release title and body from the changelog. |

Watch it with:

```bash
gh run watch
gh release view v0.1.0 --json assets
```

## 3. Verify the release

```bash
gh release download v0.1.0 --dir /tmp/paper-release
cd /tmp/paper-release && sha256sum -c sha256.sum
```

Spot-check one binary per platform family:

```bash
./paper-x86_64-unknown-linux-musl.tar.xz   # extract and run `paper --version`
```

The archive contains the `paper` binary plus `README.md`, `LICENSE-MIT`,
`LICENSE-APACHE`, `CHANGELOG.md` and `docs/`.

## 4. Publish to crates.io

This is a manual step and needs the maintainer's token:

```bash
cargo login
cargo publish --dry-run     # once more, on the tagged tree
cargo publish
```

crates.io may ask for a one-time 2FA code. After publishing:

```bash
cargo install paperscope --version 0.1.0   # smoke test the published crate
```

## 5. Publish the Homebrew formula and npm package (optional)

`dist` attaches `paperscope.rb` and `paperscope-npm-package.tar.gz` to the
release, but no publish job runs. Publishing them is manual:

- **Homebrew:** copy `paperscope.rb` into your tap repository and open a
  pull request against `homebrew/homebrew-core`.
- **npm:** `npm publish paperscope-npm-package.tar.gz --access public`.

Until a tap and an npm token exist, `publish-jobs` in `dist-workspace.toml`
stays empty.

## Signing (not enabled today)

The workflow maps these secrets as environment variables, but no certificate is
configured, so the artifacts are **unsigned**:

| Secret | Used by | Purpose |
| --- | --- | --- |
| `CODESIGN_IDENTITY` | `dist` (macOS) | Developer ID Application identity. |
| `CODESIGN_CERTIFICATE` | `dist` (macOS) | Base64 `.p12` certificate. |
| `CODESIGN_CERTIFICATE_PASSWORD` | `dist` (macOS) | Password for that certificate. |
| `CODESIGN_OPTIONS` | `dist` (macOS) | Extra `codesign` flags, e.g. `--options runtime`. |

To turn macOS signing on: create a Developer ID Application certificate,
base64 it, store it as `CODESIGN_CERTIFICATE`, and set the other two secrets.
`dist` then signs (and can notarize) the macOS binaries on the macOS runner.
Windows signing in `dist` 0.34 goes through the Azure or SSL.com signing
config rather than a raw `.pfx` secret.

Without signing, users verify the artifacts with `sha256.sum`, which is always
generated.

## SBOM

The `sbom` job generates:

- `sbom.json` — CycloneDX, from `cargo-cyclonedx`,
- `sbom.spdx.json` — SPDX 2.3, from `cargo-sbom`.

Both are attached to the GitHub Release. The same job runs on every tag; it
does not run on pull requests.

## Troubleshooting

**`dist` refuses to regenerate the workflow**
`allow-dirty = ["ci"]` is set because the committed workflow carries our
patch. Use `scripts/regenerate-release-ci.sh`, which lifts the flag for the
duration of the generation.

**A build job fails for one target**
Re-run just that job from the GitHub UI. `fail-fast` is disabled, so the other
targets still build.

**`cargo publish` rejects the crate**
Check the output of `cargo publish --dry-run`: it lists exactly which files
would ship and compiles them. A missing `docs/readme.md` or a stale
`Cargo.lock` is the usual cause.

**The release has no artifacts**
The `plan` job computes the matrix; if it failed, no build job runs. Read its
log first.

## Release checklist

- [ ] `cargo test` green on stable and on the MSRV (1.94)
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` green
- [ ] `cargo clippy --no-default-features --all-targets -- -D warnings` green
- [ ] `cargo deny check licenses bans sources` green
- [ ] `cargo audit` green (only the allow-listed advisories)
- [ ] `cargo publish --dry-run` green
- [ ] `actionlint` clean
- [ ] `version` bumped in `Cargo.toml`
- [ ] `version` bumped in `paperscope-plugin/.claude-plugin/plugin.json` and
      `.claude-plugin/marketplace.json` (both entries)
- [ ] `version` bumped in `README.md` and `docs/readme.md` ("Latest release"),
      and in the Homebrew tap's `Formula/paper.rb` after the tag
- [ ] `CHANGELOG.md` updated (dated section, not `Unreleased`)
- [ ] npm published (`npm publish` from the release's npm tarball)
- [ ] Tag pushed (`git push origin vX.Y.Z`)
- [ ] Release assets present, `sha256sum -c sha256.sum` passes
- [ ] SBOMs attached
- [ ] `cargo publish` done and the published crate installs
