import http from "node:http";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const PORT = process.env.PORT ? parseInt(process.env.PORT, 10) : 3888;

// Initial state representing OpenCode Go Multi-Agent Roster
let state = {
  provider: "OpenCode Go (Hosted Gateway)",
  orchestrator: {
    name: "Sisyphus / Lead Orchestrator",
    model: "opencode-go/glm-5.3",
    status: "idle",
  },
  agents: [
    {
      id: "scout",
      name: "Scout / Librarian",
      model: "opencode-go/qwen3.8-flash",
      role: "Repo Recon & Symbol Search",
      status: "idle",
      tools: ["grep", "find", "ast-grep", "read"],
      quotaUsed: 14,
      contextLimit: "64k",
      latency: "120ms"
    },
    {
      id: "architect",
      name: "Architect / Prometheus",
      model: "opencode-go/glm-5.3",
      role: "System Design & Decomposition",
      status: "idle",
      tools: ["plan", "spec"],
      quotaUsed: 38,
      contextLimit: "128k",
      latency: "340ms"
    },
    {
      id: "coder",
      name: "Lead Coder / Hephaestus",
      model: "opencode-go/deepseek-v4-pro",
      role: "Core Implementation & Refactor",
      status: "idle",
      tools: ["edit", "write", "bash"],
      quotaUsed: 52,
      contextLimit: "128k",
      latency: "280ms"
    },
    {
      id: "fast-fixer",
      name: "Fast Fixer",
      model: "opencode-go/deepseek-v4.1-flash",
      role: "Boilerplate, Unit Tests & Lint",
      status: "idle",
      tools: ["edit", "write", "test"],
      quotaUsed: 19,
      contextLimit: "64k",
      latency: "95ms"
    },
    {
      id: "debugger",
      name: "Systems Debugger",
      model: "opencode-go/qwen3.8-max",
      role: "Diagnostics, RCA & Stack Traces",
      status: "idle",
      tools: ["bash", "edit", "diagnose"],
      quotaUsed: 44,
      contextLimit: "128k",
      latency: "310ms"
    },
    {
      id: "reviewer",
      name: "Auditor / Argus",
      model: "opencode-go/kimi-k3.1-pro",
      role: "Security & Whole-Repo Diff Audit",
      status: "idle",
      tools: ["diff", "lint", "security"],
      quotaUsed: 27,
      contextLimit: "200k+",
      latency: "420ms"
    },
    {
      id: "docgen",
      name: "Technical Scribe",
      model: "opencode-go/kimi-k3",
      role: "Docs, READMEs & Changelogs",
      status: "idle",
      tools: ["write", "doc"],
      quotaUsed: 11,
      contextLimit: "200k+",
      latency: "260ms"
    }
  ],
  quotas: {
    "GLM Family (Zhipu)": { used: 38, limit: 100, resetIn: "2h 15m" },
    "DeepSeek Family": { used: 52, limit: 100, resetIn: "1h 40m" },
    "Qwen Family (Alibaba)": { used: 44, limit: 100, resetIn: "3h 05m" },
    "Kimi Family (Moonshot)": { used: 27, limit: 100, resetIn: "4h 20m" }
  },
  logs: [
    {
      timestamp: new Date().toLocaleTimeString(),
      type: "SYSTEM",
      message: "OpenCode Go Multi-Agent Hub initialized with 7 specialized routes."
    }
  ]
};

// Connected SSE clients
const sseClients = new Set();

function broadcastEvent(data) {
  const payload = `data: ${JSON.stringify(data)}\n\n`;
  for (const client of sseClients) {
    client.write(payload);
  }
}

function classifyTask(prompt) {
  const p = prompt.toLowerCase();
  if (p.includes("review") || p.includes("audit") || p.includes("security") || p.includes("diff")) {
    return {
      agentId: "reviewer",
      confidence: 0.98,
      reason: "Detected audit/review/security intent -> routed to large-context auditor (Kimi K3.1 Pro)"
    };
  }
  if (p.includes("debug") || p.includes("fix bug") || p.includes("fail") || p.includes("error") || p.includes("exception") || p.includes("crash")) {
    return {
      agentId: "debugger",
      confidence: 0.96,
      reason: "Detected diagnostic/debugging intent -> routed to tool-use reasoning model (Qwen 3.8 Max)"
    };
  }
  if (p.includes("test") || p.includes("lint") || p.includes("format") || p.includes("quick edit")) {
    return {
      agentId: "fast-fixer",
      confidence: 0.94,
      reason: "Detected testing/linting/minor task -> routed to ultra-low latency model (DeepSeek V4.1 Flash)"
    };
  }
  if (p.includes("where") || p.includes("find") || p.includes("locate") || p.includes("search") || p.includes("symbols")) {
    return {
      agentId: "scout",
      confidence: 0.95,
      reason: "Detected codebase exploration intent -> routed to fast recon model (Qwen 3.8 Flash)"
    };
  }
  if (p.includes("plan") || p.includes("architect") || p.includes("schema") || p.includes("design") || p.includes("break down")) {
    return {
      agentId: "architect",
      confidence: 0.97,
      reason: "Detected structural planning intent -> routed to deep logic planner (GLM 5.3)"
    };
  }
  if (p.includes("doc") || p.includes("readme") || p.includes("changelog") || p.includes("explain")) {
    return {
      agentId: "docgen",
      confidence: 0.92,
      reason: "Detected documentation/writing intent -> routed to long-context synthesizer (Kimi K3)"
    };
  }
  return {
    agentId: "coder",
    confidence: 0.91,
    reason: "Detected feature implementation/refactor -> routed to lead coding engine (DeepSeek V4 Pro)"
  };
}

