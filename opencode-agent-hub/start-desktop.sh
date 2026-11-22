#!/usr/bin/env bash
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Start server in background if not already running
if ! lsof -i :3888 >/dev/null 2>&1; then
  echo "🚀 Starting Hub Server on port 3888..."
  node "$DIR/dashboard/server.mjs" &
  sleep 1
fi

echo "🌐 Opening Desktop Dashboard in app window..."
if command -v open >/dev/null 2>&1; then
  # On macOS, try to open in Chrome App Mode if available, or default browser
  if [ -d "/Applications/Google Chrome.app" ]; then
    open -na "Google Chrome" --args --app="http://127.0.0.1:3888"
  else
    open "http://127.0.0.1:3888"
  fi
fi
