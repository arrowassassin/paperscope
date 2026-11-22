#!/bin/sh
# Install the repository git hooks (currently: .githooks/pre-commit).
#
# Usage: ./scripts/install-hooks.sh
# Undo:  git config --unset core.hooksPath

set -eu

ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"

if [ ! -f .githooks/pre-commit ]; then
    echo "install-hooks: .githooks/pre-commit is missing" >&2
    exit 1
fi

chmod +x .githooks/pre-commit
git config core.hooksPath .githooks

echo "install-hooks: enabled core.hooksPath=.githooks"
echo "install-hooks: pre-commit runs cargo fmt + clippy on Rust changes"
echo "install-hooks: bypass one commit with 'git commit --no-verify'"
