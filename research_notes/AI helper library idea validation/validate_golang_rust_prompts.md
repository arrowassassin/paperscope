# Validation: Go/Rust reliability helper (B), git-native prompt versioning (D), "SQLAlchemy-level" LLM helper (E) — late 2026

Scope note: all figures were pulled directly from primary sources (crates.io API, GitHub API, PyPI Stats, npm registry API) on 2026-11-18/19. Star counts and download counts are point-in-time snapshots, not trends, and no source I found publishes Go module download counts (see Gaps). Where I could not verify a number, I say so rather than estimate.

---

## Q1. Is there concrete, cited evidence of real demand for each idea?

### Takeaway
Demand evidence is strongest but *narrowest* for Idea D (many practitioners describe prompt-versioning pain in threads, yet almost nobody adopts a dedicated tool for it). Idea B has hard, documented bug evidence but the bugs are being fixed at the source within days, and the Go/Rust populations that would consume a fix are small. Idea E has the clearest conceptual demand signal (a widely-upvoted framing of exactly this layer) but the least evidence that anyone will pay attention to a *new* entrant, because that layer already has popular incumbents.

### Cited Findings

**Idea B — reliable retry/streaming helper for Go/Rust**

- Instructor issue #1991 (opened 2026-01-12, labelled `priority:critical`): streaming (`create_partial` / `Partial[Model]` with `stream=True`) plus `max_retries > 1` crashes with `'Stream' object has no attribute 'choices'` when validation fails; maintainer's own workaround was "use `max_retries=1` when streaming" — [github.com/567-labs/instructor/issues/1991](https://github.com/567-labs/instructor/issues/1991)
- That bug was fixed and released the next day: commit "fix(streaming): handle Stream objects in reask handlers (#1992)" merged 2026-01-13, released as v1.14.3 the same day — [commit bf9421b](https://github.com/567-labs/instructor/commit/bf9421b11c05b162fb3eed8f99e333b1900f9620), [release v1.14.3](https://github.com/567-labs/instructor/releases/tag/v1.14.3)
- As of 2026-04-12 an open PR (#2263) still describes "buffering the full partial stream" and a need for "a safer retry fallback when a streaming response cannot be inspected by provider-specific reask helpers" — i.e. partial-streaming semantics remained contested months later — [github.com/567-labs/instructor/pull/2263](https://github.com/567-labs/instructor/pull/2263)
- Validation errors not fed back to the model: issue #1736 (2025-07-22) reports retries for a `ValidationError` "is not adding the error back into the prompt" because the `except` block catches pydantic's `ValidationError` but not `instructor.exceptions.ValidationError`; closed 2026-01-24 with "Retry now catches validation errors and feeds them back" — [github.com/567-labs/instructor/issues/1736](https://github.com/567-labs/instructor/issues/1736)
- Related retry-path defect: issue #1856 (2025-10-20), JSON/validation errors wrapped as `InstructorValidationError` bypassed the retry handler's `except (ValidationError, JSONDecodeError)` block — [github.com/567-labs/instructor/issues/1856](https://github.com/567-labs/instructor/issues/1856)
- Partial-streaming defect still open in substance: issue #1299, tokens ending in `\` yield empty fields; maintainer response 2025-10-27: "This is a known limitation... Accepted as edge case" — [github.com/instructor-ai/instructor/issues/1299](https://github.com/instructor-ai/instructor/issues/1299)
- Older retry-path crash: issue #693 (2024-05-21) reports `UnboundLocalError` in `retry.py` on the last attempt — [github.com/jxnl/instructor/issues/693](https://github.com/jxnl/instructor/issues/693)
- Instructor's Python package is heavily used, so these are not theoretical: 10.24M PyPI downloads in the last 30 days — [pypistats.org/packages/instructor](https://pypistats.org/packages/instructor)
- Go-side reliability gaps exist in the dominant general framework: langchaingo has 435 open issues/PRs, including an open i/o timeout report (#550) and a closed "Ollama Package: Not handling context timeout issues on streaming response" (#948) — [github.com/tmc/langchaingo/issues/550](https://github.com/tmc/langchaingo/issues/550), [issues/948](https://github.com/tmc/langchaingo/issues/948); a streaming SSE-comment handling fix landed as PR #1366 (2025-08-18) — [pull/1366](https://github.com/tmc/langchaingo/pull/1366)
- Rust: `rig` has 158 issues/PRs matching "streaming retry", including a 2026-10-17 feature request about interrupting a streaming turn and a 2026-10-05 PR bounding malformed-tool-argument retries — [github.com/0xPlaygrounds/rig/issues/2842](https://github.com/0xPlaygrounds/rig/issues/2842), [pull/2727](https://github.com/0xPlaygrounds/rig/pull/2727). Note this is a keyword search, not 158 distinct retry bugs.
- Demand-side signal that Go developers are actively choosing Go for LLM work (the addressable population): r/golang threads "Why we chose Go over Python for building an LLM gateway", "Replace Python with Go for LLMs?", "Why we built our LLM workflow runtime in Go instead of Python" — [reddit.com/r/golang/comments/1r27pqx](https://www.reddit.com/r/golang/comments/1r27pqx/), [.../1lfr9hi](https://www.reddit.com/r/golang/comments/1lfr9hi/), [.../1t6jd7c](https://www.reddit.com/r/golang/comments/1t6jd7c/); HN "A case for Go as the best language for AI agents" — [news.ycombinator.com/item?id=47222270](https://news.ycombinator.com/item?id=47222270)

**Idea D — prompt versioning as git-trackable artifacts**

- Practitioner thread (r/LangChain, 2026-03-12, 11 comments) frames the exact tradeoff: "Route 1: Keep it all in Git... great because it uses your existing CI/CD and you get an audit trail. But it is painfully slow... It completely bottlenecks experimentation." — [reddit.com/r/LangChain/comments/1rrkrfi](https://www.reddit.com/r/LangChain/comments/1rrkrfi/hardcoding_prompt_templates_is_a_nightmare_how/)
- r/AI_Agents (2026-03-13, 6 points, 6 comments): team uses Langfuse but "There have been instances where Langfuse was down, which meant our application couldn't fetch prompts and it ended up blocking the app"; asks whether "a Git-like workflow might be a better approach" — [reddit.com/r/AI_Agents/comments/1rsji8z](https://www.reddit.com/r/AI_Agents/comments/1rsji8z/prompt_management_in_production_langfuse_vs_git/)
- r/PromptEngineering (2026-09-16, 15 comments): "does anyone actually do this in practice or does everyone just have a messy prompts folder like me"; a commenter describes keeping prompts in git and running every change against a fixed task set — [reddit.com/r/PromptEngineering/comments/1whta0r](https://www.reddit.com/r/PromptEngineering/comments/1whta0r/does_anyone_version_control_their_prompts_like/)
- A production write-up documents the concrete failure: classification accuracy fell from ~88% to 71% and "I could not reconstruct exactly which version of the prompt was running when quality was good"; the author's chosen fix was a `/prompts` directory in git plus an eval gate — [mohakdeepsingh.dev](https://mohakdeepsingh.dev/blog/how-i-actually-version-prompts-production-not-ideal-system)
- Same author documents *why* a prompt database failed: renaming `{{contract_text}}` to `{{document_text}}` became invisible to the prompt DB and "broke at runtime with a KeyError. With git, that kind of change shows up in the same diff as the code change" — [mohakdeepsingh.dev](https://mohakdeepsingh.dev/blog/how-i-actually-version-prompts-production-not-ideal-system)
- Counter-evidence that the pain is real but the *git* solution is contested: "Why Your Prompts Don't Belong in Git" (Towards Data Science, 2025-08-25) — blocks non-engineers, and "Git might show you the diff, but not the outcome" — [towardsdatascience.com](https://towardsdatascience.com/why-your-prompts-dont-belong-in-git/)
- Longer argument that git cannot represent deployment state or eval linkage, so prompt management "needs a lot more than diffs" — [Medium, 2026-06-02](https://sajithatharaka.medium.com/why-git-is-the-wrong-tool-for-managing-your-prompts-a670653bc2f5)
- Practitioner report that git-only workflows break at team scale: "Git for templates turns into a complete mess once you get past a handful of them... Version conflicts everywhere" — [community.latenode.com](https://community.latenode.com/t/managing-prompt-templates-in-git-repositories-with-langsmith-any-success-stories/39403)
- A PM's account of the two-paradigm problem (design-time git vs run-time config), including accidentally overwriting API-edited prompts on redeploy — [ilyazakharau.com](https://ilyazakharau.com/blog/ai-pm-maintaining-prompts)
- Someone built essentially this product and reports: "Pretty much no users so far (admittedly, haven't done much marketing either)"; the HN post received 3 points — [news.ycombinator.com/item?id=49232345](https://news.ycombinator.com/item?id=49232345)

**Idea E — lightweight "SQLAlchemy-level" helper**

- The framing itself comes from an HN comment (2025-01-23) that predicts two tiers: "The 'Sqlalchemy' level: just bridge the gap between code and LLM" vs "The 'Django' level: additionally handle how data is indexed for RAG... possibly GUI, support for plugins. It seems langchain and llamaindex are aiming to be the latter" — [news.ycombinator.com/item?id=42803483](https://news.ycombinator.com/item?id=42803483)
- The same thread contains the demand-side complaint that motivates thin libraries: "the abstractions work well only if I stayed on a narrow path (the quick starts generally worked great!)" and "Most of them are very brittle abstractions" — [news.ycombinator.com/item?id=42803483](https://news.ycombinator.com/item?id=42803483)
- An adjacent HN comment argues for small composable pieces over frameworks: "advocating for more piecemeal tools that can be refactored or replaced, rather than buying into a whole framework" — [news.ycombinator.com/item?id=36645575](https://news.ycombinator.com/item?id=36645575)
- Direct proof the "just bridge code and LLM" niche is commercially real: LiteLLM (a thin-ish unified client that has since grown a gateway) has 63,518 stars and 99.8M PyPI downloads/month — [github.com/BerriAI/litellm](https://github.com/BerriAI/litellm), [pypistats.org/packages/litellm](https://pypistats.org/packages/litellm)
- Mirascope explicitly brands itself "The LLM Anti-Framework" — evidence the positioning resonates enough to name a product after — [github.com/Mirascope/mirascope](https://github.com/Mirascope/mirascope)
- But its traction is modest: 1,535 stars and 20,239 PyPI downloads/month — [github.com/Mirascope/mirascope](https://github.com/Mirascope/mirascope), [pypistats.org/packages/mirascope](https://pypistats.org/packages/mirascope)

### Inferences
- Idea B's bug evidence is genuine and specific, but it is evidence about *Python Instructor*; no source I found shows the same failure modes being reported at volume in Go or Rust libraries. The pain is real; the transplant to Go/Rust is an assumption, not a documented demand.
- Idea D has the most *articulated* pain and the least *revealed* demand: many people describe the problem, several built tools for it, and the tools have near-zero adoption. This pattern usually means the job is being done by plain files + code review rather than by a dependency.
- Idea E is validated as a *category* (LiteLLM, aisuite, Mirascope, `llm` all exist and have users) but that is bad news for a new entrant: the category is proven, so the incumbents are the obstacle, not the absence of demand.

### Gaps
- No public Go module download statistics exist (pkg.go.dev does not publish them; deps.dev's `:dependents` endpoint returned HTTP 404 for every Go package I tried, and pkg.go.dev reported "Imported by: 0" even for langchaingo, which is clearly wrong/unpopulated). Go adoption therefore has to be inferred from GitHub stars and issue volume, which is weaker.
- I could not find a single GitHub issue, Reddit thread, or HN comment that explicitly asks for a *Go or Rust* retry/streaming helper. The Go/Rust demand case rests on general "we chose Go for LLM work" threads plus inferred portability of Python bugs.
- I found no survey data quantifying how many teams keep prompts in git vs a platform; all Idea D demand evidence is anecdotal forum/blog material.

---

## Q2. What already occupies each space, and is the gap real or already closed?

### Takeaway
Idea B's gap is real but narrow and closing: Go/Rust have no dominant *thin structured-output/retry* helper, yet each ecosystem already has a large general library (langchaingo, rig) plus official SDKs, and the reference implementation (Instructor) fixes these bugs in ~1 day. Idea D's gap is arguably already closed by "plain files + promptfoo/Langfuse", and the dedicated tools that exist have no traction. Idea E's gap is closed many times over at the low level.

### Cited Findings

**Idea B — Go/Rust**

| Library | Ecosystem | Stars | Downloads | Notes |
|---|---|---|---|---|
| instructor-go | Go | 223 | n/a (no module stats) | Official Instructor port; last push 2026-11-04; still an untagged pseudo-version (`v0.0.0-20251005202905`) — [GitHub](https://github.com/instructor-ai/instructor-go), [pkg.go.dev](https://pkg.go.dev/github.com/instructor-ai/instructor-go) |
| langchaingo | Go | 9,784 | n/a | Full LangChain port (chains/agents/RAG/vectorstores); v0.1.15; 435 open issues/PRs — [GitHub](https://github.com/tmc/langchaingo), [pkg.go.dev](https://pkg.go.dev/github.com/tmc/langchaingo) |
| sashabaranov/go-openai | Go | 10,812 | n/a | De-facto community OpenAI client, zero-dependency — [GitHub](https://github.com/sashabaranov/go-openai) |
| openai/openai-go (official) | Go | 3,541 | n/a | Official SDK — [GitHub](https://github.com/openai/openai-go) |
| anthropic-sdk-go (official) | Go | 1,247 | n/a | Official SDK — [GitHub](https://github.com/anthropics/anthropic-sdk-go) |
| ai-go | Go | 3 | n/a | Structured-output/extractor API modelled on Rig; tiny — [GitHub](https://github.com/open-ai-sdk/ai-go), [docs](https://open-ai-sdk.github.io/ai-go/core/structured-output) |
| structllm | Go | 0 | n/a | Created 2026-05-24, self-described "v0.1.0 — usable. Not yet battle-tested" — [GitHub](https://github.com/mohsenm4/structllm) |
| rig / rig-core | Rust | 9,180 | 4.54M total / 2.24M last 90d | "Opinionated library for building LLM powered applications" (agents, RAG, extractors) — [GitHub](https://github.com/0xPlaygrounds/rig), [crates.io](https://crates.io/crates/rig-core) |
| async-openai | Rust | 2,037 | 11.03M total / 3.60M last 90d | Provider client only — [crates.io](https://crates.io/crates/async-openai) |
| genai | Rust | n/a | 633k total / 314k last 90d | Multi-provider client — [crates.io](https://crates.io/crates/genai) |
| rstructor | Rust | n/a | 10,649 total / **5,436 last 90d** | "The Rust answer to Python's Pydantic + Instructor"; derives schema, retries on validation errors — [crates.io](https://crates.io/crates/rstructor) |
| openai_dive | Rust | n/a | 146k total / 9.7k last 90d | OpenAI client with Structured Outputs support — [crates.io](https://crates.io/crates/openai_dive) |

- The Python reference point dwarfs all of these: Instructor 14,129 stars and 10.2M downloads/month — [GitHub](https://github.com/567-labs/instructor), [pypistats](https://pypistats.org/packages/instructor)
- Instructor's issue backlog is small relative to usage: 29 open issues/PRs against 14k stars, with critical bugs closed within a day (#1991 → v1.14.3) — [GitHub](https://github.com/567-labs/instructor), [issue #1991](https://github.com/567-labs/instructor/issues/1991)
- Provider-native structured outputs are absorbing the "get typed JSON out" job: OpenAI documents strict `json_schema` response formats and `zodTextFormat`/`parse_as` helpers in its own SDKs — [developers.openai.com structured outputs](https://developers.openai.com/api/docs/guides/structured-outputs)
- Multiple new Go entrants in 2026 all claim the same feature set (struct→schema, native structured outputs, retry-with-error-feedback, streaming): structllm (May 2026), looper-agent (May 2026), elelem (Aug 2026), langrails, ai-go — e.g. [structllm](https://github.com/mohsenm4/structllm), [looper-agent](https://github.com/cuatroochenta-idi/looper-agent), [elelem](https://github.com/psyb0t/elelem), [langrails](https://promptrails.github.io/langrails/). All are at or near zero stars; this is fragmentation, not an empty field.

**Idea D — prompt versioning**

| Tool | Form | Stars / downloads | Position |
|---|---|---|---|
| promptfoo | CLI/CI eval + declarative prompt configs | 27,438 stars; 4.83M npm downloads/month | Evals-first, prompts live in YAML in repo — [GitHub](https://github.com/promptfoo/promptfoo), [npm](https://api.npmjs.org/downloads/point/last-month/promptfoo) |
| Langfuse | Self-host/cloud platform | 37,613 stars | Prompt management + observability; runtime fetch — [GitHub](https://github.com/langfuse/langfuse) |
| Microsoft POML | Prompt markup language | 4,959 stars; last push 2026-01-14 | Structured prompt markup — [GitHub](https://github.com/microsoft/poml) |
| promptfile | PyPI YAML prompt files, "No servers. No SaaS. No database." | 0 stars, 1 commit; 22 PyPI downloads/month | Almost exactly Idea D — [GitHub](https://github.com/maheshmakvana/promptfiles), [pypistats](https://pypistats.org/packages/promptfiles) |
| PromptOps | "Git-native prompt management", PyPI | 177 downloads/month | [pypistats](https://pypistats.org/packages/llmhq-promptops) |
| Sufleur | npm-style prompt registry + typed codegen, git-adjacent | "Pretty much no users so far"; 3 HN points | [HN](https://news.ycombinator.com/item?id=49232345) |
| promptfiles (npm) | — | 22 downloads/month | [npm](https://api.npmjs.org/downloads/point/last-month/promptfile) |
| BAML | Prompt/programming language with schema-checked outputs | 9,520 stars | Adjacent (compiler, not versioning) — [GitHub](https://github.com/BoundaryML/baml) |
| Guidance | Constrained generation | 21,827 stars; last push 2026-05 | Adjacent — [GitHub](https://github.com/guidance-ai/guidance) |

- The de-facto incumbent for "git-trackable prompts" is not a library at all: it is a directory of files plus the repo's existing PR review, as described by practitioners — [mohakdeepsingh.dev](https://mohakdeepsingh.dev/blog/how-i-actually-version-prompts-production-not-ideal-system), [HN comment on jj/git workflows](https://news.ycombinator.com/item?id=48063548)
- Mainstream 2026 roundups of prompt-management tools list platform products (LangWatch, PromptLayer, Langfuse, LangSmith, Braintrust) and do not treat "git-native library" as a category — [langwatch.ai](https://langwatch.ai/blog/best-prompt-management-tools), [agenta.ai](https://agenta.ai/blog/top-open-source-prompt-management-platforms), [pydantic.dev](https://pydantic.dev/articles/best-prompt-management-tools)

**Idea E — low-level bridge**

- LiteLLM: 63,518 stars, 99.8M PyPI downloads/month — the low-level "call any provider" layer is already owned by a very large project that has expanded into a gateway — [GitHub](https://github.com/BerriAI/litellm), [pypistats](https://pypistats.org/packages/litellm)
- aisuite (Andrew Ng): 16,404 stars, "Simple, unified interface to multiple Generative AI providers" — [GitHub](https://github.com/andrewyng/aisuite)
- Simon Willison's `llm`: 12,779 stars, CLI + Python library that is deliberately minimal — [GitHub](https://github.com/simonw/llm)
- `ell` ("A language model programming library"): 5,844 stars, last push 2025-06 (stale) — [GitHub](https://github.com/MadcowD/ell)
- Mirascope ("The LLM Anti-Framework"): 1,535 stars, 20,239 downloads/month — [GitHub](https://github.com/Mirascope/mirascope), [pypistats](https://pypistats.org/packages/mirascope)
- magentic (the project whose author made the SQLAlchemy argument): 2,429 stars — [GitHub](https://github.com/jackmpcollins/magentic)
- any-llm (Mozilla AI): 2,260 stars, "Communicate with an LLM provider using a single interface" — [GitHub](https://github.com/mozilla-ai/any-llm)
- The bloat the idea is reacting against is measurable: langchaingo carries chains/agents/memory/vectorstores/documentloaders in one module — [pkg.go.dev directory listing](https://pkg.go.dev/github.com/tmc/langchaingo)

### Inferences
- Idea B: "no dominant helper" is true only if you define the category narrowly as *thin structured-output + retry*. If you define it as "Go/Rust library for LLM apps", rig (4.5M crate downloads) and langchaingo (9.8k stars) are dominant. A new entrant would be competing for a sub-category that 5+ zero-star Go projects entered in 2026 alone.
- Idea D: the gap is not "no tool exists", it is "no tool has been adopted". promptfile already implements the described feature set (YAML, render, diff, validate, registry, CLI, CI) and has 0 stars; that is close to a controlled experiment showing the feature set is not the bottleneck.
- Idea E: the niche is not empty, it is *commoditized*. Every incumbent already does "string handling, API calls, loops, structured output", and several are free and enormous.

### Gaps
- crates.io `recent_downloads` is a 90-day figure; total downloads include CI/mirror traffic, so ratios between crates are more meaningful than absolute values. I could not isolate unique users.
- No download or dependent counts exist for Go modules, so I cannot rank Go libraries by actual use.
- I could not verify current Python download counts for `promptops`/Sufleur beyond the PyPI figures above, and Sufleur's platform is closed-source so no adoption metric exists at all.

---

## Q3. What would differentiate a new entrant, and what is the realistic adoption path?

### Takeaway
For B, the only defensible differentiation is *cross-language behavioural conformance* on the ugly cases (streaming + validation + retry + cancellation + cache keys), not "we also do structured output". For D, differentiation must be the thing git cannot do (eval linkage, deployment state, non-dev editing) delivered *without* asking users to leave git — but the adoption ceiling looks low. For E, differentiation is nearly impossible on features; it would have to be language/ecosystem-specific (Go/Rust ergonomics, zero deps, ownership model) or a radically better debugging story.

### Cited Findings
- Instructor's own docs advertise retries with error feedback and debug logging as the differentiator, and it already ships them in Go (`WithMaxRetries`, `WithLogging("debug")`) — [pkg.go.dev instructor-go](https://pkg.go.dev/github.com/instructor-ai/instructor-go), [instructor-go README](https://github.com/instructor-ai/instructor-go)
- Go's idiomatic answer to timeouts already exists in the standard library (`context` deadlines), which undercuts "no timeout control" as a selling point for a new Go dependency — [pkg.go.dev/context](https://pkg.go.dev/context)
- New Go entrants are already competing on exactly the differentiators one would propose: `elelem` advertises retries that "stop the instant output starts streaming" and "keeps a ledger of what the failed attempts cost you"; `langrails` advertises "zero dependencies, only Go standard library" plus retry/`Retry-After`; `looper-agent` advertises circuit breakers and concurrent-session-safe retry counters — [elelem](https://github.com/psyb0t/elelem), [langrails](https://promptrails.github.io/langrails/), [looper-agent](https://github.com/cuatroochenta-idi/looper-agent)
- Rig's own structured-output docs show the "extractor" pattern (schema derived from type, parse→validate→decode, bounded retries) is already the standard shape in Rust — [open-ai-sdk.github.io/ai-go structured output](https://open-ai-sdk.github.io/ai-go/core/structured-output) (documents Rig's extractor IA and mirrors it), [rig repo](https://github.com/0xPlaygrounds/rig)
- Evidence that "thin + one job" is the winning aesthetic in 2026: the anti-framework framing is used as a *product name* (Mirascope), and HN commenters explicitly ask for "piecemeal tools that can be refactored or replaced" — [Mirascope](https://github.com/Mirascope/mirascope), [HN](https://news.ycombinator.com/item?id=36645575)
- Evidence that a purely-git prompt tool's value proposition is weak: the most detailed practitioner account says the value "is in the gate and the audit trail, and those do not require much code to implement" — i.e. the durable value is evals/CI, which promptfoo already owns at 4.8M npm downloads/month — [mohakdeepsingh.dev](https://mohakdeepsingh.dev/blog/how-i-actually-version-prompts-production-not-ideal-system), [npm promptfoo](https://api.npmjs.org/downloads/point/last-month/promptfoo)
- Non-dev editing is the recurring reason teams leave git, which a git-only tool cannot serve — [Towards Data Science](https://towardsdatascience.com/why-your-prompts-dont-belong-in-git/), [ilyazakharau.com](https://ilyazakharau.com/blog/ai-pm-maintaining-prompts)
- The realistic distribution channel for this class of tool is HN/Reddit launch + framework comparison posts; the observed ceiling for a solo prompt tooling launch was 3 HN points and "pretty much no users" — [HN Sufleur](https://news.ycombinator.com/item?id=49232345)

### Inferences
- B: a credible wedge is a *conformance suite* — a published matrix of "streaming + validation failure + retry + cancellation + cache-key isolation" behaviours across Go/Rust clients, with the helper as the only implementation that passes. That converts an invisible quality claim into a testable one, and it is the kind of artifact that gets cited in comparison threads. The realistic path is: one language first (Go, because the user base and issue volume are larger), a 300-line core, a test matrix in CI, then a Rust port that shares the test vectors.
- B: pricing is unrealistic; this is an Apache/MIT dependency play. Success looks like becoming the default import in a niche (e.g. "the structured-output helper for Go"), i.e. low thousands of stars, not a business.
- D: the only defensible version is "git is the source of truth, and we add the missing eval/deploy linkage as a thin layer" — e.g. a CLI that maps a git SHA to eval results and to what is deployed. But that collides with promptfoo (evals) and Langfuse (deployment state), and requires integrations to be useful. Adoption path would be a GitHub Action, not a library.
- E: differentiation has to be non-functional. Candidate wedges: zero dependencies + stdlib-only (langrails already claims it), compile-time schema generation, or best-in-class error messages/observability. Feature parity is not achievable as a wedge against LiteLLM/aisuite/`llm`.

### Gaps
- No source quantifies willingness to adopt a *new* dependency for any of these; I found no survey or pricing data. All statements about adoption path are inference from observed adoption of comparable tools.
- I could not find revenue or funding data for any of the small prompt-versioning tools, so the commercial ceiling is unknown (only download/star proxies).

---

## Q4. Falsifying facts — what would make each idea non-viable?

### Takeaway
Idea B is weakened (not killed) by fast upstream fixes, official SDKs, and native `context` timeouts; its biggest risk is market size, not competition. Idea D looks close to falsified: the exact product exists, is free, and has ~zero users, while the job is absorbed by plain files + promptfoo/Langfuse. Idea E is the most clearly falsified on competition grounds — the layer is occupied by 60k-star, 100M-download incumbents.

### Cited Findings

**Idea B — falsifiers**
- The reference implementation fixes critical retry/streaming bugs within ~24 hours (issue #1991 opened 2026-01-12, fixed and released 2026-01-13 as v1.14.3), so "Instructor is buggy" is a decaying argument — [issue #1991](https://github.com/567-labs/instructor/issues/1991), [v1.14.3](https://github.com/567-labs/instructor/releases/tag/v1.14.3)
- Instructor's backlog is small relative to its user base (29 open issues/PRs, 14k stars, 10.2M downloads/month) — [GitHub](https://github.com/567-labs/instructor), [pypistats](https://pypistats.org/packages/instructor)
- Provider-native structured outputs remove the need for schema-generation + parse helpers for the simplest cases: OpenAI ships strict `json_schema` plus `zodTextFormat`/`parse_as` in its own SDKs — [developers.openai.com](https://developers.openai.com/api/docs/guides/structured-outputs)
- "No timeout control" is not true in Go: `context` deadlines are stdlib and every serious Go client accepts a `context.Context` — [pkg.go.dev/context](https://pkg.go.dev/context); official OpenAI Go SDK is context-based — [GitHub](https://github.com/openai/openai-go)
- The Go niche is being flooded, not ignored: structllm (2026-05), looper-agent (2026-05), elelem (2026-08), langrails, ai-go all ship "structured output + retry" and all have ≤3 stars — [structllm](https://github.com/mohsenm4/structllm), [looper-agent](https://github.com/cuatroochenta-idi/looper-agent), [elelem](https://github.com/psyb0t/elelem), [langrails](https://promptrails.github.io/langrails/), [ai-go](https://github.com/open-ai-sdk/ai-go)
- Rust already has a direct Instructor analogue with real (if small) usage: rstructor, 5,436 downloads in the last 90 days, explicitly "The Rust answer to Python's Pydantic + Instructor" — [crates.io](https://crates.io/crates/rstructor)
- Rust's dominant library is a *framework*, not a thin helper (rig-core: 4.5M downloads, 9.2k stars), suggesting Rust users may prefer the bundled option — [crates.io](https://crates.io/crates/rig-core), [GitHub](https://github.com/0xPlaygrounds/rig)
- No source I found contains an explicit request for a Go/Rust retry/streaming helper; the demand is inferred from Python bugs plus general Go-for-LLM enthusiasm — see Gaps in Q1.

**Idea D — falsifiers**
- The product already exists in near-identical form and is free: `promptfile` — YAML prompt files, `render`, `diff_prompts`, `validate`, registry, CLI, CI recipe, "No servers. No SaaS. No database." — 0 GitHub stars, 1 commit, 22 PyPI downloads/month — [GitHub](https://github.com/maheshmakvana/promptfiles), [pypistats](https://pypistats.org/packages/promptfiles)
- A second git-native attempt (PromptOps) sits at 177 downloads/month — [pypistats](https://pypistats.org/packages/llmhq-promptops)
- A better-resourced npm-style prompt registry launched on HN and reported "Pretty much no users so far", with 3 points — [HN](https://news.ycombinator.com/item?id=49232345)
- The job's durable value is evals/CI, and that is owned: promptfoo has 27.4k stars and 4.83M npm downloads/month — [GitHub](https://github.com/promptfoo/promptfoo), [npm](https://api.npmjs.org/downloads/point/last-month/promptfoo)
- The runtime/deployment half is owned by platforms: Langfuse (37.6k stars) and the 2026 tool roundups list LangWatch/PromptLayer/LangSmith/Braintrust — [Langfuse](https://github.com/langfuse/langfuse), [langwatch.ai](https://langwatch.ai/blog/best-prompt-management-tools)
- Git-only is actively argued against by practitioners because it cannot express deployed state, eval outcomes, access control, or non-engineer editing — [Towards Data Science](https://towardsdatascience.com/why-your-prompts-dont-belong-in-git/), [Medium](https://sajithatharaka.medium.com/why-git-is-the-wrong-tool-for-managing-your-prompts-a670653bc2f5)
- Teams report git-only prompt workflows collapsing past a handful of templates ("Version conflicts everywhere") — [community.latenode.com](https://community.latenode.com/t/managing-prompt-templates-in-git-repositories-with-langsmith-any-success-stories/39403)
- Switching cost is near-zero and switching *benefit* is near-zero: a `/prompts` directory + PR review already delivers diff/blame/history, which is precisely what the tools advertise — [promptfile README](https://github.com/maheshmakvana/promptfiles), [mohakdeepsingh.dev](https://mohakdeepsingh.dev/blog/how-i-actually-version-prompts-production-not-ideal-system)

**Idea E — falsifiers**
- LiteLLM already is the low-level bridge and has 63.5k stars / 99.8M downloads per month; it is free and covers 100+ providers — [GitHub](https://github.com/BerriAI/litellm), [pypistats](https://pypistats.org/packages/litellm)
- aisuite (16.4k stars), `llm` (12.8k), `ell` (5.8k), any-llm (2.3k), magentic (2.4k), Mirascope (1.5k) all occupy the same "thin bridge" position — [aisuite](https://github.com/andrewyng/aisuite), [llm](https://github.com/simonw/llm), [ell](https://github.com/MadcowD/ell), [any-llm](https://github.com/mozilla-ai/any-llm), [magentic](https://github.com/jackmpcollins/magentic), [Mirascope](https://github.com/Mirascope/mirascope)
- Even the best-positioned "anti-framework" has only 20k downloads/month vs LiteLLM's 99.8M — a ~5,000x gap, indicating that positioning alone does not move adoption — [pypistats mirascope](https://pypistats.org/packages/mirascope), [pypistats litellm](https://pypistats.org/packages/litellm)
- Official SDKs are absorbing the remaining work: OpenAI's own SDK now does schema-derived parsing (`parse_as`, `zodTextFormat`) — [developers.openai.com](https://developers.openai.com/api/docs/guides/structured-outputs)
- The "SQLAlchemy-level" framing is from January 2025 and its author's own project (magentic.dev) sits at 2.4k stars after ~2 years — [HN](https://news.ycombinator.com/item?id=42803483), [magentic](https://github.com/jackmpcollins/magentic)
- Developer sentiment pushes toward *no* library at all for simple cases: "I'd have to spend a whole lot more time learning how to use them than if I just imported the OpenAI Python library directly and started running prompts through my own thin set of custom functions" — [HN, Simon Willison](https://news.ycombinator.com/item?id=36645575)

### Inferences
- Idea B verdict: **conditional / narrow**. It survives only as a quality-and-conformance play in Go first, with a public test matrix as the marketing artifact. It does not survive as "Instructor for Go/Rust" — that product already exists (instructor-go, rstructor) and is not winning.
- Idea D verdict: **effectively falsified as a standalone library**. The only surviving variant is a CI/Action that links git SHAs to eval results and deployed versions — which is a different product with integration-heavy scope, competing with promptfoo.
- Idea E verdict: **falsified on differentiation**. Demand for the layer is proven, but it is served by free, enormous incumbents and by official SDKs; a new entrant has no feature wedge, only an ergonomics/language wedge, and the comparable "anti-framework" products show that wedge converting poorly.
- Cross-cutting: the prior-research premise "the market moved to thin single-responsibility helpers" is supported for *aesthetic preference* (HN comments, "Anti-Framework" branding) but not for *adoption*: the thin tools I measured have 1.5k–2.4k stars while the bundled frameworks have 9k–63k. Thinness wins arguments, not installs.

### Gaps
- I did not find any post-mortem from a failed prompt-versioning tool explaining *why* it failed, so the causal claim "feature set isn't the bottleneck" is inference from promptfile/PromptOps/Sufleur adoption, not from a stated reason.
- No data on Go/Rust LLM application counts (e.g. share of production LLM services in Go), which is the single most important unknown for sizing Idea B. Without it, "underserved" cannot be distinguished from "underserved because demand is small".
- I could not retrieve Reddit comment bodies for the r/golang "Features for a Go library for LLMs" thread (fetch returned empty), so I have no direct quotes from Go developers about which features they want.
