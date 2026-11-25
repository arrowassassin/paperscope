#!/usr/bin/env bash
# Usage: sign-remote-config.sh <private.pem>
# Signs config/remote-config.toml: base64 (one line) of the raw Ed25519 signature.
set -euo pipefail
[ $# -eq 1 ] && [ -f "$1" ] || { echo "usage: $0 <private.pem>" >&2; exit 2; }
root=$(cd "$(dirname "$0")/.." && pwd)
f=$root/config/remote-config.toml
openssl pkeyutl -sign -rawin -inkey "$1" -in "$f" | base64 -w0 > "$f.sig"
echo "wrote $f.sig"
"$root/scripts/verify-remote-config.sh" "$(openssl pkey -in "$1" -pubout -outform DER | tail -c 32 | od -An -tx1 -v | tr -d ' \n')"
