#!/usr/bin/env bash
# Re-apply the local customisations to the release workflow that `dist generate`
# owns.
#
# `dist generate --mode=ci` rewrites .github/workflows/release.yml from
# scratch, so anything added by hand is lost. The patch adds three things:
#
# 1. The block of signing secrets. dist reads them from the environment, and
#    GitHub Actions secrets are not environment variables until they are mapped.
#    They are only read when present, so a release without signing material
#    still builds (unsigned) instead of failing.
# 2. An SBOM job that attaches CycloneDX and SPDX SBOMs to the release.
# 3. A model fetch/verify step before the global dist build. dist uploads
#    extra-artifacts automatically once they exist; its per-artifact build
#    commands then reuse the verified files instead of downloading them again.
#
# Each insertion has an independent idempotency check so this script can add a
# new customisation to an already-patched workflow without duplicating old ones.
#
# Usage: scripts/patch-release-ci.sh
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
workflow="$root/.github/workflows/release.yml"

python3 - "$workflow" <<'PY'
import re
import sys

path = sys.argv[1]
with open(path) as handle:
    text = handle.read()

marker = "# patched-by: scripts/patch-release-ci.sh"
model_marker = "# patched-by: scripts/patch-release-ci.sh (model assets)"
musl_marker = "# patched-by: scripts/patch-release-ci.sh (musl target)"
changed = False

signing = """
# patched-by: scripts/patch-release-ci.sh
# Signing secrets for dist. They are only read when present, so a release
# without signing material still builds (unsigned) instead of failing.
env:
  CODESIGN_IDENTITY: ${{ secrets.CODESIGN_IDENTITY }}
  CODESIGN_CERTIFICATE: ${{ secrets.CODESIGN_CERTIFICATE }}
  CODESIGN_CERTIFICATE_PASSWORD: ${{ secrets.CODESIGN_CERTIFICATE_PASSWORD }}
  CODESIGN_OPTIONS: ${{ secrets.CODESIGN_OPTIONS }}
"""

sbom_job = """
  # patched-by: scripts/patch-release-ci.sh
  # Attach a CycloneDX and an SPDX SBOM to the release, so every artifact can
  # be traced back to the exact dependency tree that produced it.
  sbom:
    name: SBOM
    needs: [plan, host]
    # The plan job creates the release and knows the tag. On a pull request
    # there is no tag, so the job is skipped instead of failing.
    if: needs.plan.outputs.tag != ''
    runs-on: ubuntu-latest
    timeout-minutes: 20
    permissions:
      contents: write
    steps:
      - uses: actions/checkout@v7
        with:
          persist-credentials: false
      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
      - name: Install cargo-cyclonedx
        uses: baptiste0928/cargo-install@v3
        with:
          crate: cargo-cyclonedx
          version: '0.5'
      - name: Install cargo-sbom
        uses: baptiste0928/cargo-install@v3
        with:
          crate: cargo-sbom
          version: '0.10'
      - name: Generate CycloneDX SBOM
        run: |
          mkdir -p target
          # cargo-cyclonedx names its output after the package and format
          # (<name>.cdx.json on 0.5), so locate whatever it wrote instead of
          # assuming a filename.
          cargo cyclonedx --format json --override-filename sbom
          found=$(find . -maxdepth 2 -name '*.json' ! -path './target/*' ! -path './.git/*' \\
            | grep -Ei 'bom|cdx|sbom' | head -n1)
          if [ -z "$found" ]; then
            echo "::error::cargo-cyclonedx produced no JSON SBOM"
            exit 1
          fi
          mv -f "$found" target/sbom.json
      - name: Generate SPDX SBOM
        run: |
          mkdir -p target
          cargo sbom --output-format spdx_json_2_3 > target/sbom.spdx.json
      - name: Attach the SBOMs to the release
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          TAG: ${{ needs.plan.outputs.tag }}
        run: |
          # Upload whichever SBOMs were produced; a missing format must not
          # fail an otherwise complete release.
          for f in target/sbom.json target/sbom.spdx.json; do
            if [ -f "$f" ]; then
              gh release upload "$TAG" "$f" --clobber
            else
              echo "::warning::$f was not generated"
            fi
          done

"""

model_step = """
      # patched-by: scripts/patch-release-ci.sh (model assets)
      # dist includes extra-artifacts in its global manifest and uploads them
      # with the other release assets. Fetch and verify their source files first;
      # the build commands in dist-workspace.toml are idempotent and reuse them.
      - name: Fetch and verify model release assets
        shell: bash
        run: scripts/fetch-models.sh
"""

