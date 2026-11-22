import type { Plugin } from "@opencode-ai/plugin";

export const AutoRouterPlugin: Plugin = async () => {
  return {
    event: async ({ event }) => {
      // Intercept user messages and inject subagent delegation if omitted
      if (event.type === "message.send" && event.message?.role === "user") {
        const text = (event.message.content || "").trim();
        const lower = text.toLowerCase();

        // If user already specified an explicit @agent, do not override
        if (text.startsWith("@")) {
          return;
        }

        // Rule-based deterministic routing triggers
        if (lower.startsWith("review") || lower.startsWith("audit") || lower.includes("check diff") || lower.includes("security check")) {
          event.message.content = `@reviewer ${text}`;
        } else if (lower.startsWith("debug") || lower.includes("why is it failing") || lower.includes("fix bug") || lower.includes("error:")) {
          event.message.content = `@debugger ${text}`;
        } else if (lower.startsWith("test") || lower.startsWith("format") || lower.startsWith("lint") || lower.includes("add unit test")) {
          event.message.content = `@fast-fixer ${text}`;
        } else if (lower.startsWith("where") || lower.startsWith("find") || lower.startsWith("locate") || lower.startsWith("search")) {
          event.message.content = `@scout ${text}`;
        } else if (lower.startsWith("design") || lower.startsWith("plan") || lower.startsWith("architect") || lower.startsWith("schema")) {
          event.message.content = `@architect ${text}`;
        } else if (lower.startsWith("docs") || lower.startsWith("document") || lower.startsWith("readme") || lower.includes("changelog")) {
          event.message.content = `@docgen ${text}`;
        } else if (lower.startsWith("implement") || lower.startsWith("build") || lower.startsWith("code") || lower.startsWith("refactor")) {
          event.message.content = `@coder ${text}`;
        }
      }
    },
  };
};

export default AutoRouterPlugin;
