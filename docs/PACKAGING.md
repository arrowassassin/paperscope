# Paper Packaging Guide

This document describes how to build and distribute Paper for various platforms.

## Supported Platforms

- **Linux**: x86_64 (glibc), x86_64 (musl), aarch64
- **macOS**: x86_64, aarch64 (Apple Silicon)
- **Windows**: x86_64 (MSVC)

## Distribution Formats

### 1. Cargo-dist (Primary)

Paper uses `cargo-dist` for automated cross-platform builds and distribution.

```bash
# Install cargo-dist
cargo install cargo-dist

# Build for all targets (runs in CI)
cargo dist build --artifacts=all

# Build locally for current platform
cargo dist build
```

Artifacts produced:
- `tarball` (`.tar.xz`) - Universal archive
- `zip` - Windows-friendly archive
- `deb` - Debian/Ubuntu package
- `rpm` - Fedora/RHEL package
- `apk` - Alpine package
- `shell installer` (`.sh`) - curl | sh installer
- `Homebrew formula` - For macOS
- `Scoop manifest` - For Windows
- `npm package` - For Node.js environments

### 2. Homebrew (macOS)

Formula: `packaging/homebrew/paper.rb`

```bash
# Test locally
brew install --build-from-source packaging/homebrew/paper.rb

# After release, update sha256 in formula and submit to homebrew-core
```

### 3. Scoop (Windows)

Manifest: `packaging/scoop/paper.json`

```bash
# Test locally
scoop install packaging/scoop/paper.json

# After release, update hash and submit to scoop main bucket
```

### 4. Debian/Ubuntu (.deb)

Build:
```bash
cd packaging/deb
dpkg-buildpackage -b -us -uc
```

Install:
```bash
sudo dpkg -i ../paper_0.1.0-1_amd64.deb
sudo apt-get install -f  # Fix any missing dependencies
```

### 5. Fedora/RHEL (.rpm)

Build:
```bash
cd packaging/rpm
rpmbuild -bb paper.spec
```

Install:
```bash
sudo dnf install ~/rpmbuild/RPMS/x86_64/paper-0.1.0-1.x86_64.rpm
```

### 5. AppImage (Universal Linux)

Build:
```bash
# Install appimagetool
wget https://github.com/AppImage/AppImageKit/releases/download/continuous/appimagetool-x86_64.AppImage
chmod +x appimagetool-x86_64.AppImage

# Build AppImage
cd packaging/appimage
mkdir -p AppDir/usr/bin
cp ../../target/release/paper AppDir/usr/bin/
cp paper.desktop AppDir/
cp AppRun AppDir/
chmod +x AppRun
../../appimagetool-x86_64.AppImage AppDir paper-x86_64.AppImage
```

## Release Process

1. **Tag Release**: `git tag v0.1.0 && git push origin v0.1.0`
2. **CI Builds**: GitHub Actions runs `cargo dist build` on all platforms
3. **Artifacts Uploaded**: `cargo dist` uploads all artifacts to GitHub Releases
4. **Verify**: Test each artifact on target platforms
5. **Homebrew/Scoop**: Submit updated formulas/manifests to respective repositories
6. **Linux Distros**: Submit .deb/.rpm to respective repositories (optional)

## Binary Naming Convention

| Platform | Architecture | Artifact Name |
|----------|--------------|---------------|
| Linux | x86_64 (glibc) | `paperscope-x86_64-unknown-linux-gnu.tar.xz` |
| Linux | x86_64 (musl) | `paperscope-x86_64-unknown-linux-musl.tar.xz` |
| Linux | aarch64 | `paperscope-aarch64-unknown-linux-gnu.tar.xz` |
| macOS | x86_64 | `paperscope-x86_64-apple-darwin.tar.xz` |
| macOS | aarch64 | `paperscope-aarch64-apple-darwin.tar.xz` |
| Windows | x86_64 | `paperscope-x86_64-pc-windows-msvc.zip` |

## Checksums

All artifacts include SHA256 checksums in `sha256.sum`.

```bash
# Verify
sha256sum -c sha256.sum
```

## Signing

### macOS (Notarization)
```bash
# Requires Apple Developer ID
codesign --timestamp --options runtime --sign "Developer ID Application: Name (TEAM_ID)" paper
xcrun notarytool submit paper --apple-id $APPLE_ID --password $APPLE_PASSWORD --team-id $TEAM_ID --wait
xcrun stapler staple paper
```

### Windows (Authenticode)
```bash
# Requires code signing certificate
signtool sign /tr http://timestamp.digicert.com /td sha256 /fd sha256 /f cert.pfx /p PASSWORD paper.exe
```

### Linux (GPG)
```bash
gpg --detach-sign --armor paper.tar.xz
```

## Dependencies

### Runtime
- `libsqlite3` (system library)
- `libc` (system library)

### Build
- Rust 1.88+
- `cargo-dist`
- Platform-specific toolchains (installed by CI)

## Troubleshooting

### macOS: "damaged app" error
```bash
xattr -d com.apple.quarantine paper
```

### Linux: "libsqlite3 not found"
```bash
sudo apt-get install libsqlite3-0  # Debian/Ubuntu
sudo dnf install libsqlite3        # Fedora/RHEL
```

### Windows: "VCRUNTIME140.dll not found"
Install Visual C++ Redistributable from Microsoft.

## Security

- All releases are signed and checksummed
- SBOM (CycloneDX + SPDX) provided for each release
- SLSA Level 3 provenance available via `cargo dist manifest`
- Reproducible builds: `cargo dist build --artifacts=local --output-format=json` produces identical binaries