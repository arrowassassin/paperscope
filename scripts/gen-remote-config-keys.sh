#!/usr/bin/env bash
# Usage: gen-remote-config-keys.sh <outdir>
# Writes <outdir>/{primary,backup}.pem (private, keep offline, never commit) and
# config/keys/{primary,backup}.pub (raw 32-byte Ed25519 key as 64 lowercase hex).
set -euo pipefail
[ $# -eq 1 ] || { echo "usage: $0 <outdir>" >&2; exit 2; }
out=$1
root=$(cd "$(dirname "$0")/.." && pwd)
mkdir -p "$out" "$root/config/keys"
for n in primary backup; do
  [ ! -e "$out/$n.pem" ] || { echo "refusing to overwrite $out/$n.pem" >&2; exit 1; }
done
for n in primary backup; do
  (umask 077; openssl genpkey -algorithm ed25519 -out "$out/$n.pem")
  # The last 32 bytes of the SPKI DER are the raw public key.
  openssl pkey -in "$out/$n.pem" -pubout -outform DER | tail -c 32 | od -An -tx1 -v | tr -d ' \n' > "$root/config/keys/$n.pub"
  echo >> "$root/config/keys/$n.pub"
  echo "$n: $(cat "$root/config/keys/$n.pub")"
done
echo "Keep $out/*.pem safe (backup offline); commit only config/keys/*.pub."
