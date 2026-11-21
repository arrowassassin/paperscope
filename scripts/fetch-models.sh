#!/usr/bin/env bash
# Download every pinned model file from its canonical Hugging Face revision,
# verify its size and SHA256, and stage uniquely-named files for cargo-dist.
#
# cargo-dist uses each artifact's basename as its GitHub Release asset name.
# The release staging names therefore include the model ID: three upstream
# files named "config.json" would otherwise overwrite one another.
#
# Usage: scripts/fetch-models.sh
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

models_dir="$root/models"
release_dir="$models_dir/release"
mkdir -p "$release_dir"

# Keep these entries in sync with src/core/models/specs.rs.
# Fields: model ID | Hugging Face repo | pinned commit | file | SHA256 | bytes
entries=(
  "bge-small-en-v1.5|BAAI/bge-small-en-v1.5|5c38ec7c405ec4b44b94cc5a9bb96e735b38267a|model.safetensors|3c9f31665447c8911517620762200d2245a2518d6e7208acc78cd9db317e21ad|133466304"
  "bge-small-en-v1.5|BAAI/bge-small-en-v1.5|5c38ec7c405ec4b44b94cc5a9bb96e735b38267a|config.json|094f8e891b932f2000c92cfc663bac4c62069f5d8af5b5278c4306aef3084750|743"
  "bge-small-en-v1.5|BAAI/bge-small-en-v1.5|5c38ec7c405ec4b44b94cc5a9bb96e735b38267a|tokenizer.json|d241a60d5e8f04cc1b2b3e9ef7a4921b27bf526d9f6050ab90f9267a1f9e5c66|711396"
  "multilingual-e5-small|intfloat/multilingual-e5-small|614241f622f53c4eeff9890bdc4f31cfecc418b3|model.safetensors|1a55775f53449dac10a2bcbc312469fac40b96d53198c407081a831f81c98477|470641600"
  "multilingual-e5-small|intfloat/multilingual-e5-small|614241f622f53c4eeff9890bdc4f31cfecc418b3|config.json|69137736cab8b8903a07fe8afaafdda25aac55415a12a55d1bffa9f581abf959|655"
  "multilingual-e5-small|intfloat/multilingual-e5-small|614241f622f53c4eeff9890bdc4f31cfecc418b3|tokenizer.json|0b44a9d7b51c3c62626640cda0e2c2f70fdacdc25bbbd68038369d14ebdf4c39|17082730"
  "bge-reranker-base|BAAI/bge-reranker-base|2cfc18c9415c912f9d8155881c133215df768a70|model.safetensors|ced967c45fd1902eb92716c9ceeca7c95a936770ea9db611f5a841b926e33fbd|1112206140"
  "bge-reranker-base|BAAI/bge-reranker-base|2cfc18c9415c912f9d8155881c133215df768a70|config.json|289adf7ada1eb6b4afa7589a48a032d45a076cf2e46dcdb3b4cabc33be14f708|799"
  "bge-reranker-base|BAAI/bge-reranker-base|2cfc18c9415c912f9d8155881c133215df768a70|tokenizer.json|9eb652ac4e40cc093272bbbe0f55d521cf67570060227109b5cdc20945a4489e|17098107"
)

verify() {
  python3 - "$1" "$2" "$3" <<'PY'
import hashlib
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
expected_sha256 = sys.argv[2].lower()
expected_size = int(sys.argv[3])

try:
    actual_size = path.stat().st_size
except OSError as error:
    print(f"cannot stat {path}: {error}", file=sys.stderr)
    raise SystemExit(1)

if actual_size != expected_size:
    print(
        f"size mismatch for {path}: expected {expected_size} bytes, "
        f"got {actual_size}",
        file=sys.stderr,
    )
    raise SystemExit(1)

sha256 = hashlib.sha256()
try:
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            sha256.update(chunk)
except OSError as error:
    print(f"cannot read {path}: {error}", file=sys.stderr)
    raise SystemExit(1)

actual_sha256 = sha256.hexdigest()
if actual_sha256 != expected_sha256:
    print(
        f"SHA256 mismatch for {path}:\n"
        f"  expected: {expected_sha256}\n"
        f"  actual:   {actual_sha256}",
        file=sys.stderr,
    )
    raise SystemExit(1)
PY
}

