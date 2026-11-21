#!/usr/bin/env bash
# Re-apply the local customisations to the release workflow that `dist generate`
# owns.
#
# `dist generate --mode=ci` rewrites .github/workflows/release.yml from
# scratch, so anything added by hand is lost. The patch adds two things:
#
# 1. The block of signing secrets. dist reads them from the environment, and
#    GitHub Actions secrets are not environment variables until they are mapped.
#    They are only read when present, so a release without signing material
#    still builds (unsigned) instead of failing.
# 2. An SBOM job that attaches CycloneDX and SPDX SBOMs to the release.
#
# Usage: scripts/patch-release-ci.sh
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
workflow="$root/.github/workflows/release.yml"

python3 - "$workflow" <<'PY'
import sys

path = sys.argv[1]
with open(path) as handle:
    text = handle.read()

marker = "# patched-by: scripts/patch-release-ci.sh"
if marker in text:
    print(f"{path} is already patched")
    raise SystemExit(0)

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

lines = text.splitlines(keepends=True)

# 1. signing environment, right after the workflow name
for index, line in enumerate(lines):
    if line.startswith("name: Release"):
        lines.insert(index + 1, signing)
        break
else:
    raise SystemExit("could not find the `name: Release` line")

# 2. the SBOM job, before the final `announce` job
for index, line in enumerate(lines):
    if line.startswith("  announce:"):
        lines.insert(index, sbom_job)
        break
else:
    raise SystemExit("could not find the `announce:` job")

with open(path, "w") as handle:
    handle.write("".join(lines))
print(f"patched {path}")
PY
