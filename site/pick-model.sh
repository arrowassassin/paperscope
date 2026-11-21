#!/bin/sh
# Interactive model picker for Paperscope.

set -eu

if ! command -v paper >/dev/null 2>&1; then
  cat >&2 <<'EOF'
Error: the paper command is not on PATH.

Install Paperscope first:
  curl --proto '=https' --tlsv1.2 -LsSf https://kintsugi.tools/install.sh | sh
  cargo install paperscope

A Homebrew formula is also attached to the latest GitHub release:
  https://github.com/arrowassassin/paperscope/releases/latest

If Paperscope is already installed, add its binary directory to PATH and run
this picker again.
EOF
  exit 1
fi

# Support both a directly-run script and curl ... | sh. In the latter case,
# stdin contains the script, so read answers from the controlling terminal.
if [ -t 0 ]; then
  exec 3<&0
elif [ -t 1 ] && [ -r /dev/tty ]; then
  exec 3</dev/tty
else
  exec 3<&0
fi

read_reply() {
  printf '%s' "$1"
  if ! IFS= read -r REPLY <&3; then
    printf '\nError: input ended before the model selection was complete.\n' >&2
    exit 1
  fi
}

cat <<'EOF'
Paperscope model picker
-----------------------
Models are downloaded once and then run locally. Searching never downloads a
model automatically.

Language needs:
  1) English only   bge-small-en-v1.5       134 MB
  2) Multilingual   multilingual-e5-small   487 MB (100 languages)
EOF

while :; do
  read_reply 'Choose an embedding model [1/2, default: 1]: '
  case "$REPLY" in
    ''|1)
      embedding_model=bge-small-en-v1.5
      embedding_size='134 MB'
      break
      ;;
    2)
      embedding_model=multilingual-e5-small
      embedding_size='487 MB'
      break
      ;;
    *)
      printf 'Please enter 1 or 2.\n'
      ;;
  esac
done

while :; do
  read_reply 'Also install the optional bge-reranker-base reranker (1.13 GB)? [y/N] '
  case "$REPLY" in
    ''|n|N|no|NO|No)
      install_reranker=0
      break
      ;;
    y|Y|yes|YES|Yes)
      install_reranker=1
      break
      ;;
    *)
      printf 'Please enter y or n.\n'
      ;;
  esac
done

printf '\nSelected: %s (%s)\n' "$embedding_model" "$embedding_size"
printf 'Running: paper models install %s\n' "$embedding_model"
paper models install "$embedding_model"

if [ "$install_reranker" -eq 1 ]; then
  printf '\nRunning: paper models install bge-reranker-base\n'
  paper models install bge-reranker-base
fi

cat <<'EOF'

Model installation complete.
Run "paper models list" to see installed models.
EOF
