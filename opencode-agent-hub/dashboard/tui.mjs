#!/usr/bin/env node
import http from "node:http";
import readline from "node:readline";

const SERVER_URL = process.env.HUB_SERVER || "http://127.0.0.1:3888";

// ANSI Styling
const c = {
  reset: "\x1b[0m",
  bold: "\x1b[1m",
  dim: "\x1b[2m",
  cyan: "\x1b[36m",
  green: "\x1b[32m",
  yellow: "\x1b[33m",
  blue: "\x1b[34m",
  magenta: "\x1b[35m",
  red: "\x1b[31m",
  white: "\x1b[37m",
  bgBlue: "\x1b[44m",
  bgCyan: "\x1b[46m",
  clear: "\x1b[2J\x1b[H"
};

// Server-supplied text must not carry terminal control sequences (C0/C1, DEL).
const CTRL = /[\x00-\x1f\x7f-\x9f]/g;
const strip = (s) => String(s).replace(CTRL, "");

function bar(percentage, width = 18) {
  const filled = Math.round((percentage / 100) * width);
  const empty = width - filled;
  let color = c.green;
  if (percentage > 70) color = c.yellow;
  if (percentage > 90) color = c.red;
  return `${color}${"█".repeat(filled)}${c.dim}${"░".repeat(empty)}${c.reset} ${strip(percentage)}%`;
}

async function fetchStatus() {
  return new Promise((resolve, reject) => {
    http.get(`${SERVER_URL}/api/status`, (res) => {
      let data = "";
      res.on("data", chunk => { data += chunk; });
      res.on("end", () => {
        try {
          resolve(JSON.parse(data));
        } catch (e) {
          reject(e);
        }
      });
    }).on("error", reject);
  });
}

async function sendRoute(prompt) {
  return new Promise((resolve, reject) => {
    const postData = JSON.stringify({ prompt });
    const req = http.request(`${SERVER_URL}/api/route`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "Content-Length": Buffer.byteLength(postData)
      }
    }, (res) => {
      let data = "";
      res.on("data", chunk => { data += chunk; });
      res.on("end", () => {
        try {
          resolve(JSON.parse(data));
        } catch (e) {
          reject(e);
        }
      });
    });
    req.on("error", reject);
    req.write(postData);
    req.end();
  });
}

