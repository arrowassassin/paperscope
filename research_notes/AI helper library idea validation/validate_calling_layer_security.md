# Validating "Thin Typed Calling Layer" (Idea A) and "Supply-Chain-Hardened Library" (Idea C)

**Scope note.** Two candidate OSS AI helper libraries, assessed against late-2026 evidence. All download figures were pulled by the author directly from the npm registry downloads API and the pypistats.org API on 2026-11-18/19; star counts from the GitHub REST API on the same date. Where a vendor's own claim conflicts with measured data, both are reported. Idea A is a **partially-closed gap in a crowded field**; Idea C is a **real and severe problem whose solution has already been absorbed by package managers and platforms, not by a new library**.

---

## Q1. Is there concrete, cited evidence of real demand for each idea?

### Takeaway
Demand for Idea A is real but is *already being served*, and the strongest 2026 criticism is that provider-agnostic normalization saves less work than it costs. Demand for Idea C is enormous and loudly expressed — two LiteLLM compromises in seven months, 2,500+ organizations exposed, a 938-point HN thread — but the demand is for *assurance and process*, and buyers are routing it to package managers, registries, and vendor enterprise tiers rather than to a new dependency.

### Cited Findings

**Idea A — demand signals (real, but ambiguous)**

- LiteLLM is described as popular precisely because it "solve[s] a real engineering problem... a universal proxy, allowing developers to standardize API calls across dozens of different LLM providers... using a single, unified format" — [Salt Security](https://salt.security/blog/the-ai-supply-chain-is-actually-an-api-supply-chain-lessons-from-the-litellm-breach)
- A maintainer of an app that lets users bring keys for 13 providers states the abstraction "is very much worth it for me," and that he migrated "from LangChain to LiteLLM and never looked back," backed by "over 1000 automated integration tests that check the grid of LLM features (tools, json), model, and provider" — [HN comment, scosman, 2025-03](https://news.ycombinator.com/item?id=43472207)
- A developer calls LiteLLM "the right level of abstraction... I like building my own stuff but writing api wrappers just suck" — [HN comment, laborcontract, 2024-06](https://news.ycombinator.com/item?id=40714350)
- Anthropic's own agent guidance is repeatedly read by HN readers as "just use the model libraries directly or a thin abstraction layer (like litellm maybe?) if you want to keep flexibility to change models easily" — [HN comment, jascha_eng, 2024-12](https://news.ycombinator.com/item?id=42480880)
- LiteLLM's Python package records **99,782,353 downloads in the last 30 days** and 24,853,370 in the last week (measured 2026-11-18) — [pypistats.org/packages/litellm](https://pypistats.org/packages/litellm)
- LiteLLM's GitHub repo has **63,518 stars / 13,406 forks** and was pushed 2026-11-19; its own tagline is now "The fastest, litest AI Gateway. **Rust core with Python SDK.**" — [github.com/BerriAI/litellm](https://github.com/BerriAI/litellm)

**Idea A — counter-evidence on demand (the "swap is trivial" argument)**

- A detailed 2026 HN critique argues the aggregator "promise[s] to save some hypothetical work in the future if your boss asks to change vendors, and it turns out to be very trivial work that is just a regular part of our programming job." Same comment identifies the exact failure mode Idea A proposes to fix as a *cost*: "the API ends up being the common denominator so you lose out on the competitive advantages of each choice, or are forced to use even more complex API logic like `LLM(commonParam1, commonParam2, vendorParams={"vendor1":{"vendorParam1":"blabla"}})` or worse, use hard-coded aggregator provided mappings... that may be incomplete and relies on updates from the aggregator dev." — [HN comment, TZubiri, 2026-09-11](https://news.ycombinator.com/item?id=49662137)
- "LiteLLM had a major security incident recently, and often isn't actually that useful an abstraction" — [HN comment, datadrivenangel, 2026-04](https://news.ycombinator.com/item?id=47706799)
- Asked which LangChain abstractions are worth keeping, a commenter frames the alternative as "OpenAI SDK / LiteLLM" — i.e. even abstraction-skeptics treat a thin multi-provider client as the floor, not a product — [HN comment, never_inline, 2025-12](https://news.ycombinator.com/item?id=46393741)

**Idea C — demand signals (very strong)**

- LiteLLM was compromised **twice in 2026**. March: `litellm` 1.82.7 / 1.82.8 published directly to PyPI, bypassing GitHub, carrying `litellm_init.pth` — a `.pth` file Python executes at interpreter startup, so "the malware ran without any user interaction once the package was installed"; it harvested SSH keys, `.env`, cloud creds, kubeconfigs and then created privileged pods in `kube-system` — [Python Developer Tooling Handbook](https://pydevtools.com/blog/litellm-supply-chain-attack-and-securing-python-dependencies/); advisory [GHSA-92x9-889m-jgmw](https://github.com/advisories/GHSA-92x9-889m-jgmw)
- October: seven malicious `litellm==1.107.0` wheels live on PyPI 11:39–13:23 UTC, plus Docker Hub/GHCR tags `v1.107.0`, `latest`, `main-latest`, `main-stable` — [LiteLLM incident report](https://docs.litellm.ai/blog/security-incident-oct-21-2026); independent teardown: [Microsoft Security Blog, 2026-10-23](https://www.microsoft.com/en-us/security/blog/2026/10/23/inside-the-litellm-1-107-0-supply-chain-compromise-credential-theft-through-python-startup-hooks/)
- Exposure estimates: "2,500+ companies and 434,000 CI/CD pipelines worldwide" reconstructed from ~434,000 files captured by attackers — [CloudSEK](https://www.cloudsek.com/blog/ai-supply-chain-breach-2500-companies-434000-cicd-pipelines); "nearly 2,500 organizations" — [CyberInsider](https://cyberinsider.com/litellm-breach-data-shows-supply-chain-attack-impacted-2488-firms/); "over 2,500 organizations and exposed over 430,000 CI/CD pipelines" — [SecurityWeek](https://www.securityweek.com/over-2500-organizations-impacted-by-litellm-supply-chain-attack/)
- Material victim: Mercor disclosed it was hit by a cyberattack tied to the LiteLLM compromise — [TechCrunch, 2026-03-31](https://techcrunch.com/2026/03/31/mercor-says-it-was-hit-by-cyberattack-tied-to-compromise-of-open-source-litellm-project/)
- Attention: the March "Tell HN" post hit **938 points / 500 comments** — [HN 47501426](https://news.ycombinator.com/item?id=47501426); the October SafeDep write-up hit **596 points / 201 comments** — [HN 50181114](https://news.ycombinator.com/item?id=50181114)
- The compromise was found by accident: Callum McMahon at FutureSearch, testing a Cursor MCP plugin, had LiteLLM pulled in as a **transitive** dependency; the `.pth` payload's fork bomb froze his machine — [pydevtools](https://pydevtools.com/blog/litellm-supply-chain-attack-and-securing-python-dependencies/); primary account [futuresearch.ai](https://futuresearch.ai/blog/litellm-pypi-supply-chain-attack/)
- Post-incident community thread explicitly asking the Idea C question ("what else out there could be as well?") — [r/cybersecurity](https://www.reddit.com/r/cybersecurity/comments/1s2sc4w/after_litellm_being_compromised_whatelse_out/)
- A widely-shared October 2026 HN post catalogues the month's incidents — Railway, LiteLLM, Zeabur, OpenCode (AI API keys exposed), OVHcloud CLI binaries backdoored, Tensorlake npm — and argues open-source trust may be entering a "Kessler Syndrome" — [HN 50189771](https://news.ycombinator.com/item?id=50189771)

**Idea C — demand is not converting into adoption of a hardened *library***

- LiteLLM's own enterprise customers "running the private enterprise release... were **not** impacted" — the paid tier, not a new OSS dependency, was the safety net — [LiteLLM incident report](https://docs.litellm.ai/blog/security-incident-oct-21-2026)
- PyPI changed platform policy after the March attack and now rejects new files on releases older than 14 days — [pydevtools, "Mentioned in"](https://pydevtools.com/blog/litellm-supply-chain-attack-and-securing-python-dependencies/); PyPI's own incident report attributes the vector to an API token exposed via a compromised Trivy GitHub Action — [blog.pypi.org](https://blog.pypi.org/posts/2026-04-02-incident-report-litellm-telnyx-supply-chain-attack/)
- `uv` now ships `uv audit` plus an install-time malware check; `pip` v26+ adds `--uploaded-prior-to`; pip/uv/Dependabot/Renovate/pnpm/npm/Yarn all converged on release-age cooldowns (Dependabot defaults to 3 days, pnpm to 1 day) — [pydevtools: What is a Dependency Cooldown?](https://pydevtools.com/handbook/explanation/what-is-a-dependency-cooldown/)

### Inferences
- Idea A's demand is *proven at the "one API for N providers" level* but the willingness to install a **new** dependency for it is low when LiteLLM already has 63k stars, 100M downloads/month and a Rust core, and when `any-llm`/`aisuite`/`openrouter` sit underneath it as cheaper options.
- Idea C's demand is not in question — the fear is documented, mainstream, and backed by a named corporate breach. The open question is whether the fear attaches to *a library* or to *tooling/policy/vendor contracts*. The evidence in Q2/Q3 points strongly to the latter.
- The strongest single demand signal for a hardening product is the **transitive dependency** angle (a security library got pulled in by an MCP plugin without the developer choosing it). That is a scanner/policy problem, not a "trustworthy client" problem.

### Gaps
- No public data found on what fraction of LiteLLM's 100M monthly downloads are CI/bot traffic vs. real applications; download counts are a weak proxy for installed base.
- No survey or spend data found quantifying developer willingness-to-pay for supply-chain assurance specifically in AI libraries (as opposed to general AppSec budgets). This is the critical missing number for Idea C's business case.
- Could not retrieve the r/cybersecurity thread body (Reddit returned no content); only the search-result summary is cited above, so no specific claim from that thread is relied on.

---

## Q2. What already occupies each space, and how strong is it? Is the gap real or closed?

### Takeaway
Idea A's gap is **narrow and actively contested** — for TypeScript it is essentially closed by AI SDK 7's typed `providerOptions`; for Python there are at least four occupants, but all are either huge-but-bloated (LiteLLM), stale (aisuite), or thin-but-unadopted (mirascope), which is itself the key signal. Idea C's gap is **not a library gap at all**: signing, provenance and cooldowns already exist as platform features, and the two 2026 incidents demonstrated that signing does not deliver the guarantee Idea C would sell.

### Cited Findings

**Idea A — the TypeScript slot is closed**

- Vercel AI SDK published **AI SDK 7 on 2026-06-25**, positioning itself as "the TypeScript SDK for building AI applications... across any model provider," with "over 16 million weekly downloads" claimed at launch — [vercel.com/blog/ai-sdk-7](https://vercel.com/blog/ai-sdk-7)
- AI SDK 7 ships the exact "normalize the call, don't hide the capability" design Idea A proposes: a standardized `reasoning` option that "maps to provider-native reasoning settings" but with explicit permission to "still fall back to provider options when you need more detailed provider-specific reasoning configuration"; and typed escape hatches via `providerOptions: { anthropic: {...} } satisfies AnthropicLanguageModelOptions` — [vercel.com/blog/ai-sdk-7](https://vercel.com/blog/ai-sdk-7)
- AI SDK 7 also adds per-step performance statistics (`responseTimeMs`, `outputTokensPerSecond`, `timeToFirstOutputMs`), lifecycle callbacks exposing `usage`/`finishReason`, and OpenTelemetry — i.e. the "expose token usage and metadata" part of Idea A — [vercel.com/blog/ai-sdk-7](https://vercel.com/blog/ai-sdk-7)
- Measured npm downloads, week of 2026-11-11 to 2026-11-17: `ai` **47,150,887**; `openai` **60,287,773**; `@anthropic-ai/sdk` **72,708,872**; `@ai-sdk/openai` **24,658,537** — [npm downloads API](https://api.npmjs.org/downloads/point/last-week/ai)
- **Discrepancy noted:** Vercel claimed "over 16 million weekly downloads" in June 2026; the registry reports ~47M/week in November 2026. Both are cited; the gap is plausibly five months of growth plus differing counting methods, but this was not verified.

**Idea A — the Python slot is crowded but none of the occupants is dominant *as a thin layer***

| Project | GitHub stars | PyPI downloads / month | Positioning |
|---|---|---|---|
| [BerriAI/litellm](https://github.com/BerriAI/litellm) | 63,518 | 99,782,353 | Gateway + SDK, 100+ providers, Rust core |
| [andrewyng/aisuite](https://github.com/andrewyng/aisuite) | 16,404 | 79,105 | "Simple, unified interface to multiple Generative AI providers"; last push 2026-09-18 |
| [mozilla-ai/any-llm](https://github.com/mozilla-ai/any-llm) | 2,260 | 226,953 | "Communicate with an LLM provider using a single interface" |
| [mirascope/mirascope](https://github.com/mirascope/mirascope) | 1,535 | 20,239 | "The LLM Anti-Framework" |
| [`openrouter` SDK](https://pypistats.org/packages/openrouter) | — | 2,322,674 | Hosted aggregator's own client |

*(stars via GitHub API, downloads via pypistats.org, both 2026-11-18)*

- Adjacent, and relevant because they prove the *structured-output* half of the thesis is a separate, healthier niche: `instructor` has **10,244,752** downloads/month and **14,129 stars** — [pypistats](https://pypistats.org/packages/instructor), [github.com/567-labs/instructor](https://github.com/567-labs/instructor)
- PydanticAI: **6,964,997** downloads/month, **21,280 stars**, "Agents, realtime voice, image generation, embeddings. Every model, eve[ry provider]" — [pypistats](https://pypistats.org/packages/pydantic-ai), [github.com/pydantic/pydantic-ai](https://github.com/pydantic/pydantic-ai)
- BAML (typed/structured-output DSL): **527,645** downloads/month, **9,520 stars** — [pypistats](https://pypistats.org/packages/baml-py), [github.com/BoundaryML/baml](https://github.com/BoundaryML/baml)
- Raw SDKs remain larger than every abstraction: `openai` **384,352,189**/month and `anthropic` **210,739,199**/month — [pypistats openai](https://pypistats.org/packages/openai), [pypistats anthropic](https://pypistats.org/packages/anthropic)
- Notably, the "bloated framework is dead" premise is **not supported by download data**: `langchain` still records **157,123,296** downloads/month and 149,864 stars — [pypistats](https://pypistats.org/packages/langchain), [github.com/langchain-ai/langchain](https://github.com/langchain-ai/langchain). (`langchain` is a dependency of many packages, so this overstates direct usage; but it does not support a collapse narrative.)
- LiteLLM's incumbent response to the security criticism is architectural, not cosmetic: it is migrating to a Rust core — [docs.litellm.ai/rust-migration](https://docs.litellm.ai/rust-migration), [repo tagline](https://github.com/BerriAI/litellm)

**Idea C — signing/provenance is already a platform feature, and it did not stop either attack**

- The March vector was an API token exposed through a compromised Trivy GitHub Action — i.e. an *upstream dependency of the build*, not the package itself — [blog.pypi.org incident report](https://blog.pypi.org/posts/2026-04-02-incident-report-litellm-telnyx-supply-chain-attack/)
- The October vector defeated LiteLLM's *post-March hardening*, which already included "Trusted GitHub Publishers," immutable Docker tags, and cosign signing. The attacker used three sets of stolen GitHub CLI OAuth tokens, one belonging to an org owner, to disable branch protection, force-push a workflow change, and point the releaser at a lookalike repo (`Berri-AI/litellm`) — [LiteLLM incident report](https://docs.litellm.ai/blog/security-incident-oct-21-2026)
- Decisive detail for Idea C: "**The compromised `v1.107.0` images also carry a valid signature from the previous key**" — signing did not distinguish malicious from clean artifacts — [LiteLLM incident report](https://docs.litellm.ai/blog/security-incident-oct-21-2026)
- The root cause was token leakage into logs: "specific situations in which Claude Code will embed a user's GitHub CLI OAuth token into a response body, and in situations where response bodies are logged, these tokens may appear in LiteLLM logs," combined with an internet-exposed internal LiteLLM deployment — [LiteLLM incident report](https://docs.litellm.ai/blog/security-incident-oct-21-2026)
- Independent confirmation that provenance is not a trust guarantee: "Stolen credentials produced valid Sigstore certificates, clearing **633 malicious npm packages**" — [VentureBeat, 2026](https://venturebeat.com/security/npm-sigstore-provenance-stolen-identity-audit-grid-2026)
- Provenance adoption is also low even among security-minded projects: "Even the Sigstore project itself has only **7.2%** of its npm dependencies with provenance" — [Zenn, Package Registry Provenance Status: 2026 Edition](https://zenn.dev/sqer/articles/e4df3d397f5651?locale=en)
- Sigstore infrastructure is mature and free to use (keyless OIDC certs, Rekor transparency log, npm provenance GA 2023, PyPI attestations GA Nov 2024) — [sigstore.dev](https://blog.sigstore.dev/npm-provenance-ga/), [Sigstore explained 2026](https://www.ajayk.sh/blog/sigstore-artifact-signing-explained-2026/)
- LiteLLM moved to **keyless cosign** bound to the release workflow identity, recorded in public Rekor, starting `v1.111.0-dev.3` on 2026-11-15 — [LiteLLM incident report](https://docs.litellm.ai/blog/security-incident-oct-21-2026)

### Inferences
- The TypeScript half of Idea A is a **feature, not a product**: `providerOptions` + typed provider option types + `usage`/`finishReason`/raw metadata is exactly the proposed differentiator, already shipped at 47M downloads/week. A new entrant would be re-implementing AI SDK 7's escape hatch with fewer providers.
- The Python half is a **"thin layer that nobody adopts" pattern**: mirascope ("The LLM Anti-Framework", 1.5k stars, 20k downloads/month) is the closest thing to Idea A and has far less traction than the "bloated" thing it opposes. Positioning as the anti-framework is not sufficient.
- `any-llm` (Mozilla) at 227k downloads/month with only 2.2k stars suggests institutional backing can buy distribution without community mindshare — and also that the ceiling for a pure thin layer is low.
- Idea C is strongest as a **verification/attestation product or a policy gate**, not as a runtime client library: the guarantee users want ("this artifact came from the intended source, built by the intended workflow, with no unexpected deps") cannot be provided by the library's own code, and both 2026 incidents plus the 633-package npm case show signing alone is forgeable-by-credential-theft.

### Gaps
- No data found on how many of `any-llm`'s / `aisuite`'s downloads are real users vs. CI; the star-to-download ratio for both is anomalous.
- No public list of "hardened / reproducible-build" LLM client libraries was found — searches for this category returned nothing. This could mean the space is empty (opportunity) or that nobody has framed the product this way (no demonstrated demand). Cannot distinguish from available evidence.
- Did not verify whether AI SDK 7 exposes **logprobs** and **raw provider response objects** specifically; the blog documents usage, finishReason, performance stats, and providerOptions, but not raw-response passthrough. Treat "AI SDK 7 covers logprobs/raw responses" as unconfirmed.
- Could not obtain a current LiteLLM feature comparison for provider-specific parameter passthrough (`extra_body`-style escape hatches); the docs page was not fetched due to tool budget.

---

## Q3. What would differentiate a new entrant, and what is the realistic adoption path?

### Takeaway
For Idea A, the only defensible differentiators left are (a) **capability truthfulness** — a machine-checkable matrix of what each provider/model actually supports (json_schema vs. json_object, logprobs, reasoning, prompt caching, batch), (b) **zero-magic debugging** — every failure traceable to a raw HTTP request/response, and (c) **near-zero dependency footprint**. Positioning on "thin and typed" alone is already taken and has not won. For Idea C, the realistic path is **not a library at all**: publish the attestation/verification tooling and a documented trust boundary, then monetize assurance (audit, SLA, private registry) the way LiteLLM monetizes its enterprise tier.

### Cited Findings
- The feature-matrix problem is real and acknowledged by an incumbent-adjacent maintainer: supporting 13 providers required "over 1000 automated integration tests that check the grid of LLM features (tools, json), model, and provider. Without them it would still be a mess." — [HN comment, scosman](https://news.ycombinator.com/item?id=43472207)
- The counter-position that a normalization layer costs capability is stated plainly by users: aggregator APIs "end up being the common denominator so you lose out on the competitive advantages of each choice" — [HN comment, TZubiri](https://news.ycombinator.com/item?id=49662137)
- The "swap is a few lines" argument is the main threat to Idea A's value proposition: "Endpoints are largely compatible, so the code change required to switch from one to another is trivial" (quoted and endorsed in the same critique) — [HN comment, TZubiri](https://news.ycombinator.com/item?id=49662137)
- Distribution gravity favors hostnames over libraries: the `openrouter` Python client alone does **2,322,674** downloads/month, i.e. more than `any-llm`, `aisuite`, `mirascope`, `llm` and `baml-py` combined — [pypistats](https://pypistats.org/packages/openrouter)
- For Idea C, the maintainer-side incentives are documented and unflattering: after the March incident, a GitHub issue reporting the compromise "was filed and closed as 'not planned,' suggesting the maintainer account was still compromised at that point" — [pydevtools](https://pydevtools.com/blog/litellm-supply-chain-attack-and-securing-python-dependencies/)
- The migration path that actually worked for the incumbent was **paid isolation**, not better OSS hygiene: a "private LiteLLM registry for enterprise customers" with an embargoed release; OSS users got "pin your version to 1.106.2" — [LiteLLM incident report](https://docs.litellm.ai/blog/security-incident-oct-21-2026)
- Documentation-as-differentiator is plausible: LiteLLM's own first post-mortem had to be retracted in part — "Some [of] our assumptions and beliefs... were later discovered to be inaccurate or incomplete" — which is exactly the "docs require reading source" complaint in the brief — [LiteLLM incident report](https://docs.litellm.ai/blog/security-incident-oct-21-2026)

### Inferences
**Idea A — realistic path**
1. **Do not ship a general-purpose client.** Ship a *capability registry + conformance test suite* that any client (including raw SDKs) can consume. This is the one artifact nobody has and everybody needs (scosman's 1000 tests are the proof of pain), and it does not compete with LiteLLM or AI SDK.
2. If a client is shipped, make the differentiator "**every call is a `(request, response)` pair you can print**" — no hidden retries, no prompt rewriting, no model-name magic. Publish the exact HTTP body in the docs for every example.
3. Target the **bring-your-own-key / multi-tenant SaaS** segment only (scosman's use case), where the abstraction genuinely pays for itself.
4. Adoption path: single-maintainer OSS → HN launch with a live capability-matrix diff → integrate as a *backend* of PydanticAI/Instructor rather than a competitor → enterprise support contracts. Expect mirascope-level traction (1–2k stars) as the base case, not LiteLLM-level.

**Idea C — realistic path**
1. **The library is the marketing, not the product.** A "hardened client" that is 200 lines with zero deps and signed releases is a credible *artifact* but a weak *business*: users can get 80% of it from `uv` cooldowns + `uv audit` + hash pinning today (Q1), for free.
2. The defensible asset is **the trust boundary document plus machine-verifiable attestations**: reproducible-build instructions, SLSA provenance, keyless cosign identity, a published dependency budget, and a documented "what a compromise of this project can and cannot reach."
3. Differentiate by **scope minimization targeted at the actual 2026 attack pattern**: no `.pth` files, no `sitecustomize.py`, no install-time code execution, no post-install scripts, no transitive deps, and a CI that proves it. LiteLLM's October payload worked *because* it abused interpreter startup — a library that structurally cannot do that has a crisp, testable claim.
4. Be honest that signing is insufficient: publish a verification recipe that binds artifact → source commit → workflow identity (the keyless-cosign model), and state that a stolen-credential attacker can forge it. Credibility here is a differentiator because the incumbents overclaimed.
5. Adoption path: target **platform/security teams**, not app developers — the buyers are the people who wrote the 2,500-org exposure reports. Realistic monetization is audit + private registry + SLA.

### Gaps
- No evidence found of any paid product succeeding on "hardened AI client library" positioning; the closest analogues (Socket, Chainguard, SafeDep) sell scanning or hardened *base images*, not AI SDKs. Whether the analog transfers is unverified.
- No data on whether enterprises would accept a new dependency *for the purpose of reducing dependency risk* — the obvious objection ("you are now trusting one more maintainer") was not found addressed anywhere in the sources reviewed.
- Could not find pricing or revenue figures for LiteLLM Enterprise, so the "monetize assurance like LiteLLM did" inference is structural, not evidenced.

---

## Q4. What are the falsifying facts — evidence that would make each idea NOT viable?

### Takeaway
Idea A is falsified as a *standalone product* by three facts: the escape hatch it would sell is already shipped by AI SDK 7, the closest "anti-framework" analogue has ~1.5k stars after years of effort, and the core premise ("swapping providers is hard") is actively disputed by practitioners who call it a few lines of work. Idea C is falsified as a *library* by the fact that its central mechanism — signed releases — was present and useless in the very incident that created the demand, and that the ecosystem's response went to package managers and registries instead.

### Cited Findings

**Falsifiers for Idea A**

- **The differentiator already ships.** AI SDK 7 provides standardized `reasoning` with explicit fallback to provider-native options, typed `providerOptions` (`satisfies AnthropicLanguageModelOptions`), `usage`/`finishReason` in lifecycle callbacks, and per-step latency/throughput stats — [vercel.com/blog/ai-sdk-7](https://vercel.com/blog/ai-sdk-7)
- **The closest positioning has not won.** mirascope, literally "The LLM Anti-Framework," sits at **1,535 stars / 20,239 downloads per month** versus LiteLLM's 63,518 stars / 99.8M downloads per month — [github.com/mirascope/mirascope](https://github.com/mirascope/mirascope), [pypistats](https://pypistats.org/packages/mirascope), [pypistats litellm](https://pypistats.org/packages/litellm)
- **Practitioners dispute the value of the swap.** "Wrapping a specific implementation in a neutral function is something you learn to do in year 1 of programming... it turns out to be very trivial work" — [HN comment, TZubiri](https://news.ycombinator.com/item?id=49662137)
- **The escape-hatch design is itself criticized as the worst of both worlds** — "even more complex API logic like `LLM(commonParam1, commonParam2, vendorParams={...})`" — [HN comment, TZubiri](https://news.ycombinator.com/item?id=49662137)
- **Provider convergence reduces the need.** Providers cluster on OpenAI-compatible endpoints and `/v1/chat/completions` shapes, so a single SDK can talk to many providers with a `base_url` change; the aggregator's remaining value is routing/billing, which is a *gateway* not a *library* — implied by LiteLLM's own pivot to "AI Gateway... Rust core" — [github.com/BerriAI/litellm](https://github.com/BerriAI/litellm)
- **Raw SDK usage dwarfs every abstraction**, so the default is not "adopt a thin helper" but "call the SDK": `openai` 384M/month, `anthropic` 211M/month vs. LiteLLM 100M/month (and LiteLLM is itself pulled in transitively) — [pypistats openai](https://pypistats.org/packages/openai), [pypistats anthropic](https://pypistats.org/packages/anthropic)

**Falsifiers for Idea C**

- **Signing was present and failed.** "The compromised `v1.107.0` images also carry a valid signature from the previous key" — [LiteLLM incident report](https://docs.litellm.ai/blog/security-incident-oct-21-2026)
- **Provenance is forgeable with stolen credentials.** 633 malicious npm packages cleared Sigstore provenance checks — [VentureBeat](https://venturebeat.com/security/npm-sigstore-provenance-stolen-identity-audit-grid-2026)
- **Hardening the release pipeline did not prevent the second breach.** LiteLLM had already moved to a private release repo, Trusted Publishers, immutable tags and image signing after March — [LiteLLM incident report](https://docs.litellm.ai/blog/security-incident-oct-21-2026)
- **The failure was human/credential, not code:** stolen GitHub CLI OAuth tokens leaked into logs of an internet-exposed internal deployment; the fix is token hygiene and secret management, which no client library can supply — [LiteLLM incident report](https://docs.litellm.ai/blog/security-incident-oct-21-2026)
- **The market absorbed the demand without a new library:** PyPI 14-day file-rejection rule, `uv audit`, uv install-time malware check, `--uploaded-prior-to`, cooldowns in 7 package managers — [pydevtools](https://pydevtools.com/handbook/explanation/what-is-a-dependency-cooldown/), [blog.pypi.org](https://blog.pypi.org/posts/2026-04-02-incident-report-litellm-telnyx-supply-chain-attack/)
- **Adoption of the mechanism is low even where it exists**, undercutting "users will pay for signing": 7.2% npm provenance penetration for Sigstore's own dependency tree — [Zenn](https://zenn.dev/sqer/articles/e4df3d397f5651?locale=en)
- **Usage did not punish the compromised project.** LiteLLM's measured 30-day downloads (99.8M) are at or above the ~92–95M/month cited around the March incident — [pypistats](https://pypistats.org/packages/litellm) vs. [pydevtools ("roughly 95 million downloads per month")](https://pydevtools.com/blog/litellm-supply-chain-attack-and-securing-python-dependencies/) and [baygramm citing BleepingComputer at ~92M](https://baygramm.com/insights/blog/litellm-supply-chain-compromise-lessons.html). **Interpretation caveat:** this compares a 30-day figure to monthly figures at different dates and cannot prove growth; it does show no visible collapse.
- **The adjacent "not planned" response** to the March vulnerability report shows maintainer attention, not tooling, is the binding constraint — [pydevtools](https://pydevtools.com/blog/litellm-supply-chain-attack-and-securing-python-dependencies/)

### Inferences
- **Idea A verdict: viable only as a component, not a product.** Keep it if the plan is a capability/conformance registry, or a backend that PydanticAI/Instructor/AI SDK consume. Kill it if the plan is "a better LiteLLM for Python" or "a thinner AI SDK for TypeScript" — both slots are occupied by better-resourced incumbents, and the intended differentiator is already a documented feature of one of them.
- **Idea C verdict: the problem is real, the library is the wrong shape.** A new dependency cannot solve "trust fewer dependencies," and "signed releases" is demonstrably not a guarantee in 2026. The salvageable version is a *radical-minimalism* client (zero deps, no install-time code, no `.pth`, reproducible build, keyless-cosign, published trust boundary) sold as assurance to platform/security teams — with the honest caveat that its security value comes from *what it refuses to do*, not from signatures.
- **Combined play, if one must be chosen:** Idea C's trust posture is a better wedge than Idea A's feature set, because the trust claim is falsifiable by inspection (dependency count, install hooks, build reproducibility) and the security buyers are identifiable. Idea A's claim ("we normalize better") is unfalsifiable in marketing terms and already matched by a 47M-downloads/week incumbent.

### Gaps
- Could not find any post-mortem stating that a *hardened/signed* alternative library gained adoption after either LiteLLM incident. Absence of such reports is weak evidence of no demand, not proof.
- No data on whether LiteLLM's Oct 2026 incident caused measurable download decline in the following weeks; only a single 30-day snapshot was available, so the "no punishment" inference is suggestive at best.
- Did not verify whether `uv audit`'s malware check would have blocked `litellm==1.107.0` specifically (it acts on known detections, so probably not during the 2-hour window) — the pydevtools source says cooldowns and time gates, not detection, are what would have helped.
- The claim that OpenAI-compatible endpoints make swapping trivial is inferred from one HN comment plus general market structure; no provider-by-provider compatibility audit was performed.
