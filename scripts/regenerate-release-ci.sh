#!/usr/bin/env bash
# Regenerate the release workflow from dist-workspace.toml and re-apply our
# local customisations.
#
# `dist generate --mode=ci` refuses to write while `allow-dirty = ["ci"]` is
# set, so the script lifts that setting for the duration of the generation and
# puts it back afterwards.
#
# Usage: scripts/regenerate-release-ci.sh
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
config="$root/dist-workspace.toml"
backup="$(mktemp)"
cp "$config" "$backup"

restore() {
  cp "$backup" "$config"
  rm -f "$backup"
}
trap restore EXIT

python3 - "$config" <<'PY'
import sys

path = sys.argv[1]
with open(path) as handle:
    text = handle.read()

lines = []
for line in text.splitlines(keepends=True):
    if line.strip().startswith("allow-dirty"):
        continue
    lines.append(line)

with open(path, "w") as handle:
    handle.write("".join(lines))
PY

(cd "$root" && dist generate --mode=ci)
bash "$root/scripts/patch-release-ci.sh"
echo "regenerated $root/.github/workflows/release.yml"
