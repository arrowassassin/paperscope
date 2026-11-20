# Paper Release-Candidate Stack: Engineering Hardening

Source of truth framing: AGENTS.md layout; `docs/implementation-status.md` is the verified as-built description; `docs/paper-architecture.md` is superseded design (EmbeddingGemma, ONNX). Today: Thu Nov 19 2026. No telemetry/accounts/mandatory cloud in the product itself. CI/signing services fine. Target: macOS (Apple Silicon + Intel), Windows, Linux. Single ~10 MB Rust binary.

## Distribution

### Takeaway

For a single-binary Rust CLI shipping pre-built release binaries across macOS/Windows/Linux, the pragmatic 2026 stack is **cargo-dist** as the release automation/orchestrator (generated release.yml with plan/build/host/publish/announce, shell+PowerShell installers, Homebrew tap, MSI, checksums, SLSA L2 attestations, CycloneDX SBOM) **combined with pinned dtolnay/rust-toolchain + Swatinem/rust-cache**, and native builds per platform rather than cross-compilation. **release-plz** is the complementary crate-version/changelog/crates.io publisher — not a binary releaser. A hand-rolled workflow modeled on casey/just is the lighter alternative if you want to own every YAML line.

### Cited Findings

- release-plz does not build/release binaries at all; it publishes crates and creates git tags/changelog PRs. Binary release must be a separate CI job after release-plz creates the tag. [release-plz docs: Releasing binaries](https://release-plz.dev/docs/extra/releasing-binaries)
- cargo-dist (renamed "dist" at v0.24.0, Oct 2024; latest v0.32.0 May 22 2026) generates release.yml with 8 jobs: plan, build-local-artifacts (per-platform matrix), build-global-artifacts, host, publish-homebrew-formula, publish-npm, custom-publish, announce. Supports shell/PowerShell/Homebrew/npm/MSI installers and macOS (x86_64+aarch64 native), Linux aarch64 via native GitHub ARM64 runners (free since Jan 2025), Linux musl, Windows. [cargo-dist GitHub README](https://github.com/axodotdev/cargo-dist/)
- Of 14 major Rust CLI tools surveyed, 12 use fully custom GitHub Actions release workflows; only Astral uv/ruff use cargo-dist (heavily customized, maturin-based). Recommended template for small-to-medium pure-Rust CLI: casey/just (~120 lines, all-musl Linux, Windows, direct linker flags). [Rust CLI Binary Distribution Practices research](https://github.com/jlevy/rust-porting-playbook/blob/main/docs/project/research/research-rust-cli-binary-distribution.md)
- cargo-dist disadvantages: GitHub Actions only; version coupling (cargo-dist-version must match generated YAML); generated YAML not meant to be hand-edited; pre-1.0 breaking changes; no .deb/.rpm/.pkg; no aarch64-pc-windows-msvc; limited cross-compilation (issue #74 open since Feb 2023). [same research source]
- cargo-dist cargo-dist v0.32.0 adds cargo-zigbuild plus cargo-auditable support, updated generated action pins, smaller npm installer surface. v0.30.3 fixed macOS codesign. v0.26.0 added CycloneDX SBOM. [same research source]
- macOS code signing + notarization: `codesign --force --options runtime --timestamp`, zip binary/app, `xcrun notarytool submit ... --wait`, `xcrun stapler staple`. Bare binaries can only be submitted (online Gatekeeper check on first launch); ship stapled .pkg/.app/.dmg for offline validation. [Albatross release.yml](https://github.com/morganlinton/Albatross/blob/main/.github/workflows/release.yml); [cua-driver CD workflow](https://github.com/trycua/cua/blob/main/.github/workflows/cd-rust-cua-driver.yml)
- `cargo-codesign` orchestrates sign → DMG → notarize → staple in one command; reads credentials from env vars configured in sign.toml; `cargo codesign ci` generates ready-made GitHub Actions workflow. Recommended for macOS. [cargo-codesign docs](https://sassman.github.io/cargo-codesign-rs/ci/github-actions.html); [d34dl0ck.me post](https://d34dl0ck.me/cargo-codesign/index.html)
- Windows Authenticode: Microsoft Learn comparison (published 2026-11-10) — MSIX Store re-signs free; MSI/EXE outside Store needs publisher Authenticode. Azure Artifact Signing (formerly Trusted Signing) ~$9.99/month, recommended for non-Store, no hardware token, integrates CI. OV cert $150–300/yr; EV no longer instant SmartScreen bypass since 2024. Self-signed unsuitable for public SmartScreen. SignPath Foundation offers free OV-level signing for qualifying open-source projects. [Microsoft Learn code signing options](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/code-signing-options); [SignPath KB](https://signpath.io/knowledge-base/windows-platform)
- Authenticode best practices: SHA-256 digest (`/fd SHA256`), RFC 3161 timestamp (`/tr` + `/td SHA256`), always time-stamp so signatures outlive certificate expiry; dual-sign only for pre-Windows 7 SP1. [Microsoft Learn timestamping](https://learn.microsoft.com/en-us/windows/win32/seccrypto/time-stamping-authenticode-signatures)
- SignPath supports GitHub Actions via reusable workflows; artifact-configuration XML with `<authenticode-sign>` for PE files; crypto providers include Cryptoki PKCS#11 and KSP CNG. SignPath Foundation free for qualifying OSS. [SignPath docs](https://docs.signpath.io/)
- Homebrew tap: cargo-dist publishes formula to configurable tap repo; cargo-binstall auto-discovers GitHub Release artifacts. Scoop/winget: not covered by cargo-dist; require separate manifests (Scoop bucket JSON manifest, winget manifest YAML) typically published via PR to ScoopInstaller/scoop or Microsoft/winget-pkgs repo. [cargo-dist installers](https://axodotdev.github.io/cargo-dist/book/installers/index.html)

### Inferences

- Blocking for 0.1.0/1.0.0: signed macOS artifacts (codesign + notarize + staple) and Windows Authenticode artifacts, plus checksums/SHA256SUMS and a GitHub Release. Without these, Gatekeeper/SmartScreen block users — that's a distribution blocker, not a nicety.
- Linux deb/rpm/AUR/nix and Scoop/winget are wait-for-later; a tar.gz + shell/PowerShell installer + Homebrew tap covers 90% of users. Native ARM64 Linux runners remove cross-compilation pain for Apple Silicon Mac builds (aarch64-apple-darwin builds on macOS runners directly).
- cargo-dist's version coupling and opaque generated YAML are the main risks; mitigate by pinning `cargo-dist-version` and running `dist plan` in CI on PRs to catch config drift.

### Gaps

- Confirmed current pricing/eligibility for SignPath Foundation open-source program (application process not detailed in sources).
- Scoop/winget manifest template details not found; require separate research if adopted.

---

## Supply Chain & Reproducible Builds

### Takeaway

Run **cargo-audit** (RustSec advisories) + **cargo-deny** (licenses/bans/sources/duplicates) as CI gates, **cargo-vet** selectively for high-sensitivity crates, generate a **CycloneDX SBOM** via **cargo-cyclonedx**, embed dependency metadata via **cargo-auditable**, and produce **SLSA L2 provenance** via **actions/attest-build-provenance** (the slsa-github-generator is no longer maintained; GitHub artifact attestations are the replacement). Pin Rust toolchain via `rust-toolchain.toml`, commit Cargo.lock, build `--locked --release` from tagged commits only. Keyless cosign/OIDC signing optional but recommended.

### Cited Findings

- cargo-audit checks Cargo.lock against RustSec advisory database (~700 advisories); `--deny warnings` also fails on unmaintained/yanked; `cargo audit bin` scans compiled binaries built with cargo-auditable. Pin installation in CI. [Safeguard: Rust Crate Security](http://safeguard.sh/resources/blog/rust-crate-security-cargo-audit-cargo-vet-and-beyond); [RustSec](https://rustsec.org/)
- cargo-deny checks advisories, licenses, bans, sources; `cargo deny init` generates deny.toml; four sections: advisories (deny vulns + expiry-dated exceptions), licenses (allowed/denied list), bans (forbidden crates + duplicate versions), sources (allow registries/git). [Microsoft RustTraining ch06](https://github.com/microsoft/RustTraining/blob/main/engineering-book/src/ch06-dependency-management-and-supply-chain-s.md); [cargo-deny GitHub](https://github.com/EmbarkStudios/cargo-deny)
- cargo-vet: human-audit ledger; `cargo vet init` exempts existing deps; fail on new/unvetted crates; certify or diff-audit or import (Mozilla/Google/bytecode-alliance public audit sets). Most valuable for strict supply-chain orgs; cargo-deny sufficient for most. [Safeguard blog](http://safeguard.sh/resources/blog/rust-crate-security-cargo-audit-cargo-vet-and-beyond); [cargo-vet how it works](https://mozilla.github.io/cargo-vet/how-it-works.html)
- slsa-github-generator is **no longer actively maintained** (last release v2.1.0 Feb 2025); GitHub recommends **artifact attestations** via `actions/attest-build-provenance`, verified with `gh attestation verify`. [slsa-github-generator README](https://github.com/slsa-framework/slsa-github-generator); [SLSA get started](https://slsa.dev/how-to/get-started)
- SLSA Build L3 via GitHub Actions: jump directly to L3 (no need for L1/L2). Requires hosted isolated platform, signed provenance, build runs cannot influence each other. [SLSA get started](https://slsa.dev/how-to/get-started)
- SBOM: cargo-cyclonedx generates CycloneDX JSON/XML (spec 1.6/1.7); one file per shipped binary more useful than one per repo. cargo-auditable embeds <4 kB compressed JSON dependency list in `.dep-v0` linker section (Linux/Windows/macOS officially supported); readable by cargo-audit bin, Trivy, Grype, OSV-Scanner, Syft. [Safeguard SBOM guide](https://safeguard.sh/resources/blog/how-to-generate-an-sbom-for-a-rust-project); [cargo-auditable crates.io](https://crates.io/crates/cargo-auditable)
- Reproducible build claim: pin rust-toolchain.toml, commit Cargo.lock + `--locked`, pin GitHub Actions to full SHAs, `SOURCE_DATE_EPOCH` for SBOM timestamps, build from clean runners. Bit-identical between runs given same git sha + toolchain + lockfile. [Nexo-rs reproducibility](https://lordmacu.github.io/nexo-rs/docs/getting-started/reproducibility.html); [Kunal Ganglani 2026 pipeline](https://www.kunalganglani.com/blog/rust-reproducible-builds-sbom)
- Sigstore/cosign: cosign-installer GitHub Action available; `cosign sign-blob --bundle` for artifact signing; keyless OIDC chain. [aptu-coder build-and-attest.yml](https://github.com/clouatre-labs/aptu-coder/blob/main/.github/workflows/build-and-attest.yml)
- `actions/attest-build-provenance` v4 used with `subject-path`, `id-token: write`, `attestations: write` permissions; generates in-toto provenance with builder.id, source uri/digest/ref, buildInvocationID. [Kshana publish.yml](https://github.com/ashfordeOU/kshana/blob/0e0a74ff75165d72a6b66338fe58635145bf34c9/.github/workflows/publish.yml); [Benchora docs/slsa.md](https://github.com/KooshaPari/Benchora/blob/main/docs/slsa.md)

### Inferences

- Blocking for RC: cargo-audit + cargo-deny CI gates, CycloneDX SBOM attached to release, pinned toolchain/lockfile, SLSA L2 provenance via attest-build-provenance. cargo-vet can wait unless legal/compliance demands human audit evidence.
- cargo-auditable embedding is high-value, low-cost: lets security teams audit shipped binaries months later without source; under 4 kB overhead; doesn't break reproducible builds.

### Gaps

- Whether cargo-deny license policy can auto-generate the MIT/Apache-2.0 NOTICE text that the binary must ship (needs verification).
- Specific RustSec advisories affecting Paper's dependency tree not yet enumerated (requires running cargo audit against actual Cargo.lock).

## Licensing

### Takeaway

Paper's core deps are MIT/Apache-2.0 compatible, which is clean. The main licensing risk is the **model weights**: BAAI BGE and intfloat E5 are MIT (commercial use permitted) — safe. But the plan mentions models "from HF with pinned commit + per-file SHA256" without naming them; if any model uses CC BY-NC, Gemma Terms of Use, or a non-commercial license, that blocks public distribution. A NOTICE/LICENSE file aggregating all crate licenses plus model attribution must ship inside the ~10 MB binary (or alongside it), generated by cargo-deny's license output.

### Cited Findings

- BAAI BGE (bge-base-en, bge-large-en, bge-large-en-v1.5): MIT License; "released models can be used for commercial purposes free of charge." FlagEmbedding repo MIT. [HuggingFace BAAI/bge-large-en-v1.5](https://huggingface.co/BAAI/bge-large-en-v1.5)
- intfloat/multilingual-e5-large and multilingual-e5-large-instruct: MIT license, commercial/non-commercial use permitted. [WZ-IT embedding models comparison 2026](https://wz-it.com/en/blog/best-embedding-models-german/); [NVIDIA e5-large-unsupervised model card](https://developer.nvidia.com/downloads/assets/ace/model_card/e5-large-unsupervised.pdf)
- jina-embeddings-v3: CC BY-NC 4.0 (non-commercial only; commercial inquiries to provider) — NOT safe for a general public binary without separate license. [PEC Collective embedding models reference](https://pecollective.com/tools/text-embedding-models-compared/)
- EmbeddingGemma-300m: Gemma Terms of Use (permitted subject to terms + prohibited use policy; HF download only after accepting terms). [same WZ-IT source]
- cargo-deny license check: `cargo deny list` outputs license counts per crate (MIT, Apache-2.0, BSD-3-Clause, MPL-2.0, Unicode-DFS, etc.); deny.toml can set allowed/denied license lists. [Microsoft RustTraining ch06](https://github.com/microsoft/RustTraining/blob/main/engineering-book/src/ch06-dependency-management-and-supply-chain-s.md)
- CycloneDX 1.6/1.7 supports license acknowledgement and expression details; SPDX better for licensing/compliance workflows. CycloneDX better for dependency inventory + vuln tooling. [Kunal Ganglani 2026](https://www.kunalganglani.com/blog/rust-reproducible-builds-sbom)

### Inferences

- Blocking for RC: audit the transitive dependency tree with cargo-deny licenses; confirm no GPL/AGPL/LGPL crates (especially any PDF/Office parser). Ship a NOTICE file listing MIT + Apache-2.0 (and any others) with copyright attribution.
- Model license verification is a hard gate before 1.0.0: pin exact model IDs + commits + SHA256 (already planned per AGENTS.md invariant #4), but also record each model's license in config/docs and reject non-commercial licenses.
- unpdf (pure Rust PDF, MIT) is a candidate parser alternative if the planned parser has copyleft; flpdf and lopdf/hayro/krilla are also MIT. [unpdf GitHub](https://github.com/iyulab/unpdf); [flpdf threat model](https://github.com/fulgur-rs/flpdf/blob/main/docs/threat-model.md); [pylopdf README](https://github.com/yhay81/pylopdf)

### Gaps

- Paper's actual Cargo.lock not examined; copyleft scan of transitive deps pending.
- Exact model(s) Paper will ship not named in context; license of each pending confirmation.

## Security / Threat Model

### Takeaway

Paper indexes private household documents via AI agents over MCP — the trust boundary is **untrusted file bytes ingested into a local index that an LLM later reads**, creating an indirect prompt-injection channel. Defenses: (1) sandbox/isolate parser execution (child process with address-space + wall-clock limits, no network, no write outside scratch); (2) memory + size caps on every parser input; (3) cargo-fuzz on all parsers; (4) SQLite extension loading restricted to statically linked, auto-registered extensions only — never `load_extension()` from user paths; (5) path canonicalization + root allow-list to block symlink/traversal during indexing; (6) secrets never persisted (.env forbidden, keychain/SSH keys excluded from indexing scope); (7) DB + model cache files mode 0600/0700.

### Cited Findings

- RAG ingestion is the dangerous stage: it accepts unknown-provenance bytes and runs format parsers (PDF, DOCX, XLSX, PPTX, HTML, EML, images) over attacker-chosen input — exactly the class with historical memory-safety CVEs. Retrieval reads vetted chunks + query string, no parsing. Sandbox ingestion, not retrieval. [PandaStack: Run RAG Ingestion in an Ephemeral Sandbox](https://www.pandastack.ai/blog/how-to-run-a-rag-pipeline-in-an-ephemeral-sandbox)
- Indirect prompt injection: documents with hidden text/comments/metadata written as instructions must be dropped or labeled as data; no tool call or privileged action follows retrieved content. [Untrusted File and Media Processing Reference](https://martian1337.gitbook.io/notes/appsec/application-and-product-security/application-security-and-secure-sdlc/untrusted-file-and-media-processing-reference)
- flpdf threat model (pure-Rust PDF, no unsafe): no UB, no panic/abort from attacker input, no non-termination (nesting depth limit per qpdf CVE-2018-9918), no silent signature-evidence removal. Resource-exhaustion (compression bomb) out of scope — must be capped separately. [flpdf threat model](https://github.com/fulgur-rs/flpdf/blob/main/docs/threat-model.md)
- unpdf: pure Rust PDF parser, no native engine, MIT; custom zero-dependency parser; parallel page processing; memory-efficient; size/complexity limits raise LimitError instead of exhausting memory. [unpdf GitHub](https://github.com/iyulab/unpdf)
- agent_first_pdf: PDF engines are native code; malformed/hostile PDFs can abort the process or never return. Offers in-process (trusted only) vs isolated child-process execution; hard wall-clock + address-space limits (2 GiB Linux default); child contains native crashes but is not a filesystem/network sandbox. [agent_first_pdf docs.rs](https://docs.rs/agent-first-pdf/latest/agent_first_pdf/)
- notify known problems: network filesystems (NFS) may not emit events; PollWatcher fallback for WSL/Windows paths; /proc, /sys. iCloud/OneDrive/Dropbox placeholders not explicitly covered but placeholders are reparse points — events may fire on placeholder metadata changes without real content. [notify docs.rs known problems](https://docs.rs/notify/latest/notify)
- SQLite run-time loadable extensions are off by default; must explicitly enable via `sqlite3_db_config(SQLITE_DBCONFIG_ENABLE_LOAD_EXTENSION)`. Load from file path is dangerous — avoid. `sqlite3_auto_extension` for statically linked extensions runs per connection; entry point must not open another connection (recursion) or manipulate the list. [SQLite loadable extensions](https://www.sqlite.org/loadext.html); [rusqlite auto_extension.rs source](https://docs.rs/rusqlite/latest/src/rusqlite/auto_extension.rs.html); [rusqlite LoadExtensionGuard](https://docs.rs/rusqlite/latest/rusqlite/struct.LoadExtensionGuard.html) — RAII guard, "You must not run untrusted queries while extension loading is enabled."
- rusqlite `register_auto_extension`: callback runs on several threads, must not open another connection or re-register; panics caught → failed open. [diesel register_auto_extension docs](https://docs.diesel.rs/main/diesel/sqlite/fn.register_auto_extension.html)
- AppSec brief (Oct 2026): archive extraction/decompression are privileged parser boundaries; require compressed/decompressed byte, memory and time limits on every untrusted file worker; zip 9.0.1+ for symlink path traversal fixes. [SKMukhiya AppSec Brief](https://www.skmukhiya.com.np/blog/appsec-2026-10-19-untrusted-input-boundaries)

### Inferences

- Blocking for RC: parser isolation (child process or in-process with forbid(unsafe_code) + limits), memory/size/wall-clock caps, cargo-fuzz on parsers, SQLite extension lock-down (no dynamic load_extension from user-supplied paths), path traversal + symlink guards, file permissions 0600 DB / 0700 cache.
- Wait for later: full OS sandbox (sandbox-exec on macOS, job objects on Windows, seccomp/bubblewrap on Linux) — can start with process isolation + rlimits and graduate.
- iCloud/OneDrive/Dropbox placeholders: treat placeholder files as unindexable until fully downloaded (check file size/content availability); PollWatcher fallback if native backend misses events on network mounts.

### Gaps

- Per-OS sandbox implementation details (sandbox-exec entitlements, Windows job object, Linux seccomp) not researched.
- Specific parser crate(s) Paper will use for PDF/Office/image not named; fuzz target list pending.
- Secrets redaction rules (what to skip: .env, id_rsa, etc.) not yet defined in policy.

## Robustness

### Takeaway

Paper must survive: corrupt SQLite DB, interrupted model downloads, partial index writes, disk-full, concurrent index+search under WAL, and safe upgrades. Defenses: WAL mode with busy_timeout for concurrent readers/writers; schema version table + incremental migrations with rollback; atomic index writes (write temp file + fsync + rename); model download to temp + SHA256 verify + atomic rename; disk-full handling with graceful errors and cleanup; retry/backoff on model download; startup DB integrity check (`PRAGMA integrity_check`) with recovery path.

### Cited Findings

- Paper AGENTS.md invariants: chunking keeps `path`, `line_start`, `line_end` accurate; citations must resolve; search never downloads a model (missing weights → keyword-only + hint, or exit 3 with `--semantic`). These are correctness requirements that robustness must preserve. [AGENTS.md project context]
- sqlite-vec 0.1.9 statically linked via `sqlite3_auto_extension`; schema: chunks + FTS5 external content + `chunks_vec` `float[384]`; incremental line-accurate indexer. [AGENTS.md layout]
- General SQLite robustness (pattern, not Paper-specific): WAL mode allows concurrent readers + one writer; busy_timeout handles contention; `PRAGMA integrity_check` for corruption detection; journal_mode=WAL + synchronous=NORMAL for durability/performance balance. [SQLite documentation patterns referenced across sources]
- Atomic file writes: write to temp file in same directory, fsync, rename over target — prevents partial/index corruption on crash. [General durability pattern; no specific source needed]
- Model download: hf-hub downloads to cache; interrupted downloads leave partial files. Need temp-download + SHA256 verify (per AGENTS.md invariant #4, per-file SHA256 pinned) + atomic rename into cache. [AGENTS.md models/specs pinned repos + SHA256]

### Inferences

- Blocking for RC: WAL + busy_timeout, schema migrations with version table, atomic index/model writes, disk-full graceful handling, startup integrity check, retry on model download.
- Wait for later: multi-replica DB, backup/restore CLI, online compaction.

### Gaps

- Paper's actual migration code not yet reviewed; migration strategy (embed in binary vs separate SQL) pending.
- Disk-full threshold and cleanup behavior not yet specified.

## Performance / Resource Budgets

### Takeaway

Set and enforce CI budgets: indexing throughput (docs/sec), search latency p50/p95 (target <200ms p95 for semantic + keyword hybrid), memory ceiling during embedding/reranking on CPU (target <1 GB RSS), binary size (~10 MB target, enforce max), cold-start model load time. Measure with criterion benchmarks + hyperfine in CI; memory profiling with valgrind/massif or dhats; enforce via CI fail-on-regression thresholds. Release builds only (`--release`; debug Candle ~15x slower per AGENTS.md).

### Cited Findings

- AGENTS.md: performance must be measured with `--release`; debug Candle is ~15x slower. Model integration tests download real weights and are `#[ignore]`d (134 MB / 487 MB / 1.13 GB downloads). [AGENTS.md build & test]
- Search pipeline: tier0 FTS5, tier1 sqlite-vec KNN, pipeline RRF + confidence + hydration. candle-core/nn/transformers 0.8 for embeddings; reranker = Candle XLM-RoBERTa sequence classifier. Deterministic offline `HashEmbedder` in tests/common so retrieval logic never needs real weights. [AGENTS.md layout]
- No specific p50/p95 or memory numbers found for Paper; these are project-defined targets to set.

### Inferences

- Blocking for RC: define the budget numbers (indexing throughput, p95 latency, memory ceiling, binary size max) and add CI enforcement with criterion + hyperfine. Without numbers, "performance" is unverifiable.
- criterion for micro-benchmarks (embedding batch, rerank, FTS5 query), hyperfine for end-to-end CLI search latency. Memory profiling optional for RC but recommended.

### Gaps

- Actual budget target values not yet set by project.
- criterion/hyperfine not yet integrated into Paper's CI (per AGENTS.md, tests are offline with HashEmbedder; performance tests not mentioned).

## File Watching

### Takeaway

Use **notify** (v8) with **notify-debouncer-full** (2s window default) or **mini** for shorter debounces. Watch recursively; debounce to avoid re-embedding storms from editor save bursts (3-5 events per save). Handle network mounts (NFS may not emit events → PollWatcher fallback), and treat iCloud/OneDrive/Dropbox placeholders as unindexable until content is available (placeholder = metadata-only reparse point; skip zero-size/placeholder files). Filter to indexed extensions only; skip temp files (`.~*`, `~$*`, editor swap files).

### Cited Findings

- notify v8.1.0: backends = FSEvents (macOS default), kqueue (BSD), inotify (Linux), ReadDirectoryChangesW (Windows), PollWatcher fallback. `recommended_watcher` picks best backend. `RecursiveMode::Recursive` for subtree. [notify docs.rs](https://docs.rs/notify/latest/notify)
- notify known problems: network filesystems (NFS) may not emit events; PollWatcher workaround for WSL/Windows paths, /proc, /sys. [notify docs.rs known problems]
- notify-debouncer-full v0.8.0-rc.2: coalesces events within a time window, resolves renames, drops duplicates (no duplicate create, no Modify after Create), FileIdMap cache. new_debouncer(window, tick, handler). [notify-debouncer-full docs.rs]
- gpui-starter blog (Jun 2026): single save = 3-5 events (temp create, modify, remove old, rename); debounce 200ms for config reload, 50-100ms for live browser, 500-1000ms for build triggers, 2000-5000ms for backup/sync. [gpui-starter file watching blog](https://gpui-starter.freeoxide.com/blog/integrating-notify-rust-file-watching)
- oneuptime guide (Jan 2026): debounce 500ms default; build triggers 500-1000ms; backup/sync 2000-5000ms. [oneuptime file watcher debouncing](https://oneuptime.com/blog/post/2026-01-25-file-watcher-debouncing-rust/view)

### Inferences

- Blocking for RC: debouncer (2s) on indexing path, extension filter, temp-file skip, placeholder detection (skip files that can't be fully read), PollWatcher fallback configured for network mounts.
- Wait for later: iCloud/OneDrive "download on demand" opt-in, intelligent re-embedding only on content change (hash compare before re-embed).

### Gaps

- Placeholder detection implementation (how to detect iCloud/OneDrive placeholder per OS) not researched.
- Re-embedding storm safeguards beyond debouncing (rate limit, backoff) not yet specified.

## Testing for Release

### Takeaway

CI matrix: macOS (aarch64 + x86_64), Windows (x86_64), Linux (x86_64 + aarch64 musl). Integration tests use the deterministic offline `HashEmbedder` (tests/common) so retrieval logic never needs real weights; model-weight integration tests are `#[ignore]`d and run only in a nightly/cache job with vendored/cached weights. Fuzz targets (libFuzzer/cargo-fuzz) on all parsers (PDF, Office, image) for malformed-input robustness. Coverage gate: cargo-tarpaulin or grcov, fail CI below threshold (e.g. 80% line coverage on core/index/search). Offline-first: `PAPER_OFFLINE=1` must work — no network in CI test runs.

### Cited Findings

- AGENTS.md: `cargo test` = 55 tests, offline, no model downloads. `cargo test --test models -- --ignored` = 134 MB / 487 MB / 1.13 GB model downloads. tests/common/mod.rs has deterministic offline HashEmbedder. Model integration tests `#[ignore]`d. [AGENTS.md build & test]
- cargo-fuzz: standard Rust fuzzing via rust-fuzz/cargo-fuzz; fuzz targets on parsers for malformed input. [General Rust ecosystem knowledge; no specific Paper source]
- cargo-tarpaulin / grcov: coverage tooling for Rust; CI gates on percentage. [General Rust ecosystem knowledge]
- PAPER_OFFLINE=1 forbids network access (AGENTS.md core/paths.rs). [AGENTS.md layout]

### Inferences

- Blocking for RC: CI matrix builds + tests on all 3 platforms; offline test suite passes; fuzz targets for parsers; coverage gate. Model-weight tests gated behind `--ignored` + weight cache (do not run every CI run).
- Wait for later: property-based testing (proptest), differential testing against reference parsers, chaos testing for disk-full/concurrent corruption.

### Gaps

- Fuzz targets not yet written; parser crate selection pending.
- Coverage threshold value not yet set.
- Weight-caching strategy for nightly model tests (vendored vs cached download) not specified.

## Release Blocker vs. Wait-for-Later Summary

### Blocking for public 0.1.0 / 1.0.0

- **Distribution:** signed macOS artifacts (codesign + notarize + staple via cargo-codesign), Windows Authenticode artifacts (SignPath Foundation free OSS or Azure Artifact Signing ~$9.99/mo), SHA256SUMS, GitHub Release. Without these, Gatekeeper/SmartScreen block users.
- **Supply chain:** cargo-audit + cargo-deny CI gates, CycloneDX SBOM attached to release, pinned rust-toolchain.toml + committed Cargo.lock + `--locked` builds, SLSA L2 provenance via `actions/attest-build-provenance`.
- **Licensing:** cargo-deny copyleft scan of transitive tree (no GPL/AGPL/LGPL); NOTICE file with MIT/Apache-2.0 attribution; model license confirmed MIT/Apache-2.0 (BAAI BGE, intfloat E5 qualify); reject any CC BY-NC / Gemma-ToU model.
- **Security:** parser isolation + memory/size/wall-clock caps, cargo-fuzz on parsers, SQLite extension lock-down (no dynamic `load_extension` from user paths), path traversal + symlink guards, DB 0600 / cache 0700 permissions, secrets never persisted.
- **Robustness:** WAL + busy_timeout, schema migrations with version table, atomic index/model writes (temp+fsync+rename), disk-full graceful handling, startup `PRAGMA integrity_check`, retry on model download.
- **Performance:** defined budget numbers (indexing throughput, p95 latency, memory ceiling, binary size max) with CI enforcement via criterion + hyperfine.
- **Testing:** CI matrix (macOS ARM+Intel, Windows, Linux x86_64+ARM musl), offline `HashEmbedder` test suite, fuzz targets on parsers, coverage gate.

### Can wait (post-1.0)

- cargo-vet human audit ledger (unless legal/compliance demands it).
- Linux .deb/.rpm, AUR, nix, Scoop, winget packages (tar.gz + shell/PowerShell installer + Homebrew tap covers most users).
- Full OS sandbox (sandbox-exec / job objects / seccomp) — start with process isolation + rlimits.
- iCloud/OneDrive "download on demand" opt-in, hash-based re-embed skip.
- Multi-replica DB, backup/restore CLI, online compaction.
- Memory profiling with valgrind/massif (optional for RC).
- Property-based testing, differential parser testing, chaos testing.








---
