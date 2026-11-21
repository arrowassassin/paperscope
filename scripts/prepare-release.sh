#!/usr/bin/env bash
# Fetch, verify, and stage the model assets expected by dist-workspace.toml.
# This script is safe to run repeatedly; verified downloads are reused.
#
# Usage: scripts/prepare-release.sh [tag]
# The optional tag is used only in the printed commands (default: v0.1.0).
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

if (( $# > 1 )); then
  echo "usage: scripts/prepare-release.sh [tag]" >&2
  exit 2
fi

tag="${1:-v0.1.0}"
bash "$root/scripts/fetch-models.sh"

# Assert that the exact nine paths in dist-workspace.toml now exist and that
# every release asset has a unique basename (GitHub Releases cannot disambiguate
# three assets all named model.safetensors/config.json/tokenizer.json).
python3 - "$root" <<'PY'
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
model_ids = (
    "bge-small-en-v1.5",
    "multilingual-e5-small",
    "bge-reranker-base",
)
file_names = ("model.safetensors", "config.json", "tokenizer.json")
assets = [
    root / "models" / "release" / f"{model_id}-{file_name}"
    for model_id in model_ids
    for file_name in file_names
]
missing = [str(path) for path in assets if not path.is_file()]
if missing:
    print("missing staged release assets:\n  " + "\n  ".join(missing), file=sys.stderr)
    raise SystemExit(1)
if len({path.name for path in assets}) != len(assets):
    print("staged release asset names are not unique", file=sys.stderr)
    raise SystemExit(1)
print(f"release staging verified: {len(assets)} unique model assets")
PY

cat <<EOF

Model release assets are ready. No model files are included in platform archives.

Inspect the complete release plan:
  dist plan --tag="$tag"

Build the host platform archive plus global installers and model assets locally:
  dist build --tag="$tag" --artifacts=host

CI performs the equivalent local/global builds across its target matrix, then
uploads every artifact to GitHub Releases. To start that release flow:
  git tag "$tag"
  git push origin "$tag"

After CI completes, verify the nine model assets at:
  https://github.com/arrowassassin/paperscope/releases/tag/$tag
EOF