stage() {
  python3 - "$1" "$2" <<'PY'
import os
import pathlib
import shutil
import sys

source = pathlib.Path(sys.argv[1])
destination = pathlib.Path(sys.argv[2])
destination.parent.mkdir(parents=True, exist_ok=True)

if destination.exists():
    try:
        if os.path.samefile(source, destination):
            raise SystemExit(0)
    except OSError:
        pass
    destination.unlink()

try:
    # Hard links avoid a second 1.7 GB copy on the normal same-filesystem path.
    os.link(source, destination)
except OSError:
    # Some filesystems do not support hard links. Copy atomically instead.
    temporary = destination.with_name(destination.name + ".tmp")
    try:
        shutil.copyfile(source, temporary)
        os.replace(temporary, destination)
    finally:
        if temporary.exists():
            temporary.unlink()
PY
}

for entry in "${entries[@]}"; do
  IFS='|' read -r model_id repo revision file_name expected_sha256 expected_size <<< "$entry"
  model_dir="$models_dir/$model_id"
  destination="$model_dir/$file_name"
  partial="$destination.partial"
  staged="$release_dir/$model_id-$file_name"
  url="https://huggingface.co/$repo/resolve/$revision/$file_name"
  mkdir -p "$model_dir"

  if [[ -e "$destination" && ! -f "$destination" ]]; then
    echo "error: $destination exists but is not a regular file" >&2
    exit 1
  fi
  if [[ -e "$partial" && ! -f "$partial" ]]; then
    echo "error: $partial exists but is not a regular file" >&2
    exit 1
  fi

  if [[ -f "$destination" ]]; then
    if verify "$destination" "$expected_sha256" "$expected_size"; then
      echo "verified $model_id/$file_name"
      stage "$destination" "$staged"
      continue
    fi
    echo "removing invalid cached file $destination" >&2
    rm -f "$destination"
  fi

  if [[ -f "$partial" ]]; then
    partial_size="$(python3 -c 'import os, sys; print(os.path.getsize(sys.argv[1]))' "$partial")"
    if (( partial_size > expected_size )); then
      echo "discarding oversized partial download $partial" >&2
      rm -f "$partial"
    elif (( partial_size == expected_size )); then
      # Avoid a Range request past EOF when an earlier run finished downloading
      # but was interrupted just before verification/rename.
      if verify "$partial" "$expected_sha256" "$expected_size"; then
        mv -f "$partial" "$destination"
        echo "verified $model_id/$file_name"
        stage "$destination" "$staged"
        continue
      else
        rm -f "$partial"
        echo "error: deleted corrupt completed download $partial" >&2
        exit 1
      fi
    fi
  fi

  echo "fetching $model_id/$file_name"
  echo "  $url"
  if ! curl \
    --fail \
    --location \
    --proto '=https' \
    --proto-redir '=https' \
    --tlsv1.2 \
    --retry 3 \
    --retry-delay 2 \
    --retry-all-errors \
    --continue-at - \
    --output "$partial" \
    "$url"; then
    echo "error: download failed for $model_id/$file_name; partial data was retained" >&2
    exit 1
  fi

  if ! verify "$partial" "$expected_sha256" "$expected_size"; then
    rm -f "$partial"
    echo "error: deleted corrupt download $partial" >&2
    exit 1
  fi

  # The final file appears only after a complete, verified download.
  mv -f "$partial" "$destination"
  stage "$destination" "$staged"
done

echo "verified and staged ${#entries[@]} model files in $release_dir"
