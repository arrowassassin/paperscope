#!/usr/bin/env bash
# Run every release gate locally, in the same order CI does.
#
# Usage: scripts/verify-release.sh [--quick]
#   --quick   skip the MSRV (1.94) test run and cargo audit network fetch
#
# Requires: rustup toolchains 1.94.0 (MSRV) and 1.100.0 (CI stable),
# cargo-deny, cargo-audit. actionlint is used when installed.
set -euo pipefail

quick=0
if [[ "${1:-}" == "--quick" ]]; then
  quick=1
fi

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

step() {
  printf '\n\033[1;34m==> %s\033[0m\n' "$*"
}

step "rustfmt"
cargo +1.100.0 fmt --all --check

step "clippy: all features"
cargo +1.100.0 clippy --all-targets --features icu_segmenter,lindera -- -D warnings

step "clippy: default features"
cargo +1.100.0 clippy --all-targets -- -D warnings

step "clippy: no default features"
cargo +1.100.0 clippy --no-default-features --all-targets -- -D warnings

step "test: stable 1.100, all language features"
cargo +1.100.0 test --features icu_segmenter,lindera

step "test: stable 1.100, default features"
cargo +1.100.0 test

if [[ "$quick" -eq 0 ]]; then
  step "test: MSRV 1.94"
  cargo +1.94.0 test
fi

step "cargo deny (licenses, bans, sources)"
cargo deny check licenses bans sources

if [[ "$quick" -eq 0 ]]; then
  step "cargo audit"
  cargo audit
fi

step "cargo publish --dry-run"
cargo +1.100.0 publish --dry-run

if command -v actionlint >/dev/null 2>&1; then
  step "actionlint"
  actionlint .github/workflows/*.yml
else
  step "workflow YAML parse (actionlint not installed)"
  python3 - <<'PY'
import glob
import yaml
for path in sorted(glob.glob(".github/workflows/*.yml")):
    with open(path) as handle:
        yaml.safe_load(handle)
    print(f"ok: {path}")
PY
fi

step "shell script syntax"
for script in scripts/*.sh site/*.sh; do
  [[ -e "$script" ]] || continue
  bash -n "$script"
  echo "ok: $script"
done

printf '\n\033[1;32mAll release gates passed.\033[0m\n'
