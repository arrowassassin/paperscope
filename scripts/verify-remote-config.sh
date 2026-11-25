#!/usr/bin/env bash
# Usage: verify-remote-config.sh [pubkey-hex...]
# Verifies config/remote-config.toml.sig against the given 64-hex keys, or against
# config/keys/*.pub when none are given. Exit 0 = valid (or no keys configured yet), 1 = failure.
set -uo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
f=$root/config/remote-config.toml
keys=("$@")
if [ ${#keys[@]} -eq 0 ]; then
  for p in "$root"/config/keys/*.pub; do
    k=$(tr -d ' \r\n' < "$p"); [ -n "$k" ] && keys+=("$k")
  done
fi
if [ ${#keys[@]} -eq 0 ]; then
  echo "notice: no public keys configured yet; skipping signature check"; exit 0
fi
[ -f "$f.sig" ] || { echo "FAIL: $f.sig is missing" >&2; exit 1; }
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
base64 -d < "$f.sig" > "$tmp/sig" 2>/dev/null && [ "$(wc -c < "$tmp/sig")" -eq 64 ] \
  || { echo "FAIL: .sig is not base64 of a 64-byte signature" >&2; exit 1; }
for k in "${keys[@]}"; do
  [[ $k =~ ^[0-9a-f]{64}$ ]] || { echo "FAIL: bad public key '$k' (want 64 lowercase hex)" >&2; exit 1; }
  # Fixed Ed25519 SPKI header + raw key.
  printf "$(echo "302a300506032b6570032100$k" | sed 's/../\\x&/g')" > "$tmp/pub.der"
  if openssl pkeyutl -verify -rawin -pubin -keyform DER -inkey "$tmp/pub.der" -in "$f" -sigfile "$tmp/sig" >/dev/null 2>&1; then
    echo "OK: signature valid for key ${k:0:16}..."; exit 0
  fi
done
echo "FAIL: signature does not verify against any of ${#keys[@]} key(s)" >&2; exit 1
