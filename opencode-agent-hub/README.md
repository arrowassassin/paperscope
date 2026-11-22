# ⚡ OpenCode Go — Multi-Agent Native Side Panel Hub

An integrated multi-agent architecture and auto-model routing system **embedded directly into the OpenCode Desktop App side panel** and the **OpenCode TUI sidebar**. No separate web server or detached window required.

---

## 🎯 The OpenCode Go 7-Agent Roster

| Agent Role | Subagent Tag | Best OpenCode Go Model | Specialty |
| :--- | :--- | :--- | :--- |
| **Lead Orchestrator** | `build` (Primary) | `opencode-go/glm-5.3` | High-level decomposition & Sisyphus loop |
| **Principal Architect** | `@architect` | `opencode-go/glm-5.3` | System architecture, specs & file breakdown |
| **Lead Coder** | `@coder` | `opencode-go/deepseek-v4-pro` | Heavy multi-file code implementation |
| **Fast Fixer** | `@fast-fixer` | `opencode-go/deepseek-v4.1-flash` | Unit tests, linting & fast patches (<100ms) |
| **Systems Debugger** | `@debugger` | `opencode-go/qwen3.8-max` | Runtime crashes, diagnostics & stack traces |
| **Codebase Scout** | `@scout` | `opencode-go/qwen3.8-flash` | Read-only symbol search, ripgrep & AST-grep |
| **Security Auditor** | `@reviewer` | `opencode-go/kimi-k3.1-pro` | Git diff audit, security vulnerabilities |
| **Technical Scribe** | `@docgen` | `opencode-go/kimi-k3` | Markdown documentation & changelogs |

---

## 🖥️ How It Works in OpenCode

### 1. In OpenCode Desktop App (Side Panel)
* Open the **OpenCode Desktop** application.
* Press **`Cmd + B`** (macOS) or **`Ctrl + B`** (or click the Sidebar toggle icon in the header).
* The **"OpenCode Go Agents & Routing"** tab renders directly inside your right-side drawer, showing the active agents, their assigned models, and real-time routing status.

### 2. In OpenCode Terminal (TUI Sidebar)
* Launch OpenCode in your terminal: `opencode`
* The right sidebar is enabled by default (width: 46 columns).
* Press **`Ctrl + B`** at any time to toggle the sidebar view.
* Displays your live subagent routing table, current model allocations, and context sizes.

### 3. In-Session Quick Command
* Type `/hub` in any active OpenCode session to print an instant status table of all agents and model allocations.

---

## 🚀 One-Step Setup

```bash
cd opencode-agent-hub
./install.sh
```

Ensure your API key is set:
```bash
export OPENCODE_API_KEY="your-opencode-go-api-key"
```
