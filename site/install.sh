#!/bin/sh
# Paperscope installer helper for macOS and Linux.
# Usage: sh install.sh [--yes]

set -eu

INSTALLER_URL="https://github.com/arrowassassin/paperscope/releases/latest/download/paper-installer.sh"
assume_yes=0

usage() {
  cat <<'EOF'
Usage: sh install.sh [--yes]

Downloads and runs the official Paperscope cargo-dist installer for macOS or
Linux. The detected platform and installer URL are shown before anything runs.

Options:
  -y, --yes   Run without a confirmation prompt (for automation).
  -h, --help  Show this help.
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    -y|--yes)
      assume_yes=1
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      printf 'Unknown option: %s\n\n' "$1" >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

if ! command -v curl >/dev/null 2>&1; then
  printf 'Error: curl is required. Install curl or use: cargo install paperscope\n' >&2
  exit 1
fi

system=$(uname -s)
machine=$(uname -m)

case "$system" in
  Darwin)
    platform="macOS"
    ;;
  Linux)
    platform="Linux"
    ;;
  *)
    printf 'Error: this shell installer supports macOS and Linux; detected %s.\n' "$system" >&2
    printf 'See https://github.com/arrowassassin/paperscope#install for other options.\n' >&2
    exit 1
    ;;
esac

cat <<EOF
Paperscope installer
--------------------
Detected platform: $platform ($machine)
Installer source: $INSTALLER_URL

After confirmation, this helper downloads the official cargo-dist installer
from GitHub Releases and pipes it to sh. The installer selects the matching
pre-built paper binary and installs it on this machine.

Command that will run:
  curl --proto '=https' --tlsv1.2 -LsSf "$INSTALLER_URL" | sh

Alternatives:
  cargo install paperscope
  Homebrew formula: https://github.com/arrowassassin/paperscope/releases/latest

EOF

if [ "$assume_yes" -eq 0 ]; then
  printf 'Download and run this installer? [y/N] '

  # With curl ... | sh, stdin is the script itself. If stdout is a terminal,
  # read the confirmation from the controlling terminal instead. Otherwise,
  # reading stdin also supports redirected answers and non-interactive tests.
  if [ ! -t 0 ] && [ -t 1 ] && [ -r /dev/tty ]; then
    if ! IFS= read -r answer </dev/tty; then
      answer=""
    fi
  else
    if ! IFS= read -r answer; then
      answer=""
    fi
  fi

  case "$answer" in
    y|Y|yes|YES|Yes)
      ;;
    *)
      printf '\nInstallation cancelled; nothing was downloaded or executed.\n'
      exit 0
      ;;
  esac
fi

printf '\nRunning the official Paperscope installer...\n'
curl --proto '=https' --tlsv1.2 -LsSf "$INSTALLER_URL" | sh