# 1. Signing environment, right after the workflow name. Check for the actual
# setting rather than only the shared marker so workflows patched by older
# versions of this script remain compatible.
if "  CODESIGN_IDENTITY: ${{ secrets.CODESIGN_IDENTITY }}" not in text:
    lines = text.splitlines(keepends=True)
    for index, line in enumerate(lines):
        if line.startswith("name: Release"):
            lines.insert(index + 1, signing)
            break
    else:
        raise SystemExit("could not find the `name: Release` line")
    text = "".join(lines)
    changed = True

# 2. The SBOM job, before the final `announce` job.
if not re.search(r"(?m)^  sbom:\s*$", text):
    lines = text.splitlines(keepends=True)
    for index, line in enumerate(lines):
        if line.startswith("  announce:"):
            lines.insert(index, sbom_job)
            break
    else:
        raise SystemExit("could not find the `announce:` job")
    text = "".join(lines)
    changed = True

# 3. Model files are global (architecture-independent) extra-artifacts, so they
# belong in build-global-artifacts rather than in every platform build.
if model_marker not in text:
    lines = text.splitlines(keepends=True)
    for index, line in enumerate(lines):
        if "dist build " in line and '"--artifacts=global"' in line:
            insertion = index
            while insertion >= 0 and not lines[insertion].startswith("      - id: cargo-dist"):
                insertion -= 1
            if insertion < 0:
                raise SystemExit("could not find the global cargo-dist step")
            lines.insert(insertion, model_step)
            break
    else:
        raise SystemExit("could not find the global `dist build` command")
    text = "".join(lines)
    changed = True

# 4. The `x86_64-unknown-linux-musl` target needs the Rust target and musl
# system headers installed before building. Cargo-dist doesn't add the Rust
# target automatically, so we add a step before every platform build.
musl_step = """
      # patched-by: scripts/patch-release-ci.sh (musl target)
      # The musl target isn't installed by default on GitHub runners. Ubuntu's
      # musl-tools package provides the compiler as `musl-gcc`, but cc-rs looks
      # for the triple-prefixed name (`x86_64-linux-musl-gcc`), so point CC and
      # the linker at musl-gcc explicitly.
      - name: Install musl target and toolchain
        if: ${{ matrix.targets && contains(matrix.targets, 'x86_64-unknown-linux-musl') }}
        run: |
          rustup target add x86_64-unknown-linux-musl
          sudo apt-get update -qq
          sudo apt-get install -y -qq musl-tools musl-dev
          echo "CC_x86_64_unknown_linux_musl=musl-gcc" >> "$GITHUB_ENV"
          echo "AR_x86_64_unknown_linux_musl=ar" >> "$GITHUB_ENV"
          echo "CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=musl-gcc" >> "$GITHUB_ENV"
"""
if musl_marker not in text:
    lines = text.splitlines(keepends=True)
    for index, line in enumerate(lines):
        if "      - name: Build artifacts" in line:
            insertion = index
            while insertion >= 0 and not lines[insertion].startswith("      - name:"):
                insertion -= 1
            if insertion < 0:
                raise SystemExit("could not find the Build artifacts step")
            lines.insert(insertion, musl_step)
            break
    else:
        raise SystemExit("could not find the `Build artifacts` step")
    text = "".join(lines)
    changed = True

# 5. Ensure consistent artifact action versions. cargo-dist may emit a mix of
# @v7 and @v8; mixed versions cause a digest mismatch on
# artifacts-plan-dist-manifest, and upload-artifact@v8 does not resolve on
# GitHub. Normalize both actions to v7, the version the released workflow uses.
for action in ("upload-artifact", "download-artifact"):
    if f"{action}@v8" in text:
        text = text.replace(f"{action}@v8", f"{action}@v7")
        changed = True

# 6. Push the Homebrew formula with a deploy key on kintsugi-tools/homebrew-tap
# (write access to that one repository, tied to no person) instead of a token.
# actions/checkout switches to SSH when ssh-key is set, so `git push` uses it.
tap_token = "token: ${{ secrets.HOMEBREW_TAP_TOKEN }}"
if tap_token in text:
    text = text.replace(tap_token, "ssh-key: ${{ secrets.HOMEBREW_TAP_DEPLOY_KEY }}")
    changed = True

if not changed:
    print(f"{path} is already patched")
    raise SystemExit(0)

with open(path, "w") as handle:
    handle.write(text)
print(f"patched {path}")
PY
