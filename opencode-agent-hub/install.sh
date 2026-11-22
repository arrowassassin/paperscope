#!/usr/bin/env bash
set -e

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "=========================================================="
echo "⚡ Integrating Multi-Agent Dashboard into OpenCode"
echo "   (Embedded directly inside Desktop Side Panel & TUI Sidebar)"
echo "=========================================================="

# 1. Check Node
if ! command -v node >/dev/null 2>&1; then
  echo "❌ Node.js is required but not found. Please install Node.js (v18+) first."
  exit 1
fi
echo "✔ Node.js detected: $(node -v)"

# 2. Check OpenCode CLI / Desktop
if ! command -v opencode >/dev/null 2>&1; then
  echo "ℹ OpenCode CLI not found in PATH."
  echo "  Installing opencode-ai globally via npm..."
  npm install -g opencode-ai || true
else
  echo "✔ OpenCode detected: $(which opencode)"
fi

# 3. Create config & plugin directories
echo "ℹ Deploying OpenCode configuration and side-panel integration..."
mkdir -p "$HOME/.config/opencode/agents" "$HOME/.config/opencode/plugin" "$HOME/.omo/agent" 2>/dev/null || true

# Copy configs
cp "$DIR/configs/opencode.json" "$HOME/.config/opencode/opencode.json" 2>/dev/null || true
cp "$DIR/configs/opencode.json" "$DIR/../opencode.json" 2>/dev/null || true
cp "$DIR/configs/tui.json" "$HOME/.config/opencode/tui.json" 2>/dev/null || true
cp "$DIR/configs/omo/models.json" "$HOME/.omo/agent/models.json" 2>/dev/null || true
cp "$DIR/configs/omo/config.json" "$HOME/.omo/config.json" 2>/dev/null || true

# Copy plugins to OpenCode plugin directory
cp "$DIR/configs/plugins/auto-router.ts" "$HOME/.config/opencode/plugin/auto-router.ts" 2>/dev/null || true
cp "$DIR/configs/plugins/hub-sidebar.ts" "$HOME/.config/opencode/plugin/hub-sidebar.ts" 2>/dev/null || true

# Install community sidebar & pruning plugins
echo "ℹ Installing OpenCode sidebar plugins (@idrisgit/opencode-subagent-model-selector, @tarquinen/opencode-dcp)..."
npm install -g @idrisgit/opencode-subagent-model-selector @tarquinen/opencode-dcp 2>/dev/null || true

echo ""
echo "=========================================================="
echo "🎉 Native Side-Panel Dashboard Integrated!"
echo "=========================================================="
echo "How it works inside your OpenCode Desktop App & TUI:"
echo ""
echo "  1. Desktop App Side Panel:"
echo "     - Launch your OpenCode Desktop app."
echo "     - Press Cmd+B (or click the Sidebar icon on the top right)."
echo "     - The 'OpenCode Go Agents & Routing' panel is pinned inside the side panel."
echo ""
echo "  2. Terminal TUI Sidebar:"
echo "     - Run 'opencode' in your terminal."
echo "     - Press Ctrl+B to toggle the right-hand sidebar."
echo "     - Displays the live active models, subagent routes, and status directly in the TUI."
echo ""
echo "  3. In-Chat Status Command:"
echo "     - Type '/hub' in any OpenCode session for an instant summary table."
echo "=========================================================="