const server = http.createServer((req, res) => {
  const parsedUrl = new URL(req.url, `http://${req.headers.host}`);

  // CORS headers
  res.setHeader("Access-Control-Allow-Origin", "*");
  res.setHeader("Access-Control-Allow-Methods", "GET, POST, OPTIONS");
  res.setHeader("Access-Control-Allow-Headers", "Content-Type");

  if (req.method === "OPTIONS") {
    res.writeHead(204);
    res.end();
    return;
  }

  // SSE Stream
  if (parsedUrl.pathname === "/api/events") {
    res.writeHead(200, {
      "Content-Type": "text/event-stream",
      "Cache-Control": "no-cache",
      "Connection": "keep-alive"
    });
    res.write(`data: ${JSON.stringify({ type: "SYNC", state })}\n\n`);
    sseClients.add(res);

    req.on("close", () => {
      sseClients.delete(res);
    });
    return;
  }

  // Get current status
  if (parsedUrl.pathname === "/api/status" && req.method === "GET") {
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(JSON.stringify(state));
    return;
  }

  // Test Route Dispatcher
  if (parsedUrl.pathname === "/api/route" && req.method === "POST") {
    let body = "";
    req.on("data", chunk => { body += chunk; });
    req.on("end", () => {
      try {
        const { prompt } = JSON.parse(body || "{}");
        if (!prompt) {
          res.writeHead(400, { "Content-Type": "application/json" });
          res.end(JSON.stringify({ error: "prompt is required" }));
          return;
        }

        const decision = classifyTask(prompt);
        const targetAgent = state.agents.find(a => a.id === decision.agentId);

        // Update agent status temporarily to 'working'
        targetAgent.status = "working";
        targetAgent.quotaUsed = Math.min(100, targetAgent.quotaUsed + 2);

        const logEntry = {
          timestamp: new Date().toLocaleTimeString(),
          type: "ROUTED",
          prompt,
          agent: targetAgent.name,
          model: targetAgent.model,
          reason: decision.reason,
          confidence: `${(decision.confidence * 100).toFixed(0)}%`
        };

        state.logs.unshift(logEntry);
        if (state.logs.length > 50) state.logs.pop();

        broadcastEvent({ type: "ROUTE_EVENT", decision, targetAgent, logEntry, state });

        // Reset agent to idle after brief animation window
        setTimeout(() => {
          targetAgent.status = "idle";
          broadcastEvent({ type: "AGENT_IDLE", agentId: targetAgent.id });
        }, 3000);

        res.writeHead(200, { "Content-Type": "application/json" });
        res.end(JSON.stringify({ decision, targetAgent }));
      } catch (e) {
        res.writeHead(500, { "Content-Type": "application/json" });
        res.end(JSON.stringify({ error: e.message }));
      }
    });
    return;
  }

  // Override model for an agent
  if (parsedUrl.pathname === "/api/override" && req.method === "POST") {
    let body = "";
    req.on("data", chunk => { body += chunk; });
    req.on("end", () => {
      try {
        const { agentId, newModel } = JSON.parse(body || "{}");
        const agent = state.agents.find(a => a.id === agentId);
        if (agent) {
          const oldModel = agent.model;
          agent.model = newModel;
          const logEntry = {
            timestamp: new Date().toLocaleTimeString(),
            type: "OVERRIDE",
            message: `Manually changed ${agent.name} model from ${oldModel} to ${newModel}`
          };
          state.logs.unshift(logEntry);
          broadcastEvent({ type: "OVERRIDE", agentId, newModel, state });
          res.writeHead(200, { "Content-Type": "application/json" });
          res.end(JSON.stringify({ success: true, agent }));
        } else {
          res.writeHead(404, { "Content-Type": "application/json" });
          res.end(JSON.stringify({ error: "Agent not found" }));
        }
      } catch (e) {
        res.writeHead(500, { "Content-Type": "application/json" });
        res.end(JSON.stringify({ error: e.message }));
      }
    });
    return;
  }

  // Serve static UI
  let filePath = path.join(__dirname, "public", parsedUrl.pathname === "/" ? "index.html" : parsedUrl.pathname);
  if (fs.existsSync(filePath) && fs.statSync(filePath).isFile()) {
    const ext = path.extname(filePath);
    const contentTypes = {
      ".html": "text/html",
      ".js": "text/javascript",
      ".css": "text/css",
      ".json": "application/json"
    };
    res.writeHead(200, { "Content-Type": contentTypes[ext] || "text/plain" });
    fs.createReadStream(filePath).pipe(res);
  } else {
    res.writeHead(404, { "Content-Type": "text/plain" });
    res.end("Not Found");
  }
});

server.listen(PORT, "127.0.0.1", () => {
  console.log(`[OpenCode Hub] Live Dashboard running at http://127.0.0.1:${PORT}`);
});
