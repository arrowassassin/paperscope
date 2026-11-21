# Paper packaging guide

This document describes what is actually shipped, how it is built, and what is
**not** built. It is written against the current pipeline (`dist` 0.34 +
`dist-workspace.toml`), not against an aspiration.

## The two distribution channels

| Channel | What users get | Built by |
| --- | --- | --- |
| crates.io | `cargo install paperscope` → the `paper` binary (and a `paper` library) | `cargo publish` |
| GitHub Releases | per-platform archives, installers, checksums, SBOMs | `dist` (cargo-dist 0.34) in CI |

### crates.io

The crate is `paperscope`; the binary and library targets are both named
`paper`, so `cargo install paperscope` installs a `paper` command.

- **MSRV:** 1.91 (`rust-version` in `Cargo.toml`).
- **Runtime dependencies:** none. SQLite is compiled in (`rusqlite` with
  `bundled`) and `sqlite-vec` is statically linked, so there is no system
  `libsqlite3` to install. The only dynamic dependency is libc on the glibc
  targets; the musl build is fully static.
- **Optional external tools:** `pdftotext`/`pandoc` (feature `shell-accel`) and
  Tesseract + `pdftoppm` (feature `ocr`). Both are optional accelerators; the
  pure-Rust extractors work without them.
- **Package size:** see `docs/implementation-status.md` for the measured
  `cargo publish --dry-run` numbers.

```bash
cargo publish --dry-run     # verify the crate builds from the packaged tarball
cargo publish               # needs `cargo login` and a crates.io 2FA token
```

### GitHub Releases

`dist` builds one archive per target plus the installers:

| Platform | Archive |
| --- | --- |
| Linux x86_64 (glibc) | `paperscope-x86_64-unknown-linux-gnu.tar.xz` |
| Linux x86_64 (musl, static) | `paperscope-x86_64-unknown-linux-musl.tar.xz` |
| Linux aarch64 | `paperscope-aarch64-unknown-linux-gnu.tar.xz` |
| macOS x86_64 | `paperscope-x86_64-apple-darwin.tar.xz` |
| macOS aarch64 | `paperscope-aarch64-apple-darwin.tar.xz` |
| Windows x86_64 | `paperscope-x86_64-pc-windows-msvc.zip` |

plus:

- `source.tar.gz` — the crate source.
- `sha256.sum` and a `.sha256` file per artifact.
- `paperscope-installer.sh` — POSIX shell installer.
- `paperscope-installer.ps1` — PowerShell installer.
- `paperscope.rb` — Homebrew formula.
- `paperscope-npm-package.tar.gz` — npm package.
- `sbom.json` (CycloneDX) and `sbom.spdx.json` (SPDX) — attached by the
  `sbom` job.

## The release pipeline

`dist-workspace.toml` is the source of truth for the release. The workflow file
is **generated**, so hand edits to `.github/workflows/release.yml` are lost on
the next regeneration.

```bash
scripts/regenerate-release-ci.sh
```

That script:

1. Temporarily lifts `allow-dirty = ["ci"]` (dist refuses to write while it is
   set, because the committed workflow differs from the generated one).
2. Runs `dist generate --mode=ci`.
3. Re-applies `scripts/patch-release-ci.sh`, which adds back the two things
   dist cannot know about:
   - the signing secrets (`CODESIGN_*`) as workflow environment variables, and
   - the `sbom` job that attaches the two SBOMs to the release.

The generated workflow has these jobs:

| Job | What it does |
| --- | --- |
| `plan` | Installs the pinned `dist` version, computes the build matrix, and (on a tag) creates the GitHub Release. |
| `build-local-artifacts` | Builds the per-target binaries and archives on the matching runner. |
| `build-global-artifacts` | Builds the platform-independent installers (shell, PowerShell, Homebrew, npm). |
| `host` | Uploads the artifacts to the GitHub Release and writes the manifest. |
| `sbom` | Generates the CycloneDX and SPDX SBOMs and attaches them. (Added by our patch.) |
| `announce` | Writes the release title and body from the changelog. |

## What is deliberately not produced

These are **not** built today, and no documentation should claim otherwise:

- **`.deb`, `.rpm`, `.apk`** — `dist` does not build Linux distribution
  packages. Use `cargo install paperscope`, the musl tarball, or the shell
  installer. A distro package would need a separate tool (`cargo-deb`,
  `cargo-generate-rpm`) and its own CI job.
- **AppImage** — not built.
- **MSI / Apple `.pkg`** — not enabled in `installers`.
- **Code signing and notarization** — the workflow maps the `CODESIGN_*`
  secrets, but no certificate is configured, so the artifacts are currently
  **unsigned**. They are checksummed (`sha256.sum`), which is what a user can
  verify today.
- **SLSA provenance** — not generated. GitHub's own build attestation is not
  wired up either.
- **Homebrew tap and npm publish jobs** — `publish-jobs` is empty. The formula
  and the npm tarball are attached to the release for manual publishing.

## Verifying a download

```bash
sha256sum -c sha256.sum          # Linux
shasum -a 256 -c sha256.sum      # macOS
```

On macOS, a freshly downloaded unsigned binary may be quarantined:

```bash
xattr -d com.apple.quarantine paper
```

## Naming

The crate is `paperscope`; the product, library and binary are `paper`. Release
artifacts are named after the crate (`paperscope-…`), which is what `dist`
derives from the package name.
