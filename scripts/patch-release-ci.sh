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
          cargo cyclonedx --format json --override-filename sbom
          find . -maxdepth 2 -name 'sbom.json' -exec mv -f {} target/sbom.json \;
      - name: Generate SPDX SBOM
        run: |
          cargo sbom --output-format spdx_json_2_3 > target/sbom.spdx.json
      - name: Attach the SBOMs to the release
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          TAG: ${{ needs.plan.outputs.tag }}
        run: |
          gh release upload "$TAG" target/sbom.json target/sbom.spdx.json --clobber

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
      # The musl target isn't installed by default on GitHub runners; add it
      # plus the musl system headers so static musl builds can link.
      - name: Install musl target and system headers
        if: ${{ matrix.targets && contains(matrix.targets, 'x86_64-unknown-linux-musl') }}
        run: |
          rustup target add x86_64-unknown-linux-musl
          sudo apt-get update -qq
          sudo apt-get install -y -qq musl-tools musl-dev
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

# 5. Ensure consistent artifact action versions: cargo-dist generates the plan
# job with upload-artifact@v7 while build jobs use download-artifact@v8, which
# causes digest-mismatch on artifacts-plan-dist-manifest. Normalize both to v8.
if "upload-artifact@v7" in text:
    text = text.replace("upload-artifact@v7", "upload-artifact@v8")
    changed = True

if not changed:
    print(f"{path} is already patched")
    raise SystemExit(0)

with open(path, "w") as handle:
    handle.write(text)
print(f"patched {path}")
PY
