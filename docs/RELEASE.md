# Release Process

This document describes how to create a signed, multi-platform release for Paper.

## Prerequisites

### Required Secrets (GitHub Repository Settings → Secrets and variables → Actions)

#### macOS Code Signing & Notarization
| Secret | Description | How to Obtain |
|--------|-------------|---------------|
| `APPLE_CERTIFICATE` | Base64-encoded `.p12` Developer ID Application certificate | Export from Keychain Access (File → Export), then `base64 -i certificate.p12 -o -` |
| `APPLE_CERT_PASSWORD` | Password for the `.p12` file | The password you set when exporting |
| `APPLE_TEAM_ID` | 10-character Team ID (e.g., `ABCD123456`) | Apple Developer Account → Membership |
| `APPLE_API_KEY` | App Store Connect API Key ID (e.g., `ABC123DEFG`) | App Store Connect → Users and Access → Keys |
| `APPLE_API_ISSUER` | App Store Connect API Key Issuer ID (UUID) | App Store Connect → Users and Access → Keys |

#### Windows Code Signing
| Secret | Description | How to Obtain |
|--------|-------------|---------------|
| `WINDOWS_CERT_FILE` | Base64-encoded `.pfx` EV Code Signing certificate | Export from certificate provider, then `base64 -i certificate.pfx -o -` |
| `WINDOWS_CERT_PASSWORD` | Password for the `.pfx` file | The password you set when exporting |

#### Linux GPG Signing
| Secret | Description | How to Obtain |
|--------|-------------|---------------|
| `GPG_PRIVATE_KEY` | ASCII-armored private key (`gpg --export-secret-keys --armor <key-id>`) | Your GPG key |
| `GPG_PASSPHRASE` | Passphrase for the GPG private key | Your GPG key passphrase |

## Creating a Release

### Automated (Recommended)

Push a version tag:

```bash
git tag v0.2.0
git push origin v0.2.0
```

This triggers the `release.yml` workflow which:
1. Builds for all 6 targets via `cargo-dist`
2. Signs macOS binaries (codesign + notarize + staple)
3. Signs Windows binaries (signtool with timestamp)
4. Creates detached GPG signatures for Linux artifacts
5. Generates SBOMs (CycloneDX + SPDX)
6. Generates SLSA Provenance (Level 3)
7. Uploads all artifacts to GitHub Releases

### Local Test Build

```bash
# Install cargo-dist
cargo binstall cargo-dist --locked

# Build tarball artifacts locally (no signing)
cargo dist build --artifacts=tarball

# Artifacts appear in target/dist/
ls -la target/dist/
```

Expected output:
```
target/dist/
├── paperscope-v0.1.0-x86_64-unknown-linux-gnu.tar.gz
├── paperscope-v0.1.0-x86_64-unknown-linux-musl.tar.gz
├── paperscope-v0.1.0-aarch64-unknown-linux-gnu.tar.gz
├── paperscope-v0.1.0-x86_64-pc-windows-msvc.zip
├── paperscope-v0.1.0-x86_64-apple-darwin.tar.gz
├── paperscope-v0.1.0-aarch64-apple-darwin.tar.gz
├── paperscope-v0.1.0-x86_64-unknown-linux-gnu.deb
├── paperscope-v0.1.0-x86_64-unknown-linux-gnu.rpm
├── paperscope-v0.1.0-x86_64-unknown-linux-gnu.apk
├── homebrew/
│   └── paperscope.rb
├── scoop/
│   └── paperscope.json
├── npm/
│   └── paperscope-v0.1.0.tgz
├── sbom.json
├── sbom.spdx.json
└── *.intoto.jsonl
```

## Verifying Artifacts

### macOS
```bash
# Verify code signature
codesign --verify --deep --strict --verbose=2 paper

# Verify notarization
spctl --assess --type execute --verbose paper
```

### Windows
```powershell
# Verify signature
signtool verify /pa /v paper.exe
```

### Linux (GPG)
```bash
# Verify detached signature
gpg --verify paperscope-v0.1.0-x86_64-unknown-linux-gnu.tar.gz.asc paperscope-v0.1.0-x86_64-unknown-linux-gnu.tar.gz
```

### SBOM
```bash
# View CycloneDX SBOM
jq . target/dist/sbom.json

# View SPDX SBOM
jq . target/dist/sbom.spdx.json
```

## Troubleshooting

### macOS Notarization Fails
- Ensure `APPLE_API_KEY` and `APPLE_API_ISSUER` are correct
- Check that the certificate is "Developer ID Application" (not Mac App Store)
- Verify the binary has the `--options runtime` flag

### Windows Signtool Fails
- Ensure the `.pfx` is an EV certificate (required for SmartScreen)
- Check timestamp server reachability (`http://timestamp.digicert.com`)

### GPG Signing Fails
- Ensure `GPG_PRIVATE_KEY` includes the full ASCII-armored key (public + private)
- Verify `GPG_PASSPHRASE` matches the key

### cargo-dist Build Fails
- Run `cargo check` and `cargo test` locally first
- Ensure all targets are installed: `rustup target add x86_64-unknown-linux-musl aarch64-unknown-linux-gnu x86_64-pc-windows-msvc x86_64-apple-darwin aarch64-apple-darwin`

## Release Checklist

- [ ] All tests pass: `cargo test`
- [ ] No warnings: `cargo check`
- [ ] Licenses clean: `cargo deny check licenses`
- [ ] CHANGELOG.md updated
- [ ] Version bumped in Cargo.toml
- [ ] Tag pushed: `git push origin vX.Y.Z`
- [ ] GitHub Release created with all artifacts
- [ ] Homebrew formula updated (automatic via cargo-dist)
- [ ] Scoop manifest updated (automatic via cargo-dist)
- [ ] npm package published (automatic via cargo-dist)
- [ ] SBOMs attached to release
- [ ] SLSA provenance attached to release