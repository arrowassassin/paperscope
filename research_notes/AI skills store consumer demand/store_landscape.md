# Consumer AI Skills / Connector / MCP App Store Landscape (late 2026)

Scope: products that let non-technical people add capabilities (MCP servers, connectors, agent skills, prompt/workflow packs) to AI assistants with minimal setup. Snapshot date: 18 November 2026. All figures are as reported by the cited source; where sources conflict, both are shown.

---

## What products already let consumers browse and one-click install AI capabilities?

### Takeaway
The space has split into three layers: (1) **first-party app/skill directories inside the assistants** (ChatGPT App Directory, Claude's unified Skills/Connectors/Plugins directory, Cursor Marketplace, VS Code's MCP gallery) which are the only true one-click consumer flows; (2) **third-party MCP registries/directories** (Smithery, MCP.so, MCPpedia, HostedMCP) which are mostly copy-a-command catalogs for developers; and (3) **self-hosted platform marketplaces** (Open WebUI, Dify, n8n) that require running your own instance. The most consumer-friendly independent player, Smithery, was acquired and **shut down its one-click connection layer on 31 Oct 2026**, leaving a directory only.

### Cited Findings

**First-party assistant directories**
- OpenAI launched an in-ChatGPT app directory at `chatgpt.com/apps` on 17 Dec 2025, alongside opening Apps SDK submissions; apps are triggered by @-mention or the tools menu — [OpenAI](https://openai.com/index/developers-can-now-submit-apps-to-chatgpt/); [The Verge](https://theverge.com/news/847067/openai-app-store-directory-sdk-chatgpt)
- OpenAI renamed its "connectors" to "apps" at the same time: chat connectors became "apps with file search", deep research connectors "apps with deep research", synced connectors "apps with sync" — [The Verge](https://theverge.com/news/847067/openai-app-store-directory-sdk-chatgpt)
- Anthropic shipped a **single unified directory** for skills, connectors and plugins on 25 Sep 2026, under Customize → Skills / Connectors / Plugins → Discover. "Click the 'Add' button on any skill"; installed skills are enabled by default and are view-only — [Claude Help Center](https://support.claude.com/en/articles/14328846-browse-skills-connectors-and-plugins-in-one-directory)
- Anthropic's directory also surfaces organisation-provisioned and colleague-published skills/plugins for Team and Enterprise, with owner-controlled publishing ("Open" or "Requires review") — [Claude Help Center](https://support.claude.com/en/articles/14328846-browse-skills-connectors-and-plugins-in-one-directory)
- Anthropic reportedly launched a **public Skills Marketplace on 1 May 2026 with ~600 skills (~90 first-party), one-command install (`claude skills install`), and a 15% cut on paid skills** — [500k.io](https://500k.io/journal/anthropic-skills-marketplace-launch); corroborated as "reported by siliconangle, venturebeat" by [Skill Locker](https://skilllocker.ai/blog/state-of-claude-code-skills-2026). **Caveat:** I could not verify the marketplace URL, CLI or 15% cut against Anthropic primary documentation; the primary Help Center article (Sept 2026) describes an in-product directory with an "Add" button, not a paid marketplace with a revenue share.
- Cursor has a Marketplace with "official plugins and one-click installation"; clicking "Add to Cursor" installs the server and runs OAuth sign-in; community plugins/MCP servers are pointed to `cursor.directory` — [Cursor Help](https://cursor.com/help/customization/mcp); [MCP.so Cursor setup guide](https://mcp.so/blog/cursor-mcp-setup)
- VS Code surfaces MCP servers in the Extensions view via `@mcp`, plus a "Browse" flow with a GitHub feed (`@type:mcp`), showing publisher and server details before install — [VS Code docs](https://code.visualstudio.com/docs/agent-customization/mcp-servers)

**Third-party MCP registries / directories**
- Smithery was "a leading public registry and hosting platform for MCP built on the idea that finding and running an MCP server should take one click"; Arcade.dev acquired it on 5 Aug 2026 — [Arcade.dev](https://arcade.dev/blog/smithery-is-now-part-of-arcade-dev)
- Smithery's MCP Proxy and managed identity were **shut down 31 Oct 2026**: all `run.tools` and `server.smithery.ai` URLs, the API, Connect, Toolbox, accounts, API keys, CLI and SDKs stopped working. Smithery remains "a public (non-proxied) MCP registry" with direct vendor URLs; paid charges since 5 Aug were refunded — [Smithery](https://smithery.ai/future)
- MCP.so lists **20,803 MCP servers (1,536 remote/hosted), 396 MCP clients, 272 agent skills and 177 CLI tools** (counts updated 2026-11-19). Installation is a copy-paste command, e.g. `claude mcp add playwright -- npx @playwright/mcp@latest` — [MCP.so](https://mcp.so/)
- MCP.so also aggregates "Loops" (reusable agent prompts/workflows) and "Agent Skills" (e.g. `obra/superpowers` skills) as browsable categories — [MCP.so](https://mcp.so/)
- The official MCP Registry held **53,553 active servers** on 1 Nov 2026 (54,396 distinct names incl. deprecated; 202,828 version records) — [MCPpedia census](https://mcppedia.com/blog/how-many-mcp-servers-are-there)
- A separate full crawl of the official registry on 13 Nov 2026 counted **63,364 servers / 250,478 versions across 17,240 GitHub accounts**, with one publisher alone accounting for 2,366 servers — [mcp-registry-census](https://github.com/zovo-one/mcp-registry-census)
- HostedMCP claims to track **118,434 active MCP servers** (14 Nov 2026) aggregated from the registry, GitHub, npm and PyPI, of which 62,952 come from the MCP Registry and 28,440 pass its quality bar — [HostedMCP](https://hostedmcp.org/learn/state-of-mcp-november-2026)
- MCPpedia's own directory shows 47,703 listings and states "99.7% of those are registry records" — [MCPpedia](https://mcppedia.com/blog/how-many-mcp-servers-are-there)

**Self-hosted platform marketplaces (consumer-adjacent, not consumer-grade)**
- Open WebUI's community site "hosts hundreds of community-built Tools and Functions you can import into your instance in one click. There is no pip install" — [Open WebUI docs](https://docs.openwebui.com/features/extensibility/community/); install flow is "choose a Tool, click Get, enter your instance URL" — [Open WebUI Tools docs](https://docs.openwebui.com/features/extensibility/plugin/tools)
- Dify's Marketplace brings together "nearly 1,000 plugins and more than 300 templates" — [Dify blog](https://dify.ai/blog/from-discovery-to-response-a-more-connected-dify-marketplace)
- n8n's public template library lists **14,219 workflow templates** — [n8n.io/workflows](https://n8n.io/workflows)

**Infrastructure / picks-and-shovels players**
- Manufact (formerly mcp-use) positions itself as "the cloud for MCP servers and MCP apps", explicitly naming the "Connectors marketplace, ChatGPT / Codex plugins store" as a distribution surface; it is the team behind the mcp-use SDK — [Y Combinator](https://ycombinator.com/companies/manufact)
- Alpic sells build/deploy/monitor for "ChatGPT Apps and MCP servers", plus **Beacon**, an automated pre-submission audit for the ChatGPT store and Claude directory — [Alpic](https://alpic.ai); [Beacon announcement](https://alpic.ai/blog/introducing-beacon-what-dozens-of-chatgpt-app-store-submissions-taught-us)
- MCP's app/widget extension became official in the **MCP 2026-07-28 spec**, which Alpic describes as making the protocol stateless and the app extensions official — [Alpic](https://alpic.ai/blog/mcp-2026-07-28-makes-the-protocol-stateless-and-the-app-extensions-official)

### Inferences
- The consumer-facing "store" is being absorbed by the assistants themselves. Third-party registries are becoming **SEO/discovery surfaces and metadata sources** (feeds into first-party directories), not install runtimes — Smithery's shutdown is the clearest evidence.
- A new consumer store cannot win on catalog size: there are 50k–120k MCP listings and 20k–40k skill listings already. Differentiation must be curation, trust, and cross-assistant "works anywhere" install.
- "Skills" (Markdown + scripts, no server, no OAuth) are structurally cheaper to make consumer-safe than MCP servers, which is why the skills layer (Claude, superpowers) is growing faster in raw artifact count.

### Gaps
- No public count of **ChatGPT apps that are live and actively used** (OpenAI declines to disclose; see next section).
- No verified primary source for the reported Anthropic Skills Marketplace economics (15% cut) or CLI.
- Smithery's pre-acquisition scale (servers, users, installs) was never disclosed publicly; only "tens of thousands of entries" appears in coverage — [neura.market](https://neura.market/news/arcade-acquires-smithery-mcp-registry-execution-layer).

---

## How are these products adopted? (users, downloads, stars, funding)

### Takeaway
Reach at the assistant layer is enormous (ChatGPT ~800M WAU, 1.1B MAU projected), but **capability-level adoption is thin and poorly measured**: MCP SDK downloads are huge (~353M/month) yet the dedicated install tooling sees trivial volume (Smithery CLI ~19k/month), and the ChatGPT app store was publicly described as "off to a sluggish start" six months in. Funding is concentrating in infrastructure (Arcade, Manufact), not in consumer storefronts.

### Cited Findings
- ChatGPT had "800 million weekly active users" at the time of the app directory launch — [VentureBeat](https://venturebeat.com/technology/openai-now-accepting-chatgpt-app-submissions-from-third-party-devs-launches); ~1.1B monthly active users projected for May 2026, with ChatGPT's share of assistant users falling below 50% for the first time — [Gigazine](https://gigazine.net/gsc_news/en/20260617-state-of-ai/); [TechCrunch](https://techcrunch.com/2026/06/16/chatgpts-market-share-slips-below-50-for-first-time)
- ChatGPT's app directory had "more than 300 app integrations" by 30 Mar 2026 — [LA Times / Bloomberg](https://www.latimes.com/business/story/2026-03-30/chatgpt-app-store-falters-six-months-after-launch)
- By 13 Oct 2026 the directory's id families were: 5,609 Apps SDK apps (85%), 899 `plugins_` apps, 103 other (OpenAI connectors), across 13 categories — implying roughly 6,600 total listings, with Productivity the largest category at 1,400 — [OpenApps](https://theopenapps.com/glossary/chatgpt-app)
- OpenAI was approving "nearly 70 apps last week, up from just three to five a day previously" (March 2026) — [LA Times / Bloomberg](https://www.latimes.com/business/story/2026-03-30/chatgpt-app-store-falters-six-months-after-launch)
- `@modelcontextprotocol/sdk` recorded **353,280,700 npm downloads in the 30 days to 17 Nov 2026**; `@playwright/mcp` 39.4M; `mcp-use` 173,223; `@smithery/cli` **18,969** — [npm registry API](https://api.npmjs.org/downloads/point/last-month/@modelcontextprotocol/sdk)
- GitHub: `n8n-io/n8n` 211,269 stars; `langgenius/dify` 161,472; `open-webui/open-webui` 159,062; `punkpeye/awesome-mcp-servers` 98,157; `obra/superpowers` (agent-skills framework) 321,514; `mcp-use/mcp-use` 10,992; `modelcontextprotocol/registry` 7,478; `alpic-ai/skybridge` 2,198 — [GitHub API, retrieved 18 Nov 2026](https://api.github.com/repos/obra/superpowers)
- Registry growth: 456 new server names in Sep 2025 → 1,592 cumulative by Jan 2026 → 16,307 new names in Oct 2026 alone (~30% of all names ever published); 18 namespaces hold 9,337 servers (17.4%) — [MCPpedia](https://mcppedia.com/blog/how-many-mcp-servers-are-there)
- Manufact raised a **$6.3M seed** led by Peak XV (Feb 2026), with Liquid 2 Ventures and Ritual participation; it targets **$2–3M ARR by end of 2026** — [VentureBeat](https://venturebeat.com/infrastructure/manufact-raises-usd6-3m-as-mcp-becomes-the-usb-c-for-ai-powering-chatgpt-and); [Seedtable](https://seedtable.com/companies/manufact/funding-rounds/seed-2026-02)
- Arcade.dev raised a **$60M Series A in June 2026, total funding $72M**, then acquired Smithery in August — [neura.market](https://neura.market/news/arcade-acquires-smithery-mcp-registry-execution-layer)
- Claude Code weekly active users "reportedly surpassed ~2M by May 2026", with an annualized revenue run-rate around $2.5B by Feb 2026 (secondary trackers, not independently audited) — [Skill Locker](https://skilllocker.ai/blog/state-of-claude-code-skills-2026)
- Skill-catalog sizes are wildly inconsistent: ~600 (Anthropic marketplace at launch), ~2,200 (AugmentClaude curated), 23,402 (ClaudeMarketplaces), 39,697 skills + 22,068 agents (Cult of Claude, crawled from GitHub) — [AugmentClaude](https://augmentclaude.com); [ClaudeMarketplaces](https://claudemarketplaces.com/skills); [Cult of Claude](https://cultofclaude.com); [500k.io](https://500k.io/journal/anthropic-skills-marketplace-launch)
- Top-installed skills on one paid marketplace (Agensi) had **116, 65, 49, 36, 30 installs** — i.e. low triple digits at the top of the chart — [Skill Locker](https://skilllocker.ai/blog/state-of-claude-code-skills-2026)
- Instacart is cited as the exception with deeper in-chat payment integration; StubHub/Booking/Uber all complete transactions off-platform — [LA Times / Bloomberg](https://www.latimes.com/business/story/2026-03-30/chatgpt-app-store-falters-six-months-after-launch)

### Inferences
- The ratio between protocol-level adoption (353M SDK downloads/month) and dedicated consumer install tooling (~19k Smithery CLI downloads/month) suggests **most MCP usage is developer-embedded, not consumer-installed**.
- A skills store's realistic early scale is thousands of installs, not millions; the "long tail of 50k servers" is mostly publish-once artifacts (56.2% have a single version; 90.8% of namespaces publish exactly one server — [MCPpedia](https://mcppedia.com/blog/how-many-mcp-servers-are-there)).
- Willingness to pay is unproven: no skills-specific WTP study exists, and the free/OSS layer sets a reference price of zero — [Skill Locker](https://skilllocker.ai/blog/state-of-claude-code-skills-2026).

### Gaps
- **No public install/download counts for first-party directories.** OpenAI does not disclose per-app usage; Anthropic does not publish skill install counts. This is the single biggest measurement hole for sizing consumer demand.
- No public revenue figures for any paid skill marketplace; the "15% cut" and "$5K/mo MRR" ceiling are analyst guesses — [500k.io](https://500k.io/journal/anthropic-skills-marketplace-launch).
- Smithery's user/install numbers were never published; the only hard proxy is the tiny CLI download count.
- MCP server totals conflict by 2x+ (53.5k official / 63.4k crawl / 118k aggregated) because of differing definitions (names vs versions vs packages vs GitHub repos). Do not present a single number as fact.

---

## UX maturity: developer setup or true one-click?

### Takeaway
First-party surfaces (Claude, ChatGPT, Cursor, VS Code) have reached **genuine one-click + OAuth**, but only for capabilities the platform has admitted to its directory. Everything outside those walls still assumes a developer: copy a CLI command, edit `mcp.json`, supply API keys, or self-host. The gap between "Add" in Claude and `claude mcp add ... -- npx ...` is the market's defining UX fault line.

### Cited Findings
- Claude: browse → "Add" → skill appears and is enabled by default; connectors use a "Connect" button that starts an auth flow; skills from the directory are view-only (download + re-upload to edit) — [Claude Help Center](https://support.claude.com/en/articles/14328846-browse-skills-connectors-and-plugins-in-one-directory)
- ChatGPT: apps are triggered by @-mention or the tools menu; connecting discloses what data is shared and the app's privacy policy, and disconnecting "immediately" revokes access — [OpenAI](https://openai.com/index/developers-can-now-submit-apps-to-chatgpt/)
- ChatGPT apps are nonetheless "hidden away", with functionality "limited by the partner companies" — [LA Times / Bloomberg](https://www.latimes.com/business/story/2026-03-30/chatgpt-app-store-falters-six-months-after-launch)
- Cursor: "Add to Cursor" installs the server and runs OAuth; but community servers route users to `cursor.directory` and manual `mcp.json` — [Cursor Help](https://cursor.com/help/customization/mcp); [mcp.so guide](https://mcp.so/blog/cursor-mcp-setup)
- MCP.so's documented install path is a terminal command (`claude mcp add playwright -- npx @playwright/mcp@latest`) — [MCP.so](https://mcp.so/)
- Open WebUI: one-click "Get" import, but you must already run an Open WebUI instance and paste its URL — [Open WebUI docs](https://docs.openwebui.com/features/extensibility/plugin/tools)
- n8n/Dify distribution is workflow/plugin import into a (usually self-hosted) instance; n8n markets 14,219 templates and Dify ~1,000 plugins — [n8n](https://n8n.io/workflows); [Dify](https://dify.ai/blog/from-discovery-to-response-a-more-connected-dify-marketplace)
- Local models are served by the same pattern: Open WebUI "import Tools and Functions … no pip install", but the user owns the server — [Open WebUI docs](https://docs.openwebui.com/features/extensibility/community/)
- Publishing is developer-grade even for consumer stores: submissions require MCP connectivity details, testing guidelines, directory metadata and country settings; reviewers check tool annotations, CSP domains, widget rendering on desktop/iOS/Android, and malformed-input handling; there is **no pre-flight check built into the platforms**, so each fix costs a full review cycle — [OpenAI](https://openai.com/index/developers-can-now-submit-apps-to-chatgpt/); [Alpic/Beacon](https://alpic.ai/blog/introducing-beacon-what-dozens-of-chatgpt-app-store-submissions-taught-us)

### Inferences
- Consumer-grade UX currently depends on **platform admission**, not on the store. A third-party consumer store that ships "install into Claude/ChatGPT/Cursor" without app review will either be blocked or fall back to config files.
- The winning UX primitive in 2026 is the **hosted remote MCP endpoint with OAuth** (no local process, no key paste). Smithery proved demand for this and then exited it — leaving an opening, but also a cautionary tale about proxy economics and liability.
- Skills are the only capability type that installs with zero credentials and zero runtime — the cheapest path to a true consumer flow.

### Gaps
- No published conversion/funnel data (browse → install → retain) for any store.
- No data on how many consumers have ever installed a non-first-party capability; surveys cover AI usage, not capability installation.

---

## Trust and safety features — and where they are weak

### Takeaway
Platforms have built **review gates, permission hints and consent dialogs**, but the underlying supply chain is unaudited: only ~4% of MCP listings have enough public evidence to score "Good" on one independent trust scale, and tool-poisoning attacks succeed at ~36% average across models. The weak points are exactly where a consumer store would be most exposed: post-install behavior, credential handling, and the absence of signing/sandboxing.

### Cited Findings
- OpenAI requires submission guidelines on safety/privacy/transparency, a privacy policy per app, data-minimisation ("only request the information needed"), pre-connection disclosure of data shared, and one-click disconnect that revokes access — [OpenAI](https://openai.com/index/developers-can-now-submit-apps-to-chatgpt/)
- OpenAI's review enforces per-tool `readOnlyHint`, `destructiveHint`, `openWorldHint` annotations with justifications; apps must declare CSP `connectDomains`, `resourceDomains`, `frameDomains`, `redirectDomains`; missing CSP is the most common rejection cause — [Alpic/Beacon](https://alpic.ai/blog/introducing-beacon-what-dozens-of-chatgpt-app-store-submissions-taught-us)
- Rejections can be driven by "false signals" from OpenAI's AI-assisted review, requiring human support intervention — [LA Times / Bloomberg](https://www.latimes.com/business/story/2026-03-30/chatgpt-app-store-falters-six-months-after-launch)
- Anthropic gives org owners publishing policy controls ("Open" / "Requires review"), central provisioning, and view-only directory skills (users cannot silently modify installed skill contents) — [Claude Help Center](https://support.claude.com/en/articles/14328846-browse-skills-connectors-and-plugins-in-one-directory)
- VS Code added a consent dialog that shows the exact command before a one-click MCP install executes; before that, "the command would be instantly triggered" for `npx`/`docker` servers — [den.dev](https://den.dev/blog/vs-code-mcp-install-consent/)
- The same analysis notes the missing primitives: digital signing for MCP servers, sandboxed execution, granular per-path permissions, and community reputation systems — [den.dev](https://den.dev/blog/vs-code-mcp-install-consent/)
- Open WebUI warns users: "Never import Tools or Functions from unknown or untrusted sources. Malicious code can compromise your entire system." — [Open WebUI docs](https://docs.openwebui.com/features/extensibility/plugin/)
- Evidence quality across MCP listings: only **4.3% score ≥60/100** and **two listings score ≥80**; only **0.7% of remote-only listings** reach 60; **77.5% have no readable license** (91.9% for remote listings); 47.1% of remote-only entries link a repository vs 95.5% of package-only — [MCPpedia](https://mcppedia.com/blog/how-many-mcp-servers-are-there)
- GitGuardian found a path traversal in Smithery's Docker build (`dockerBuildPath: ".."`) that exposed a fly.io token granting root code execution across **3,243 apps / 3,000+ hosted MCP servers**, enabling interception of client API keys (e.g. a Brave key captured via tcpdump). Fixed within days; no evidence of exploitation — [GitGuardian](https://blog.gitguardian.com/breaking-mcp-server-hosting/)
- Tool poisoning: MCPTox reports an average attack success rate of **36.5%** across model settings, with up to **72.8%** against some real-world servers; a separate large-scale assessment found **7.2% of active MCP endpoints** vulnerable — [MCPTox (AAAI)](https://ojs.aaai.org/index.php/AAAI/article/view/40895); [arXiv 2512.06556](https://arxiv.org/html/2512.06556v1)
- Arcade's ToolBench benchmark claims "growth has outpaced quality" across MCP tools — [Arcade.dev](https://arcade.dev/blog/smithery-is-now-part-of-arcade-dev)
- Consumer-side trust is a separate problem: **55% of 6,000+ consumers surveyed are "extra cautious about sharing payment information with AI"**, and 96% of regular AI-chatbot shoppers still use other channels — [Criteo via LA Times](https://www.latimes.com/business/story/2026-03-30/chatgpt-app-store-falters-six-months-after-launch)

### Inferences
- Review gates verify **what an app declares**, not what it does after install. A store that adds runtime monitoring, egress logging, or per-tool permission receipts would be differentiated rather than duplicative.
- The strongest consumer trust signal available today is **first-party review + hosted endpoint + OAuth scopes**; the weakest link is local `npx`/`docker` servers, which is where a consumer store should simply refuse to play.
- Signing/verifiable publishers is an unclaimed primitive (den.dev calls it out explicitly), and it is cheap to add at the store layer even if the protocol lacks it.

### Gaps
- No public data on how many apps/skills are rejected, or rejection rates by category.
- No published incident data for ChatGPT apps or Claude skills specifically (only MCP-host compromises and academic attack-success benchmarks).
- I found no evidence of a first-party store performing **runtime** behaviour monitoring of installed third-party capabilities.

---

## Market gaps and complaints

### Takeaway
The complaints cluster into four themes: **discovery is broken** (apps "hidden away", no ranking levers, no analytics), **publishing is painful and opaque** (multi-cycle review, no pre-flight, buggy SDK), **quality is unverifiable** (AI-slop catalogs, license/repo gaps, no trust standard), and **the one-click layer is economically fragile** (Smithery's proxy shutdown, refunds, forced migration). The most-cited consumer objection is simpler: the apps don't do anything the base assistant can't.

### Cited Findings
- Six months after launch, the ChatGPT app store was "off to a sluggish start": 300+ integrations, "hidden away", limited functionality, tedious approval, buggy coding system, no usage data — [LA Times / Bloomberg](https://www.latimes.com/business/story/2026-03-30/chatgpt-app-store-falters-six-months-after-launch)
- Partners hold back: Booking's CEO says referral traffic from ChatGPT is "small" and "it's easier to discover listings on Booking.com"; StubHub requires users to leave chat to buy; Uber requires an "@Uber" prefix and off-platform booking — [LA Times / Bloomberg](https://www.latimes.com/business/story/2026-03-30/chatgpt-app-store-falters-six-months-after-launch)
- Developers get "very limited" analytics because prompts are treated as private data, leaving them "running quite blind"; one founder: "I've got this app live, but I have no idea if it's failing" — [LA Times / Bloomberg](https://www.latimes.com/business/story/2026-03-30/chatgpt-app-store-falters-six-months-after-launch)
- "There isn't yet a proven method to influence how apps are discovered", and OpenAI has begun inserting ads into the chatbot, so partner apps compete with sponsored content — [LA Times / Bloomberg](https://www.latimes.com/business/story/2026-03-30/chatgpt-app-store-falters-six-months-after-launch)
- "The floodgates have not opened yet" — Fractal CTO, on the app ecosystem — [LA Times / Bloomberg](https://www.latimes.com/business/story/2026-03-30/chatgpt-app-store-falters-six-months-after-launch)
- Consumer sentiment on r/ChatGPT: asked whether anyone finds value in the ChatGPT "app" market, a top reply was "No because none of the apps are well developed that I can't just do in the main chatgpt." (thread: "Anyone find value in the ChatGPT 'app' market?") — [Reddit](https://www.reddit.com/r/ChatGPT/comments/1pyjeif/anyone_find_value_in_the_chatgpt_app_market/). *Note: Reddit blocked full-page fetch; quote is from the search-result snippet of that thread.*
- Catalog quality: "Being listed is not an endorsement. With 54,000 names, and over half published once and never re-released, treat a listing as the start of your own checks."; 56.2% of names have a single version — [MCPpedia](https://mcppedia.com/blog/how-many-mcp-servers-are-there)
- Skill-catalog inflation: published sizes range from ~600 to ~70,000, "overlap heavily, mix skills with agents/prompts/MCP servers"; "most 'skills' are prompts in a folder" — [Skill Locker](https://skilllocker.ai/blog/state-of-claude-code-skills-2026)
- Skill sprawl: teams accumulate 40–50 skills in six months and actively use ~6; `CLAUDE.md` reportedly starts being ignored past ~200 lines — [Skill Locker](https://skilllocker.ai/blog/state-of-claude-code-skills-2026)
- Platform risk realised: Smithery told users to migrate to vendor URLs, cancelled subscriptions and refunded all charges since the acquisition — [Smithery](https://smithery.ai/future)
- Review friction is structural: "each submission is a full review cycle… There's no pre-flight check built into the platforms themselves" — [Alpic/Beacon](https://alpic.ai/blog/introducing-beacon-what-dozens-of-chatgpt-app-store-submissions-taught-us)
- Monetisation is unresolved: OpenAI says only that it is "exploring additional monetization options over time, including digital goods" — [OpenAI](https://openai.com/index/developers-can-now-submit-apps-to-chatgpt/); [The Verge](https://theverge.com/news/847067/openai-app-store-directory-sdk-chatgpt)
- Trust, not supply, is the binding constraint for skills: "whoever makes quality legible" wins; consolidation toward 3–4 trusted hubs is a *prediction*, not observed — [Skill Locker](https://skilllocker.ai/blog/state-of-claude-code-skills-2026)

### Inferences
- The unserved job is **"tell me which of these 50,000 things is safe and actually works, and install it into whichever assistant I use."** Nobody currently owns cross-assistant, trust-first, consumer-grade distribution: Smithery retreated to a directory, MCP.so is a copy-paste catalog, and first-party stores are walled gardens with weak discovery.
- A store should optimise for **time-to-first-useful-result** (one click, no keys, no config) and **visible proof of safety** (publisher identity, permission receipts, test results), because both are named failure modes of incumbents.
- Monetisation should assume **free dominates** and paid demand concentrates in vertical/specialist packs and B2B licensing, not impulse consumer purchases — consistent with both the WTP data and the GPT Store's weak revenue share.

### Gaps
- No quantitative consumer survey on *capability installation* (as opposed to AI usage). The 55%/96% Criteo figures are about payments and channel behaviour, not app installs.
- No public retention or churn data for installed apps/skills at any store.
- No reliable data on the share of MCP servers that actually run successfully; MCPpedia explicitly says "We haven't run them."
- Whether consumers will pay for skills at all remains untested; the only pricing evidence is platform self-reporting and one analyst's inference.

---

## Source-quality notes (for the report writer)

- **Strongest primary sources:** OpenAI's submission post, Anthropic's Help Center directory article, Smithery's shutdown notice, Arcade's acquisition post, GitGuardian's disclosure, VS Code docs, Open WebUI docs, MCP.so's own stats page, npm and GitHub APIs.
- **Secondary but credible:** LA Times/Bloomberg investigation (reprinted), The Verge, VentureBeat, TechCrunch.
- **Treat with caution:** 500k.io (unverified specifics about Anthropic's marketplace, likely AI-assisted; some claims are unverifiable), Skill Locker (vendor with commercial interest, but explicitly discloses and labels its own data), Alpic (vendor selling audit tooling, but its review-criteria description is concrete and consistent with OpenAI docs), HostedMCP/MCPpedia (competing directories, each with its own counting method).
- **Conflicting counts to present as ranges, not facts:** MCP servers 53.5k–118k; skills 600–39.7k; ChatGPT apps ~300 (Mar 2026) to ~6,600 (Oct 2026).
