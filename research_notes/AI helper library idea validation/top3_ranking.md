# Ranking the Strongest Open-Source AI Helper Library Opportunities (Late 2026)

*Scope note: this note tests the five candidate ideas supplied by the coordinator (A–E) against the late-2026 evidence base, adds gaps the list misses, and produces a ranked top 3. Star counts quoted below as "observed" were pulled from the GitHub REST API on 18 Nov 2026 unless a different date is stated; they are point-in-time snapshots, not growth rates.*

---

## Key Question 1: Which 3 ideas have the strongest combined demand + feasibility + differentiation signal, with citations?

### Takeaway

The three strongest are **(1) a local-first, no-account audit of the AI/agent dependency surface, shipped alongside a minimal-surface "hardened" client; (2) an MCP server readiness audit whose fixes are proven by re-scoring, backed by an open multi-host rule pack; (3) a thin Go/Rust client whose selling point is failure semantics rather than provider coverage.** The decisive ranking factor was *not* "how big is the gap" — near every gap I probed already has three to six zero-star attempts — but whether the gap has a budget holder, whether a 200-line script can close it, and whether a roadmap-owning incumbent has publicly declined to enter it.

### Cited Findings — the structural finding that reshaped the ranking

- Almost every "unmet need" I tested already has multiple near-zero-traction attempts, which means idea scarcity is not the binding constraint; distribution, trust and maintenance are. Evidence: model-deprecation CI scanners exist as four separate projects with **0, 0, 0 and 3 GitHub stars** ([deprecation-desk](https://github.com/Ort0x36/deprecation-desk), [model-retire](https://github.com/BYDTechSolutions/model-retire), [llm-model-deprecation](https://github.com/techdevsynergy/llm-model-deprecation), [modeldeprecations.dev](https://github.com/mnfst/modeldeprecations.dev)); LLM record/replay "VCR" libraries exist as at least seven separate projects ([llm-vcr](https://github.com/SiluPanda/llm-vcr), [promptecho](https://github.com/shwetank/promptecho), [llm-cassette](https://github.com/gangjig/llm-cassette), [cassette-fn](https://github.com/pjdurden/cassette-fn), [vcr-llm](https://github.com/crithstudio-hash/vcr-llm), [baml_vcr](https://github.com/gr-b/baml_vcr), [llmvcr on PyPI](https://pypi.org/project/llmvcr)); local-first eval harnesses similarly ([gatecheck](https://github.com/satchmakua/gatecheck), [quantiles](https://github.com/quantiles-evals/quantiles)); prompt-versioning as git artifacts similarly ([graver](https://github.com/PracticalMind/graver)).
- The corollary from the prior landscape pass holds: post-launch, agent frameworks converge to roughly 200–600 new stars/month, so a new library's realistic ceiling is set by repeat use, not the launch spike — [Nine Agent Frameworks, Compared with Data and Code](https://blog.mikegchambers.com/posts/agentic-framework-landscape/).
- The single most important market event of 2026 for this question is not an abstraction debate but a security one: LiteLLM shipped a malicious 1.107.0 to PyPI and Docker Hub on **21 Oct 2026** carrying credential theft and remote code loading — [LiteLLM security incident notice](https://docs.litellm.ai/blog/security-incident-oct-21-2026); [Datadog Security Labs analysis](https://securitylabs.datadoghq.com/articles/litellm-compromised-pypi-docker/). This was LiteLLM's **second** compromise of 2026 — [PyPI incident report, 2 Apr 2026](https://blog.pypi.org/posts/2026-04-02-incident-report-litellm-telnyx-supply-chain-attack/).
- The May 2026 "Mini Shai-Hulud" worm compromised **169 npm packages and 2 PyPI packages (373 malicious package-version entries)**, including `mistralai 2.4.6` and `guardrails-ai 0.10.1`, by hijacking legitimate CI/CD — [CSA research note](https://labs.cloudsecurityalliance.org/research/csa-research-note-mini-shai-hulud-ai-supply-chain-20260513-c). Critically, **the published packages carried valid SLSA Build Level 3 provenance attestations** — the same attestations compliance tooling treats as proof of trust — [CSA, Mini Shai-Hulud and provenance](https://labs.cloudsecurityalliance.org/research/csa-research-note-mini-shai-hulud-ai-supply-chain).
- "Finding trustworthy Go modules and packages" is a **top-3 frustration for 26% of Go developers**, alongside idioms (33%) and missing language features (28%) — [Results from the 2025 Go Developer Survey](https://go.dev/blog/survey2025). This is independent, first-party evidence that *dependency trust* — not abstractions — is a named, measured pain point.

**Inferences**
- Provenance/signing is no longer a sufficient differentiator for a "secure" library, because the worm's packages were validly attested. The defensible product is **local, reproducible auditability of behaviour** (what code runs at install time, what changed between versions), which the attacker cannot forge because the user recomputes it.
- Because so many categories already have 0–30-star attempts, an idea with *identical* shape to those attempts should be assumed dead on arrival unless the entrant has a distribution or trust advantage.

### Cited Findings — demand evidence for the named candidates

**A. Thin typed provider-agnostic calling layer exposing provider metadata**
- The pain is real and old: developers wanted token-usage metadata and could not reach it through abstractions ("I wanted access to the token usage metadata in the response... This wasn't possible at the time, or it wasn't well documented") — [HN, Why we no longer use LangChain](https://news.ycombinator.com/item?id=40739982). Cross-provider structured-output gaps documented in LiteLLM issues (#16148, #19444, #5172, #5074) — [BerriAI/litellm issues](https://github.com/BerriAI/litellm/issues/).
- But incumbents are enormous and free: Vercel AI SDK **28,079 stars** ([vercel/ai](https://github.com/vercel/ai)), LiteLLM **63,518** ([BerriAI/litellm](https://github.com/BerriAI/litellm)), Pydantic AI **21,280** ([pydantic/pydantic-ai](https://github.com/pydantic/pydantic-ai)), Instructor **14,129** ([567-labs/instructor](https://github.com/567-labs/instructor)), BAML **9,520** ([BoundaryML/baml](https://github.com/BoundaryML/baml)), Mozilla `any-llm` **2,260** ([mozilla-ai/any-llm](https://github.com/mozilla-ai/any-llm)), plus a Go port of the same concept (["any-llm-go: One Interface for LLMs in Go"](https://blog.mozilla.ai/run-openai-claude-mistral-llamafile-and-more-from-one-interface-now-in-go)).
- The metadata layer itself is already a well-adopted open dataset: `models.dev` has **7,686 stars** ([anomalyco/models.dev](https://github.com/anomalyco/models.dev), formerly `sst/models.dev`) — so a metadata-exposing client adds types and drift-checking, not data.

**B. Reliable retry/streaming helper for Go/Rust**
- There is a documented vacuum: "langchaingo is the one everyone reaches for first and had not been pushed in seven months when this was written. There is no Go equivalent of the Vercel AI SDK and that gap is not one framework's to fill" — [pkg.go.dev: jimmitjoo/tjo/llm](https://pkg.go.dev/github.com/jimmitjoo/tjo/llm).
- The category has proven Go demand at scale: `sashabaranov/go-openai` has **10,812 stars** with **230 open issues**, last push 2026-09-29 ([repo](https://github.com/sashabaranov/go-openai)); `tmc/langchaingo` has **9,784 stars** and **435 open issues** ([repo](https://github.com/tmc/langchaingo)); official `openai/openai-go` has **3,541** and `anthropics/anthropic-sdk-go` **1,247**.
- But provider-agnostic attempts are all pre-traction: `pkieltyka/go-llm` **6 stars** (created Jul 2026), `JoakimCarlsson/ai` **50**, `azrtydxb/go-ai-sdk` **2**, `jimmitjoo/tjo` **1**. Rust is thinner still: `clifton/rstructor` **34 stars** ([repo](https://github.com/clifton/rstructor)).
- **Counter-evidence that materially weakens B:** the Go team's own survey reports **78% of respondents are not building AI-powered features** into their Go software, with 66% reporting no AI functionality at all, and only **11%** working with "ML models, tools, or agents" — and this is a **14-point shift away from** AI feature work versus 2024 (59% uninvolved / 39% involved) — [Results from the 2025 Go Developer Survey](https://go.dev/blog/survey2025). The "22% of Go devs build AI features" figure circulating in library READMEs (e.g. [tjo](https://pkg.go.dev/github.com/jimmitjoo/tjo/llm)) is a misreading of the same survey, which separates 20% "includes AI features I developed myself" from 12% "didn't develop AI features" — [Gigazine summary of the survey](https://gigazine.net/gsc_news/en/20260123-go-developer-survey-2025).
- Demand for the *specific* helper shape is voiced by Go developers, but as a bundle, not a retry library: "Helps me with building and molding prompts... Integrates with the Go test tools for evals and provides relevant tooling around that, as well as best practices" — [r/LLMgophers: "I'm building a new LLM library for Go"](https://reddit.com/r/LLMgophers/comments/1h2kzsa/im_building_a_new_llm_library_for_go).

**C. Supply-chain-hardened helper library**
- Demand is a forced budget line, not a preference: two LiteLLM compromises in one year ([Oct 2026](https://docs.litellm.ai/blog/security-incident-oct-21-2026); [Mar/Apr 2026](https://blog.pypi.org/posts/2026-04-02-incident-report-litellm-telnyx-supply-chain-attack/)), a 169-package npm/PyPI worm hitting first-party AI SDKs ([CSA](https://labs.cloudsecurityalliance.org/research/csa-research-note-mini-shai-hulud-ai-supply-chain-20260513-c)), and valid SLSA L3 attestations on the malicious releases ([CSA](https://labs.cloudsecurityalliance.org/research/csa-research-note-mini-shai-hulud-ai-supply-chain)).
- The existing response is *mirroring*, not library choice: "Echo Libraries provides a pre-secured, continuously maintained source for npm and PyPI packages" — [echo.ai](https://www.echo.ai/blog/protect-against-attacks). Generic SCA and agent-config scanners exist and are crowded: Snyk `agent-scan` (~3.1k stars) "discovers agent configs, scans tool descriptions for injection, poisoning, shadowing, toxic flows" and "needs a Snyk token and sends data to Snyk's API"; Cisco `mcp-scanner` (~1.1k) — both catalogued in [Testing MCP servers in 2026: 31 tools compared and the gaps between them](https://pournasserian.com/writing/mcp-server-testing-tools-2026).
- **Weakness of C as literally stated:** "hardened" is a property a three-month-old repo cannot credibly assert. My reframing in the ranking below converts it from an assertion into a *verifiable audit*, which is why it scores well.

**D. Prompt versioning as git-trackable artifacts without SaaS**
- The need is real but the answer has collapsed to "use git": "My preferred approach is storing prompts in Git. This treats them as software artifacts that are versioned, reviewed, and deployed atomically" — [Hamel Husain, evals FAQ](https://hamel.dev/blog/posts/evals-faq/how-should-i-version-and-manage-prompts.html).
- Every observability incumbent bundles it: LangSmith, PromptLayer, Agenta, Helicone, Braintrust, MLflow, Langfuse, Confident AI, PromptHub all ship prompt versioning, most with Git sync ([PromptLayer comparison](https://www.promptlayer.com/blog/5-best-tools-for-prompt-versioning/); [LangWatch](https://langwatch.ai/blog/best-prompt-management-tools); [MLflow](https://mlflow.org/articles/top-llm-prompt-versioning-platforms-3); [Braintrust](https://www.braintrust.dev/articles/best-prompt-versioning-tools-2025)).
- Open-source attempts are pre-traction: `graver` **2 stars** ("Version control for LLM prompts. Zero dependencies, works offline") — [repo](https://github.com/PracticalMind/graver).

**E. Lightweight SQLAlchemy-level helper (no agent/RAG scope)**
- This is candidate A restated with a metaphor. The metaphor originates in the same HN thread as A's demand evidence — two library archetypes, "Sqlalchemy level" (bridge the gap between code and LLM) vs "Django level" (RAG, patterns, GUI) — [HN comment, jackmpcollins](https://news.ycombinator.com/item?id=42803483). No separate demand evidence exists for "SQLAlchemy-level" as distinct from "thin typed layer"; the market has already voted for the latter shape via Pydantic AI, Instructor, BAML and the AI SDK (stars above).

### New gaps the candidate list misses (assessed and mostly rejected)

- **MCP server readiness/audit — STRONG (kept, ranked #2).** ~**94,600 MCP servers** listed on Glama and "close to half-a-billion downloads a month" across Tier-1 SDKs as of July 2026 ([MCP blog](https://blog.modelcontextprotocol.io/posts/2026-07-28/), via [the 31-tool survey](https://pournasserian.com/writing/mcp-server-testing-tools-2026)); **97.1% of 856 tool descriptions contain at least one defect** ([arXiv 2602.14878](https://arxiv.org/abs/2602.14878), cited in the same survey). The survey's own gap table rates "local-first AI evaluation", "fixes proven by re-scoring" and "an open multi-host rule pack" as **high openness**, and notes no open tool covers Claude + ChatGPT + Cursor/VS Code/Codex/Gemini CLI/Kiro/Windsurf rules together. Decisively, the official Inspector's roadmap **"has no LLM or eval work planned"** and "the Inspector is not a host" — the roadmap-owning incumbent has publicly declined this territory ([Inspector roadmap 2026-H2](https://github.com/modelcontextprotocol/inspector/blob/main/docs/inspector-roadmap-2026-h2.md)).
- **General evals-as-code / local-first eval harness — REJECT (crowded, commoditised).** `promptfoo` **27,438 stars**, `deepeval` **19,554**, `langfuse` **37,613**, `phoenix` **12,350** (observed, repos: [promptfoo](https://github.com/promptfoo/promptfoo), [deepeval](https://github.com/confident-ai/deepeval), [langfuse](https://github.com/langfuse/langfuse), [phoenix](https://github.com/Arize-ai/phoenix)). The category is also consolidating into vendors: the MCP survey reports "Promptfoo's README says it is now part of OpenAI" (second-hand; not independently verified).
- **LLM app testing / record-replay for nondeterministic apps — REJECT.** Seven-plus projects, all pre-traction (citations above). The barrier is trivially low, so no library can hold the position.
- **Token/cost budgeting — REJECT.** Dozens of micro-libraries ([TokenWise](https://github.com/MukundaKatta/TokenWise), [TokenTracker](https://github.com/he-yufeng/TokenTracker), [llm-cost-tracker](https://github.com/fatihkutlar/llm-cost-tracker), [tokonomix](https://github.com/stef41/tokonomix), [LLM-Cost-Guardian](https://github.com/ogulcanaydogan/LLM-Cost-Guardian), [llm-cost-telemetry](https://github.com/sbkuehn/llm-cost-telemetry)) plus every gateway and observability vendor ([Fleece AI comparison](https://fleeceai.company/blog/best-llm-cost-tracking-tools-2026); [Braintrust](https://www.braintrust.dev/articles/best-tools-tracking-llm-costs-2026)). Cost *emission* is settled; cost *control* has no converged pattern — a Go maintainer's explicit reasoning for omitting it is that "the pattern has not converged, so this emits the numbers as span attributes and stops" ([pkg.go.dev](https://pkg.go.dev/github.com/jimmitjoo/tjo/llm)).
- **Observability without vendor lock-in — REJECT as a library.** OpenTelemetry is already the neutral layer and is well staffed: `openllmetry` **7,544 stars**, `openinference` **1,326**, `phoenix` **12,350**, and the GenAI semantic conventions now cover MCP spans and metrics ([OTel GenAI MCP conventions](https://github.com/open-telemetry/semantic-conventions-genai/blob/main/docs/gen-ai/mcp.md)).
- **Local-first inference helpers / structured output for local models — REJECT.** Constrained decoding is a solved, deeply embedded problem (llguidance is integrated into vLLM, SGLang, llama.cpp, Chromium — [microsoft/llguidance](https://github.com/microsoft/llguidance)); Outlines, BAML and Ollama cover the helper layer.
- **Model lifecycle / deprecation CI — REJECT despite excellent demand evidence.** The demand is genuinely severe: ~**82%** of migrations away from retired models were committed *after* the shutdown date, **94%** of migrating apps hardcode model identifiers, and only **3%** had a pre-existing abstraction — [When the Model Retires (arXiv 2609.31288)](https://arxiv.org/html/2609.31288). But four independent projects already implement exactly this and hold **0–3 stars**, which is the strongest available signal that the need is real yet not painful enough to adopt.

### Ranked shortlist

Scoring is my judgment, 1–5 per criterion, from the evidence above. "Gap" = how defensible the remaining space is against named incumbents.

| # | Idea | Demand | Gap | Feasibility | Differentiation | 24-mo durability | Total /25 |
|---|------|--------|-----|-------------|-----------------|------------------|-----------|
| 1 | **C′ — Local-first AI/agent dependency audit + minimal-surface client** | 5 | 4 | 4 | 5 | 4 | **22** |
| 2 | **F — MCP readiness audit, fixes proven by re-scoring, open host rule pack** | 5 | 4 | 4 | 4 | 4 | **21** |
| 3 | **B — Go/Rust thin client sold on failure semantics** | 2 | 3 | 5 | 3 | 4 | **17** |
| 4 | A — Thin typed provider-agnostic layer w/ metadata | 5 | 1 | 3 | 2 | 2 | 13 |
| 5 | E — "SQLAlchemy-level" helper | 3 | 1 | 3 | 2 | 3 | 12 |
| 6 | D — Prompt versioning as git artifacts | 2 | 1 | 5 | 2 | 2 | 12 |

#### #1 — C′: a local-first, no-account audit of the AI/agent dependency surface, plus a minimal-surface client

**Why it ranks first.** It is the only candidate with (a) a *forced* budget line rather than a DX preference, (b) a documented 2026 demand shock that repeated three times, (c) a differentiator the attacker cannot fake (the user recomputes the evidence locally), and (d) an output that screenshots well — a rare combination for an OSS launch.

- Demand: 21 Oct 2026 LiteLLM compromise shipping credential theft ([LiteLLM](https://docs.litellm.ai/blog/security-incident-oct-21-2026)); a second LiteLLM incident in the same year ([PyPI](https://blog.pypi.org/posts/2026-04-02-incident-report-litellm-telnyx-supply-chain-attack/)); 169 npm + 2 PyPI packages in the Mini Shai-Hulud worm, including first-party AI SDKs ([CSA](https://labs.cloudsecurityalliance.org/research/csa-research-note-mini-shai-hulud-ai-supply-chain-20260513-c)).
- Why the obvious defence fails: the malicious Mini Shai-Hulud packages "carry valid SLSA Build Level 3 provenance attestations — the same attestations that compliance frameworks and automated verification tools treat as proof" ([CSA](https://labs.cloudsecurityalliance.org/research/csa-research-note-mini-shai-hulud-ai-supply-chain)). Signing therefore cannot be the product; behavioural audit can.
- The gap against generic SCA is *surface*, not detection: existing agent-focused scanners cover agent configs and tool descriptions (Snyk `agent-scan`) but require an account and exfiltrate config data to a vendor API, while the category's own review concludes that security scanning is "the most crowded segment and the least promising place to compete" **for MCP-specific scanners** — i.e. the defensible position is the dependency/install-time layer that applies to any AI stack, not the MCP prompt-injection layer ([31-tool survey](https://pournasserian.com/writing/mcp-server-testing-tools-2026)).
- Feasibility: no model calls, no provider parity treadmill, no protocol to track — the maintenance surface is lockfile formats and package-manager hooks, which change slowly.
- Cross-validation from an unrelated first-party survey: dependency trust is a top-3 Go frustration at 26% ([Go survey 2025](https://go.dev/blog/survey2025)).

**Main falsifying risk.** It becomes a one-off scan rather than a habit: users run it once after reading about LiteLLM, get a clean-ish report, and never return, so it never compounds into a dependency — and generic SCA vendors (Snyk, Socket) ship equivalent AI-aware rules inside an already-purchased contract. Test cheaply: publish the rule pack and a "replay the LiteLLM 1.107.0 diff" demo *before* writing the client, and measure whether anyone asks to run it on their own repo within two weeks.

#### #2 — F: MCP readiness audit with fixes proven by re-scoring, plus an open multi-host rule pack

**Why it ranks second.** It has the highest absolute demand of any option and the cleanest incumbent abstention, but the highest variance: its value depends on a rule pack becoming a de facto standard, and the protocol underneath it broke twice in twenty months.

- Demand at scale: ~94,600 servers and ~500M monthly SDK downloads ([MCP blog, Jul 2026](https://blog.modelcontextprotocol.io/posts/2026-07-28/)); 97.1% of 856 tool definitions defective ([arXiv 2602.14878](https://arxiv.org/abs/2602.14878)).
- Incumbent abstention, explicitly: the official Inspector's roadmap has "no LLM or eval work planned", with assertions/CI/conformance arriving Nov 2026–Jan 2027 and "the Inspector is not a host" — so the deterministic-testing layer *will* be eaten, but the judge/design/host-audit layer is deliberately out of scope ([roadmap](https://github.com/modelcontextprotocol/inspector/blob/main/docs/inspector-roadmap-2026-h2.md); [31-tool survey](https://pournasserian.com/writing/mcp-server-testing-tools-2026)).
- The specific open cells: "local-first AI evaluation", "fixes, not just findings", "an open multi-host rule pack" — all rated **high** openness, with no open tool covering Claude + ChatGPT + Cursor/VS Code/Codex/Gemini CLI/Kiro/Windsurf ([31-tool survey](https://pournasserian.com/writing/mcp-server-testing-tools-2026)).
- Churn is also a moat: many small tools "went quiet within three to six months of launch" and the funded players (MCPJam, Manufact, Alpic Beacon) keep their judge/host checks cloud-side ([31-tool survey](https://pournasserian.com/writing/mcp-server-testing-tools-2026)).

**Main falsifying risk.** The audience is hobbyist server authors who do not adopt audit tooling — the category's graveyard of 0–30-star tools is evidence — while the paying audience (companies submitting to the ChatGPT store / Claude directory) is small and already served by closed products (Alpic Beacon, Manufact). Secondary risk: the 2026-07-28 stateless redesign plus five spec revisions in twenty months outpaces a small team's maintenance.

#### #3 — B: a thin Go/Rust client whose differentiator is failure semantics, not provider coverage

**Why it ranks third and not higher.** Feasibility is the best of any candidate and the vacuum is documented, but the demand evidence did not survive checking: the Go team's own survey shows AI feature work *declining* (78% not building AI features; 14-point shift away from 2024) — [Go survey 2025](https://go.dev/blog/survey2025). I am ranking it third because a 10.8k-star single-provider client proves the category can be large ([sashabaranov/go-openai](https://github.com/sashabaranov/go-openai)), and because Go's culture rewards exactly the property a new entrant can offer: no churn.

- Vacuum, stated by a competing maintainer: "There is no Go equivalent of the Vercel AI SDK and that gap is not one framework's to fill"; langchaingo "had not been pushed in seven months" ([pkg.go.dev](https://pkg.go.dev/github.com/jimmitjoo/tjo/llm)).
- Proven ceiling and incumbent decay: 10,812 stars with 230 open issues (go-openai), 9,784 with 435 open issues (langchaingo) — observed via GitHub API.
- What must be the differentiator: not retries as such (Go developers write those in an afternoon) but the parts that are genuinely hard and that every existing Go attempt punts on — SSE framing, partial tool-call JSON that is unsafe to act on mid-stream (a maintainer's own rationale: "Partial tool calls are not emitted: a half-parsed JSON argument list is not something a caller can act on" — [pkg.go.dev](https://pkg.go.dev/github.com/jimmitjoo/tjo/llm)), resumable streams, idempotency keys, and token accounting on aborted streams.
- Why retries *alone* is not a product: the Go standard ecosystem already treats retry as a small composable library (`hashicorp/go-retryablehttp`, **2,363 stars**, untouched conceptually since 2015 — [repo](https://github.com/hashicorp/go-retryablehttp)).

**Main falsifying risk.** The TAM is too small to sustain even 200–600 stars/month: only 11% of surveyed Go developers work with ML models/tools/agents, official SDKs (`openai-go`, `anthropic-sdk-go`) are good enough, and the "raw SDK by default" norm means the provider-agnostic layer is a convenience, not a need ([Go survey 2025](https://go.dev/blog/survey2025)).

### Inferences

- The ranking inverts the intuitive ordering: the two ideas with the *best* documented abstraction pain (A, E) rank last, because abstraction pain is the most contested and least monetisable pain in the market.
- Security and publishing-compliance are the only two pains in this landscape with a budget holder who is not the developer's own goodwill, which is why they take the top two slots despite weaker "developer love" signals.
- "Unmet need + no incumbent" is a weak signal in late 2026; "unmet need + incumbent has publicly declined it" (Inspector/evals) or "incumbent's substitute is structurally unable to work" (provenance attestations) is the strong signal.

### Gaps

- No representative survey quantifies how many teams changed dependency policy after the LiteLLM incidents; all security-demand evidence is incident-based, not adoption-based.
- The MCP gap analysis rests substantially on a **single, detailed secondary source** (one author's 31-tool comparison, 1 Oct 2026). It is methodologically transparent and dated, but not independently corroborated; treat the specific openness ratings as one expert's assessment.
- I could not verify "Promptfoo is now part of OpenAI" beyond the survey's report of its README; no primary OpenAI or Promptfoo announcement was retrieved.
- No data on willingness-to-pay for either top idea; both rest on inferred budgets (AppSec, DevRel/publishing) rather than observed transactions.
- Go/Rust adoption data is thin: Go module download counts are not consistently public, and crates.io counts for these packages were not retrievable in this pass.

---

## Key Question 2: Which candidate ideas from the list should be dropped, and why?

### Takeaway

Drop **A, D and E outright**, drop **B as a standalone retry helper** (keep only its failure-semantics reframing), and keep **C only in its auditable form**. The common reason is not that the needs are fake — every one of them is confirmed by real developer complaints — but that each is already served by a free, funded or trivially reimplementable substitute.

### Cited Findings

- **A — drop as a product, salvage the thesis.** Five incumbents already occupy the "thin typed provider-agnostic layer": Vercel AI SDK 28,079 stars, LiteLLM 63,518, Pydantic AI 21,280, Instructor 14,129, BAML 9,520, plus Mozilla `any-llm` 2,260 and a Go port ([vercel/ai](https://github.com/vercel/ai); [litellm](https://github.com/BerriAI/litellm); [pydantic-ai](https://github.com/pydantic/pydantic-ai); [instructor](https://github.com/567-labs/instructor); [baml](https://github.com/BoundaryML/baml); [any-llm](https://github.com/mozilla-ai/any-llm); [any-llm-go](https://blog.mozilla.ai/run-openai-claude-mistral-llamafile-and-more-from-one-interface-now-in-go)). The one thing A claims as differentiation — exposing provider metadata — is now an open dataset with 7,686 stars ([models.dev](https://github.com/models.dev)). Additional structural reason: the incumbents' own philosophy is to keep the API surface conservative and only generalise after three providers implement a concept ("rule of 3"), which is a moat a new entrant must out-execute on 40+ providers — [vercel/ai project philosophies](https://github.com/vercel/ai/blob/83877a1e/contributing/project-philosophies.md).
- **D — drop; git already is the product.** The canonical expert recommendation is to store prompts in git and treat them as ordinary software artifacts ([Hamel Husain](https://hamel.dev/blog/posts/evals-faq/how-should-i-version-and-manage-prompts.html)), and every observability incumbent bundles prompt versioning with git sync (PromptLayer, LangSmith, Agenta, Helicone, Braintrust, MLflow, Langfuse, Confident AI, PromptHub) ([PromptLayer](https://www.promptlayer.com/blog/5-best-tools-for-prompt-versioning/); [MLflow](https://mlflow.org/articles/top-llm-prompt-versioning-platforms-3)). The open-source attempt in this exact niche has 2 stars ([graver](https://github.com/PracticalMind/graver)).
- **E — drop; it is A with a metaphor attached.** The "SQLAlchemy level vs Django level" framing is a single HN comment ([jackmpcollins](https://news.ycombinator.com/item?id=42803483)), and the market has already treated "SQLAlchemy level" as satisfied by typed thin layers (Pydantic AI, Instructor, BAML). There is no separate demand signal for a differently-scoped variant.
- **B — drop "retry/streaming helper" as the pitch.** Retry is a composable commodity in Go (`hashicorp/go-retryablehttp`, 2,363 stars, [repo](https://github.com/hashicorp/go-retryablehttp)), and the addressable population is small and shrinking: 78% of Go developers are not building AI features and only 11% work with ML models/tools/agents, a 14-point decline from 2024 ([Go survey 2025](https://go.dev/blog/survey2025)). Keep B only if re-pitched as a failure-semantics client (rank #3 above).
- **C — keep, but not as stated.** "Hardened" cannot be asserted credibly, and the industry's existing hardening answer is to mirror packages rather than switch libraries ([Echo](https://www.echo.ai/blog/protect-against-attacks)); provenance-based trust was defeated in practice by validly attested malicious releases ([CSA](https://labs.cloudsecurityalliance.org/research/csa-research-note-mini-shai-hulud-ai-supply-chain)). Keep only as verifiable, local audit (rank #1 above).
- **Also dropped from the "new gaps" set**, with reasons: general evals-as-code (promptfoo 27,438 / deepeval 19,554 / langfuse 37,613 / phoenix 12,350 stars); token/cost tracking (dozens of micro-libs plus every gateway — [Fleece AI](https://fleeceai.company/blog/best-llm-cost-tracking-tools-2026)); observability without lock-in (OpenTelemetry/OpenInference own it — [openllmetry](https://github.com/traceloop/openllmetry), [openinference](https://github.com/Arize-ai/openinference)); record-replay testing (7+ pre-traction projects); local-inference structured output (llguidance embedded in vLLM/SGLang/llama.cpp — [microsoft/llguidance](https://github.com/microsoft/llguidance)); model-deprecation CI (4 projects at 0–3 stars despite 82%-late-migration evidence — [arXiv 2609.31288](https://arxiv.org/html/2609.31288)).

### Inferences

- The failure mode to avoid is "correct diagnosis, undefended position": every dropped idea has a true underlying complaint in the HN/LiteLLM-issue record, which is exactly why the space is crowded with well-intentioned clones.
- A useful drop test for any future candidate here: name the incumbent, name what it structurally cannot do, and name who pays. If any of the three is missing, the idea is a 2-star repo.

### Gaps

- No download-level data to distinguish "abandoned" from "quietly used internally" for the tiny projects cited; star counts understate private/internal adoption.
- No evidence on whether prompt versioning demand could be captured by an adjacent product (evals) rather than a standalone tool; the drop rests on substitute availability, not on measured abandonment.

---

## Key Question 3: What is the single most promising idea, and what would its MVP look like?

### Takeaway

**The single most promising idea is C′: a local-first, no-account audit of the AI and agent dependency surface — "what will this package run on my machine, and what changed since the version I approved?"** shipped as a single CLI/library with a versioned, source-linked rule pack, and only later extended into a minimal-surface client. It wins because the demand is a security budget rather than a DX preference, the differentiator (locally recomputed evidence) is exactly what defeated the incumbent trust mechanism in 2026, and the demo is a four-second replay of a real, named incident.

### Cited Findings

- The trigger event is concrete and reproducible: LiteLLM **1.107.0**, published to PyPI and Docker Hub on 21 Oct 2026, "contained compiled malware which steals credentials and loads remote-code execution tools" ([LiteLLM incident notice](https://docs.litellm.ai/blog/security-incident-oct-21-2026)); independent analyses describe Python startup hooks and credential theft ([Microsoft Threat Intelligence](https://www.microsoft.com/en-us/security/blog/2026/10/23/inside-the-litellm-1-107-0-supply-chain-compromise-credential-theft-through-python-startup-hooks/); [Socket](https://socket.dev/blog/litellm-compromise)).
- It is a pattern, not an outlier: LiteLLM was compromised earlier the same year ([PyPI incident report](https://blog.pypi.org/posts/2026-04-02-incident-report-litellm-telnyx-supply-chain-attack/)), and the May 2026 worm hit 169 npm and 2 PyPI packages including first-party AI SDKs, with **valid SLSA Build L3 provenance** ([CSA](https://labs.cloudsecurityalliance.org/research/csa-research-note-mini-shai-hulud-ai-supply-chain-20260513-c)).
- The substitute that exists is organisational, not technical — pre-secured mirrors ([Echo](https://www.echo.ai/blog/protect-against-attacks)) — and generic scanners are account-gated and telemetry-bearing (Snyk `agent-scan` "needs a Snyk token and sends data to Snyk's API") while the category review explicitly calls security scanning the most crowded, least promising place to compete *for MCP-specific scanners* ([31-tool survey](https://pournasserian.com/writing/mcp-server-testing-tools-2026)).
- The "AI dependency surface" is genuinely new and not covered by generic SCA: agents install code at runtime from MCP configs, skills and plugin manifests, and ~94,600 MCP servers exist in one registry with ~500M monthly SDK downloads ([MCP blog](https://blog.modelcontextprotocol.io/posts/2026-07-28/)).
- Credibility is bootstrapped by being an auditor rather than a vendor: the tool asserts nothing about itself, it reports on other people's packages. This sidesteps the trust problem that sinks a new "secure library" and matches the Go community's stated need for "quality signals" on modules, a top-3 frustration at 26% ([Go survey 2025](https://go.dev/blog/survey2025)).
- Prior research fixes the adoption expectation: post-launch, expect convergence to ~200–600 new stars/month, so the MVP must earn *repeat* use, not a launch spike ([Nine Agent Frameworks](https://blog.mikegchambers.com/posts/agentic-framework-landscape/)).

### Proposed MVP (6 weeks, one to two people)

**Positioning:** `npx depaudit` / `uvx depaudit` / `go install …/depaudit` — zero config, zero account, zero telemetry, runs offline after one ruleset fetch.

**v0.1 — the report (weeks 1–3)**
1. **Inputs:** lockfiles (`package-lock.json`, `pnpm-lock.yaml`, `uv.lock`, `poetry.lock`, `go.sum`, `Cargo.lock`) plus agent surfaces (`.mcp.json`, `claude_desktop_config.json`, `.cursor/mcp.json`, skill/plugin directories).
2. **Per-dependency findings**, each mapped to a capability rather than a CVE: install-time code execution (npm `postinstall`/`preinstall`, wheel build hooks, `setup.py`), network egress at build/import, obfuscated or packed source, bundled native binaries fetched post-install, unpinned ranges resolving at install time, telemetry enabled by default, and base-URL override (the exact vector that turns a "gateway" into an exfiltration path).
3. **Output:** human report + SARIF + non-zero exit code, with `--diff <ref>` to gate only what a PR *adds*.
4. **The demo that sells it:** `depaudit diff litellm@1.106.x..1.107.0` fails in under five seconds with "this release adds an import-time hook that reads `~/.aws` and POSTs to an external host".

**v0.2 — the moat (weeks 4–6)**
5. **Rule pack as versioned data:** `rules/*.yaml`, each rule carrying its rationale and a source URL, semver'd and signed; users can pin a ruleset version. This is the asset that compounds and the thing generic SCA cannot copy cheaply for AI-specific surfaces.
6. **Evidence receipts:** hash the analysed tree + ruleset version into a signed receipt a team can attach to a release, so a reviewer can re-run and get the same verdict. This is the direct answer to "provenance was valid but the package was malicious".
7. **Capability-delta view:** the default output for any version bump is "what new capability did this release acquire", not "is it vulnerable" — deliberately out of the CVE business.

**v0.3 — the client (only if v0.1–v0.2 shows repeat use)**
8. A minimal-surface client (single dependency or stdlib-only per language) for 3 providers, plus a `hardened(sdk)` wrapper for teams that keep their existing SDK: pins base URL, disables telemetry, forbids env-var-driven redirects, and refuses to run when a lockfile hash drifts. Ship this *after* the auditor has an audience; launching the client first repeats A's mistake.

**Explicit non-goals:** vulnerability database, runtime sandboxing, prompt-injection scanning of tool descriptions (Snyk/Cisco own it), LLM-judge evals, any hosted dashboard, any account.

**Success gates, in order:** (1) a stranger runs it on their own repo within two weeks of the demo post; (2) ≥20% of runs use `--diff` (proves CI habit, not curiosity); (3) one external team pins a ruleset version (proves the rule pack is the product); (4) only then build the client.

**Main falsifying risk (restated for the MVP):** the audit is consumed as a post-incident one-off and never wired into CI, so it never becomes a dependency; or generic SCA vendors ship equivalent AI-surface rules inside contracts teams already pay for. Both are testable within six weeks and under one person-month, which is the strongest argument for starting here rather than with a client library that needs a year of provider parity before it can be judged.

### Inferences

- The MVP is deliberately an auditor rather than a client because the client market's entry price is parity across dozens of providers, while the auditor's entry price is one honest, reproducible finding.
- Sequencing matters more than scope: shipping the client first would put the project in direct comparison with a 28k-star free SDK; shipping the auditor first puts it in comparison with nothing that is both local and AI-aware.
- If gate (3) fails, the fallback is not "build the client anyway" but "sell the rule pack as a service to platforms" — which is how this category's funded players (MCPJam, Manufact, Beacon) actually monetise, per the [31-tool survey](https://pournasserian.com/writing/mcp-server-testing-tools-2026).

### Gaps

- No public data on how many organisations have an AppSec budget line that a $0 OSS tool can capture versus a paid scanner; the budget inference is reasoned, not observed.
- No measured conversion from "security tool stars" to sustained usage; the 200–600 stars/month baseline comes from agent frameworks, not security tooling, and may not transfer.
- I did not verify whether npm/PyPI already expose install-hook metadata in a machine-readable form sufficient for step 2 without downloading and unpacking artifacts; if they do, the MVP's hardest engineering step gets materially cheaper (and the moat correspondingly thinner).