function render(state, promptResult = null) {
  process.stdout.write(c.clear);
  const now = new Date().toLocaleTimeString();

  console.log(`${c.bold}${c.cyan}┌────────────────────────────────────────────────────────────────────────┐${c.reset}`);
  console.log(`${c.bold}${c.cyan}│${c.reset} ${c.bold}${c.magenta}⚡ OPENCODE GO MULTI-AGENT LIVE ROUTING HUB${c.reset}               ${c.dim}${now}${c.reset} ${c.bold}${c.cyan}│${c.reset}`);
  console.log(`${c.bold}${c.cyan}│${c.reset} ${c.dim}Gateway:${c.reset} ${c.green}${strip(state.provider)}${c.reset}  ${c.dim}Orchestrator:${c.reset} ${c.yellow}${strip(state.orchestrator.model)}${c.reset}   ${c.bold}${c.cyan}│${c.reset}`);
  console.log(`${c.bold}${c.cyan}├────────────────────────────────────────────────────────────────────────┤${c.reset}`);

  // Table Header
  console.log(`${c.bold}${c.cyan}│${c.reset} ${c.bold}AGENT ROLE${c.reset}        ${c.bold}MODEL${c.reset}                  ${c.bold}STATUS${c.reset}   ${c.bold}LATENCY${c.reset}  ${c.bold}WINDOW QUOTA${c.reset}   ${c.bold}${c.cyan}│${c.reset}`);
  console.log(`${c.bold}${c.cyan}├────────────────────────────────────────────────────────────────────────┤${c.reset}`);

  // Agents
  for (const agent of state.agents) {
    const statusIcon = agent.status === "working" ? `${c.yellow}● WORKING${c.reset}` : `${c.green}○ IDLE   ${c.reset}`;
    const namePadded = agent.name.replace(CTRL, "").padEnd(16).slice(0, 16);
    const modelPadded = agent.model.replace(CTRL, "").replace("opencode-go/", "").padEnd(21).slice(0, 21);
    const latPadded = agent.latency.replace(CTRL, "").padEnd(7);
    const quotaBar = bar(agent.quotaUsed, 12);
    console.log(`${c.bold}${c.cyan}│${c.reset} ${c.white}${namePadded}${c.reset}  ${c.cyan}${modelPadded}${c.reset}  ${statusIcon}  ${c.dim}${latPadded}${c.reset}  ${quotaBar} ${c.bold}${c.cyan}│${c.reset}`);
  }

  console.log(`${c.bold}${c.cyan}├────────────────────────────────────────────────────────────────────────┤${c.reset}`);
  console.log(`${c.bold}${c.cyan}│${c.reset} ${c.bold}5-HOUR ROLLING WINDOW QUOTAS (PER MODEL FAMILY)${c.reset}                          ${c.bold}${c.cyan}│${c.reset}`);
  console.log(`${c.bold}${c.cyan}├────────────────────────────────────────────────────────────────────────┤${c.reset}`);

  for (const [family, q] of Object.entries(state.quotas)) {
    const famPadded = family.replace(CTRL, "").padEnd(24);
    const qBar = bar(q.used, 20);
    console.log(`${c.bold}${c.cyan}│${c.reset} ${c.dim}${famPadded}${c.reset} ${qBar}  ${c.dim}Resets in: ${strip(q.resetIn)}${c.reset}    ${c.bold}${c.cyan}│${c.reset}`);
  }

  console.log(`${c.bold}${c.cyan}├────────────────────────────────────────────────────────────────────────┤${c.reset}`);
  console.log(`${c.bold}${c.cyan}│${c.reset} ${c.bold}LIVE TELEMETRY & ROUTING STREAM${c.reset}                                          ${c.bold}${c.cyan}│${c.reset}`);
  console.log(`${c.bold}${c.cyan}├────────────────────────────────────────────────────────────────────────┤${c.reset}`);

  const recentLogs = state.logs.slice(0, 4);
  for (const log of recentLogs) {
    if (log.type === "ROUTED") {
      const line = `[${strip(log.timestamp)}] ${c.yellow}${strip(log.agent)}${c.reset} (${strip(log.model)}) ← "${strip(log.prompt.slice(0, 32))}"`;
      console.log(`${c.bold}${c.cyan}│${c.reset} ${line.padEnd(80).slice(0, 70)} ${c.bold}${c.cyan}│${c.reset}`);
    } else {
      const line = `[${strip(log.timestamp)}] ${c.dim}${strip(log.message || log.type)}${c.reset}`;
      console.log(`${c.bold}${c.cyan}│${c.reset} ${line.padEnd(80).slice(0, 70)} ${c.bold}${c.cyan}│${c.reset}`);
    }
  }

  if (promptResult) {
    console.log(`${c.bold}${c.cyan}├────────────────────────────────────────────────────────────────────────┤${c.reset}`);
    console.log(`${c.bold}${c.cyan}│${c.reset} ${c.green}✔ LAST TEST ROUTED:${c.reset} ${c.bold}${strip(promptResult.targetAgent.name)}${c.reset} (${strip(promptResult.targetAgent.model)})`);
    console.log(`${c.bold}${c.cyan}│${c.reset} ${c.dim}Reason:${c.reset} ${strip(promptResult.decision.reason)}`);
  }

  console.log(`${c.bold}${c.cyan}└────────────────────────────────────────────────────────────────────────┘${c.reset}`);
  console.log(`${c.dim}Type any task description below to test live auto-routing (or 'q' to quit):${c.reset}`);
}

async function start() {
  const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout
  });

  async function refresh(lastResult = null) {
    try {
      const state = await fetchStatus();
      render(state, lastResult);
      rl.prompt();
    } catch {
      console.log(`${c.red}Could not connect to Dashboard Server at ${SERVER_URL}.${c.reset}`);
      console.log(`${c.dim}Make sure 'node opencode-agent-hub/dashboard/server.mjs' is running!${c.reset}`);
    }
  }

  // Initial render
  await refresh();

  // Periodic poll to keep live
  const timer = setInterval(async () => {
    try {
      const state = await fetchStatus();
      render(state);
      rl.prompt();
    } catch {}
  }, 4000);

  rl.setPrompt(`${c.bold}${c.green}opencode-hub > ${c.reset}`);
  rl.prompt();

  rl.on("line", async (line) => {
    const trimmed = line.trim();
    if (trimmed === "q" || trimmed === "exit") {
      clearInterval(timer);
      process.exit(0);
    }
    if (trimmed.length > 0) {
      try {
        const result = await sendRoute(trimmed);
        await refresh(result);
      } catch (e) {
        console.error(`${c.red}Routing error: ${strip(e.message)}${c.reset}`);
        rl.prompt();
      }
    } else {
      rl.prompt();
    }
  });
}

start();
