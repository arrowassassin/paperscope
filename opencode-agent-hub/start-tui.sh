#!/usr/bin/env bash
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Start server in background if not already running
if ! lsof -i :3888 >/dev/null 2>&1; then
  echo "🚀 Starting Hub Server on port 3888 in background..."
  node "$DIR/dashboard/server.mjs" >/dev/null 2>&1 &
  sleep 1
fi

# Launch TUI Dashboard
node "$DIR/dashboard/tui.mjs"
