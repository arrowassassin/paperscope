# Paper release candidate stack: retrieval & answer quality

Objective: research retrieval/ranking/answer-quality improvements to make Paper release-candidate worthy, plus how to prove quality with an evaluation harness.

Constraints: Nov 2026. Everything local on CPU, no accounts, no mandatory large downloads. Prioritize by quality gain per complexity unit; flag ideas NOT worth it.

Current Paper state (source read):
- Chunking: line-based fixed-size ~400 whitespace words + 40 overlap, per-file page=0, line_start/line_end preserved for citation. No structure awareness (headings/pages/PDF table structure ignored).
- Hybrid: FTS5 keyword (OR + bm25 + stopwords) -> early exit if >=10 keyword hits -> sqlite-vec cosine KNN (limit 20, floor 0.25) -> RRF(k=60) -> optional cross-encoder rerank top 20.
- Models: bge-small-en-v1.5 default 134 MB, multilingual-e5-small opt-in 487 MB, bge-reranker-base opt-in 1.13 GB.
- Answers: extractive (top chunk snippet + up to 3 citations), not_found flag. Confidence per hit (High/Medium/Low) from keyword/semantic corroboration.

## Chunking: fixed-size vs structure-aware vs semantic vs late chunking

### Takeaway
Structure-aware chunking (headings / page / paragraph boundaries) is the highest-value chunking change for household documents (warranties, receipts, tax PDFs, manuals) — it prevents mid-sentence/mid-table cuts that break citation line accuracy and retrieval relevance. Pure fixed-size 400-word chunks keep working for plain text but should be bounded by structural boundaries. Semantic chunking and late chunking are NOT worth the complexity for this stack.

### Cited Findings
- Structure-based splitting follows the document's own skeleton (markdown headings, HTML sections, PDF pages), grouping related content so related statements stay in one chunk rather than being split mid-thought. — [What is Chunking in RAG?](https://theaiengineer.substack.com/p/what-is-chunking)
- Smaller chunks capture fine-grained details; larger chunks suit broader themes; chunk choice must match document structure and latency budget. — [Splunk: Advanced Chunking Techniques](https://www.splunk.com/en_us/blog/artificial-intelligence/chunking-techniques.html)
- Late chunking embeds a whole document with a long-context model, then pools token vectors per chunk so each chunk embedding keeps document context — requires long-context encoders and is not yet a drop-in for sqlite-vec single-vector schema. — [Late Chunking: Contextual Chunk Embeddings](https://arxiv.org/html/2409.04701v1)
- Parent-child / small-to-big retrieval: retrieve on small focused pieces, then expand to the bigger structural parent for context — one of the highest-leverage RAG techniques. — [Advanced RAG 01: Small-to-Big Retrieval](https://medium.com/data-science/advanced-rag-01-small-to-big-retrieval-172181b396d4); [Dify parent-child retrieval](https://dify.ai/blog/introducing-parent-child-retrieval-for-enhanced-knowledge)

### Inferences
- For household docs, a heading-aware chunker (e.g., split on H1/H2/# headings or PDF page boundaries, then cap at ~400 words with 40-word overlap) preserves citations better than raw line windows: a warranty clause won't be sliced across the line boundary that determines `line_start`/`line_end`.
- The current `page: 0` hardcode means PDF page citations are already lost; structure-aware chunking should populate `page` from PDF text extraction if a PDF extractor is added — otherwise page stays 0 and citations say `path:L-L`.
- Parent-child is valuable for answers that need a whole clause/warranty section, but Paper's answers are extractive (no generative model consuming expanded context), so parent expansion only matters if the answer renderer quotes more than one chunk. Defer until answers quote multiple chunks.

### Gaps
- No measured data on how much structure-aware chunking changes nDCG/recall for a household-doc corpus; needs the eval harness (see Evaluation section) to quantify before promoting from config default.

## Hybrid fusion: RRF(k=60) vs weighted score fusion; early-exit threshold

### Takeaway
Keep RRF(k=60) — it is rank-based and avoids the score-incompatibility problem of naively averaging bm25 (unbounded negative) with cosine (0..1). The risky part is the early exit "skip semantic if >=10 keyword hits": for household docs this is often harmful because a short receipt/invoice can produce 10+ keyword hits on stopword-ish OR terms while the truly relevant document (different wording, no shared keywords) gets zero semantic consideration. Lower the threshold or make exit contingent on confidence, not raw hit count.

### Cited Findings
- RRF operates on ranks not scores, which solves the score-incompatibility problem that makes naïve weighted averaging fail when combining BM25 with dense vectors. — [Hybrid Search: BM25, Vector & Reranking Reference 2026](https://digitalapplied.com/blog/hybrid-search-bm25-vector-reranking-reference-2026)
- RRF lets it combine a cosine similarity of 0.7 with a BM25 score of 12.4 without comparing the two directly; results present in both lists accumulate two contributions. — [Qdrant Hybrid Search](https://qdrant.tech/documentation/search-tuning/hybrid-search); [Paper pipeline.rs rrf_fuse](src/core/search/pipeline.rs)
- Weighted fusion combines normalized scores as αL(c)+(1−α)S(c) (OpenSearch, Elasticsearch) but is sensitive to score normalization because BM25 and cosine live on different scales. — [arxiv 2608.00452v1](https://arxiv.org/html/2608.00452v1)
- BEIR hybrid: XERJ vs SQLite FTS5 + sqlite-vec stack gave nDCG@10 differing by at most 0.0091, never significantly — keyword+dense on a single SQLite stack is a proven, competitive combo. — [xerj benchmarks](https://raw.githubusercontent.com/xerj-org/xerj/main/benchmarks/query-1-2026-11/README.md)

### Inferences
- Paper already normalizes bm25 to 0..1 within the keyword result set (divide by best), which partially addresses normalization but NOT cross-list incompatibility (semantic cosine floor 0.25 vs normalized bm25 0..1 have different meaning). RRF remains the safer default; weighted fusion only worth trying if RRF underperforms on eval, and then with explicit per-list normalization + tuned α.
- Early exit on `kw_hits.len() >= 10` conflates "many keyword matches" with "sufficient evidence". With OR+bmm25+stopwords, a 3-word query like "fridge warranty expiry" ORs to fridge OR warranty OR expiry and can return 10+ chunks from one long manual, suppressing semantic search that might find the actual warranty card in a different file. Recommend: exit only when top keyword hit has High confidence (full_term_coverage) OR semantic already ran; never skip semantic purely on keyword count.

### Gaps
- No data yet on the household-doc corpus for what threshold avoids harmful early exits; eval harness should sweep keyword_sufficiency_threshold in {0 (always semantic), 5, 10, 20}.

## ANN vs exact KNN: corpus size threshold and quantization

### Takeaway
Keep exact brute-force sqlite-vec KNN for now. Exact scan is O(N) but for household corpora (hundreds to low-thousands of 384-dim chunks) it is single-digit to tens of milliseconds — well within interactive latency. Only consider ANN/quantization if measurement shows >100 ms p95 at the target corpus size, and only if it stays a single-file SQLite extension. Quantization (int8/bit) trades recall for speed; not worth it until exact is proven too slow.

### Cited Findings
- sqlite-vec exhaustive/brute-force benchmarks: 1,000 chunks ≈ 0.11–0.23 ms; 5,000 ≈ 0.33–0.82 ms; 10,000 ≈ 0.82–? ms; recall@10 = 1.00 for brute force at all sizes. — [SimpleVecDB benchmarks](https://github.com/CoderDayton/SimpleVecDB/blob/main/docs/benchmarks.md)
- 2ndbrain vector-search ADR: 1,000 chunks ≈ 1.5–4 ms; 10,000 ≈ 9.4–34 ms; 50,000 ≈ 49–178 ms; 100,000 ≈ 99–358 ms for exact scan — still interactive at 100k. — [2ndbrain ADR 0001](https://github.com/apresai/2ndbrain/blob/main/docs/adr/0001-vector-search.md)
- sqlite-vec rescore index (quantizer=bit, oversample=8): 28× faster queries, 25× faster index build vs flat — but this is an ANN/approximation path, not yet the default. — [basic-memory issue #1996](https://github.com/basicmachines-co/basic-memory/issues/1996)
- sqlite-vec benchmarks-ann README documents brute-force (float/int8/bit), rescore, IVF, and DiskANN index types — ANN options exist but are opt-in benchmarks, not stable defaults. — [sqlite-vec benchmarks-ann](https://github.com/asg017/sqlite-vec/blob/main/benchmarks-ann/README.md)
- sqlite-vector (alternative extension): FLOAT32 exact 484 qps; INT8 37.6 qps (13× speedup, 99.5% recall); 1BIT 2.5 qps (195×, 10% recall) — quantization recall degrades sharply at 1-bit. — [sqlite-vector](https://github.com/sqliteai/sqlite-vector)

### Inferences
- At 384 dims and household scale (<10k chunks), exact cosine KNN in sqlite-vec is ~1 ms or less; the cross-encoder reranker (top 20) will dominate latency, not the ANN search. Optimize reranker batching before touching ANN.
- USearch / hnsw_rs / Qdrant edge / LanceDB all introduce a second storage file or a separate process, breaking the "single-file SQLite" invariant. Not worth it unless exact sqlite-vec is measured too slow at the deployment corpus size.
- int8 quantization (~13× speedup, ~99.5% recall) is the only quantization worth keeping in reserve; 1-bit (10% recall) is unacceptable for citation-correct search.

### Gaps
- No measured p95 latency for Paper's exact KNN at realistic household corpus sizes; add a bench target (e.g., p95 < 50 ms at 10k chunks) to the eval harness/CI.

## Metadata/structure: file-type/date/path filters, recency weighting, document-level vs chunk-level aggregation

### Takeaway
Add metadata filters (file type, path root, mtime/date range) as SQL WHERE clauses on the existing `chunks` table before KNN/FTS5 — they are cheap and directly useful for household queries ("tax PDFs from 2025", "warranties in /Manuals"). Recency weighting and document-level aggregation are lower priority; chunk-level retrieval with citations already satisfies Paper's extractive model. Do not add recency scoring by default — it can demote an old but still-correct warranty clause.

### Cited Findings
- Metadata/structure: file-type/date/path filters, recency weighting, document-level vs chunk-level aggregation are listed as open retrieval-design questions for local document search (this project's own context). — [AGENTS.md project context](/Users/p0is0n/GitHub/enzo/AGENTS.md)
- sqlite-vec KNN supports a WHERE clause on the joined `chunks` table; tier1 already filters by root path in Rust after the query. Filters can be pushed into SQL instead. — [src/core/search/tier1.rs](src/core/search/tier1.rs)
- FTS5 external content + triggers + WAL already store path, mtime, size, content_hash per chunk (schema.rs / indexer.rs), so date and path filters need no new schema. — [src/core/index/schema.rs](src/core/index/schema.rs)

### Inferences
- `mtime` is already indexed per chunk; a `--before`/`--after` date filter or `ext:pdf` filter is a WHERE clause addition to tier0/tier1 SQL, low complexity, high usefulness for receipts/tax queries.
- Document-level aggregation (collapse multiple chunks from one file into one result) conflicts with Paper's exact-citation invariant (every hit carries path+line_start+line_end). Keep chunk-level hits; if a user wants "the whole warranty", that's a parent-child expansion (see chunking), not a ranking change.
- Recency weighting is a relevance signal, not a filter; for household documents an old manual is still the correct answer to "how do I reset the fridge". Avoid by default; expose only as an opt-in config knob if eval shows recency bias helps.

### Gaps
- No query-language design yet for filters (e.g., `receipts after:2025-01-01` vs `--after` flag); needs CLI/MCP design separate from retrieval quality research.

## Query understanding: dates/numbers/identifiers, spelling tolerance, query expansion

### Takeaway
Do NOT add query expansion or an LLM-based query rewriter (adds model, non-determinism, breaks the extractive/no-LLM invariant). Add lightweight, deterministic handling: (1) preserve quoted phrases and identifiers as exact FTS5 terms (already double-quoted), (2) a stopword-aware OR already handles "receipts over $500 in 2025" by OR-ing content words while numbers/symbols tokenize as their own terms, (3) defer spelling tolerance/fuzzy — household documents contain exact identifiers (invoice numbers, model numbers) where fuzzy matching can surface wrong docs; add only if eval shows typo queries are a real user pain point.

### Cited Findings
- FTS5 MATCH terms are double-quoted so user input can never inject FTS5 operators; OR combination with bm25 makes natural-language questions work because AND of every word often matches nothing. — [src/core/search/tier0.rs fts5_match_expr](src/core/search/tier0.rs)
- STOPWORDS list filters common words; stopword-only queries fall back to OR of all terms. — [src/core/search/tier0.rs STOPWORDS](src/core/search/tier0.rs)
- Query expansion / rewriting requires a generative model, violating invariant #3 (answers are extractive, no generated prose without an explicit engine) and the "no mandatory large downloads / no accounts" constraint. — [AGENTS.md invariants]( /Users/p0is0n/GitHub/enzo/AGENTS.md)
- Spelling tolerance/fuzzy search can surface wrong documents for exact identifiers (invoice/model numbers); household search benefits more from exact lexical match than fuzzy. — [project context: warranties, receipts, tax PDFs, manuals with exact citations]( /Users/p0is0n/GitHub/enzo/AGENTS.md)

### Inferences
- "receipts over $500 in 2025": numeric range and date filtering belong in metadata filters (date range + amount extraction), not in the embedding/keyword query. The keyword search will OR "receipts" "500" "2025"; amounts over $500 require structured extraction (see Aggregation section), not query understanding.
- bge-small / e5 embeddings already encode some semantic tolerance for paraphrase ("warranty expiry" vs "when does coverage end"), so query expansion gives diminishing returns for this embedder already.
- Keep MIN_QUERY_LEN=3 and stopword handling as-is; they are reasonable defaults for short household queries.

### Gaps
- No amount/currency extraction from receipts (needed for "over $500"); see Aggregation section — structured field extraction is the right layer, not query understanding.

## Aggregation/extraction questions: structured extraction + deterministic aggregation vs text-to-SQL vs local LLM

### Takeaway
Do NOT add text-to-SQL or a local LLM for aggregation questions — both break the extractive/no-generative invariant and add model weight. Instead: deterministic field extraction at index time (merchant, date, amount, warranty/expiry dates via regex over chunk text into a structured `metadata`/sidecar table) + SQL aggregation for "list every warranty and expiry" / "total spent on appliances 2025", with RAG only providing citations/explanation. This keeps answers verifiable and citations exact.

### Cited Findings
- Text-to-SQL + semantic search combined is the recommended pattern for queries needing both structured analytics and unstructured context; LLMs achieve high accuracy on simple text-to-SQL but drop significantly on hard queries. — [LlamaIndex: Combining Text-to-SQL with Semantic Search](https://www.llamaindex.ai/blog/combining-text-to-sql-with-semantic-search-for-retrieval-augmented-generation-c60af30ec3b); [Review of LLM-based Text-to-SQL Systems](https://arxiv.org/html/2410.01066v2)
- Production pattern: extract metadata from unstructured documents into a SQL table; use SQL for structured queries and RAG only for explanations/citations ("SQL finds the invoice rows, RAG provides explanations"). — [HuggingFace discuss: Local Document RAG System](https://discuss.huggingface.co/t/help-with-a-local-document-rag-system-storage-ingestion-query-highlighting/176993)
- RAG alone is poor at grouping/aggregation; those are the most common enterprise/document queries and need structured data. — [Reddit: Will RAG be effective or text-to-SQL?](https://www.reddit.com/r/LangChain/comments/1j7uu82/will_rag_be_effective_for_my_use_case_or_should_i/)
- Answers are extractive; no generated prose without an explicit engine — invariant #3. — [AGENTS.md invariants](/Users/p0is0n/GitHub/enzo/AGENTS.md)

### Inferences
- "list every warranty and expiry" = SELECT from a warranties sidecar table (vendor, expiry_date, path, line citation) — deterministic, no model, exact citations preserved.
- "total spent on appliances 2025" = SUM(amount) WHERE category='appliance' AND year=2025 from an extracted receipts table; RAG cites the source receipts.
- Extraction at index time via regex/patterns (dates `\d{4}-\d{2}-\d{2}`, amounts `\$\d+(?:\.\d{2})?`) is local, deterministic, and needs no downloads. More complex extraction (merchant name normalization) can be opt-in later.
- The `chunks.metadata` column already exists as `'{}'` JSON in indexer.rs — a natural place to store extracted fields, or a separate sidecar table keyed by chunk id.

### Gaps
- No extraction rules defined yet (what counts as "appliance", date formats, currency symbols); needs a small config/schema addition separate from retrieval-quality research.
- No SQL aggregation surface in CLI/MCP yet; this is a feature beyond "release candidate retrieval quality" but directly answers the aggregation question type in the eval harness.

## Reranking: measured benefit of cross-encoder, candidate count, CPU latency; ColBERT/flashrank

### Takeaway
Keep the cross-encoder reranker (bge-reranker-base, opt-in, 1.13 GB) but restrict it to top ~20 fused candidates and measure its actual nDCG/MRR delta on the household corpus before making it default. Cross-encoders are the gold standard for reranking quality but cost tens of milliseconds per pair on CPU — reranking 20 candidates is acceptable; reranking everything is not. ColBERT/flashrank are NOT worth it: they need multi-vector storage and a non-SQLite engine, breaking the single-file invariant.

### Cited Findings
- Cross-encoders read query + candidate together as one sequence and output a single relevance score per pair; cannot be precomputed; tens of milliseconds per pair on CPU. — [Rerankers Aren't Magic Either](https://towardsdatascience.com/rerankers-arent-magic-either-when-the-cross-encoder-layer-is-worth-the-cost-enterprise-document-intelligence-vol-1-2bis)
- zk2 ADR CPU reranking: cross-encoder/mmarco-mMiniLMv2-L12-H384-v1 (118M) 3.5 s for 30 candidates; BAAI/bge-reranker-base (278M) 18 s for 30 candidates — per-pair CPU cost is real and model-size dependent. — [zk2 ADR 0016](https://github.com/zk2/zk2-rag-platform/blob/main/docs/adr/0016-cpu-reranking.md)
- bge-reranker-v2-m3 ONNX-CPU: rerank 1 query × 10 docs ≈ 8.9 s — but this appears to be a slow/unoptimized path; other measures suggest per-pair tens of ms. — [a-ivanovitch/bge-reranker-v2-m3-onnx](https://huggingface.co/a-ivanovitch/bge-reranker-v2-m3-onnx)
- "Top 7 rerankers under 120 ms" — small cross-encoders + runtime optimization can meet interactive latency. — [Quellin](https://medium.com/@npavfan2facts/top-7-rerankers-that-stay-under-120ms)
- Reranking benefit is measured delta, not assumed: Qdrant's "When Is a Reranker Worth It?" measures cross-encoder impact rather than declaring it universally valuable. — [Qdrant](https://qdrant.tech/documentation/search-tuning/when-a-reranker-is-worth-it/)
- ColBERT late interaction / flashrank require multi-vector MaxSim scoring and non-SQLite storage; ColSearch is CPU/GPU but a separate engine. — [colsearch](https://github.com/ddickmann/colsearch); [Late Chunking arxiv](https://arxiv.org/html/2409.04701v1)

### Inferences
- Paper already reranks only `rerank_top` (default 20) candidates and appends the rest unreranked — the right shape. The question is whether reranking changes the top-3 answer chunk at all; eval should measure "rerank changed top hit" rate.
- bge-reranker-base at 1.13 GB is a large opt-in download; if eval shows <5% nDCG@10 gain over RRF alone on household docs, make reranker opt-in only and default to RRF (satisfies "no mandatory large downloads").
- CPU latency target: reranking 20 pairs should complete well under ~500 ms on a desktop CPU to keep total search interactive.

### Gaps
- No measured nDCG/MRR delta from the cross-encoder on Paper's household corpus; the eval harness must compare RRF-only vs RRF+rerank to decide if the 1.13 GB download is justified.

## Evaluation: reproducible local benchmark, corpus, question types, metrics, CI

### Takeaway
Build an offline, CI-runnable eval harness in the repo (no network, no model downloads) using a deterministic HashEmbedder (already exists in tests/common/mod.rs) plus a small hand-labeled household-doc golden set. Metrics: recall@k, MRR, nDCG@10, citation accuracy (path+line resolve to real content), and abstention correctness (Correct Abstention Rate on unanswerable questions). Question types: exact identifier, paraphrase, cross-lingual, aggregation, unanswerable. CI must run in < a few minutes and fail on regression.

### Cited Findings
- ragcheck: offline RAG retrieval-quality harness — Recall@k, nDCG, MRR, chunking diagnostics, regression diffs; no LLM-as-judge required; CI-friendly. Metrics: dcg@k/ndcg@k = Σ(2^gain−1)/log₂(i+1); MRR = reciprocal rank of first relevant item. — [JSLEEKR/ragcheck](https://github.com/JSLEEKR/ragcheck)
- hybrid-graph-rag: labeled benchmark with recall@k/MRR/nDCG and metrics.py computed from scratch (auditable, not hidden in a library). — [tahasiddiquii/hybrid-graph-rag](https://github.com/tahasiddiquii/hybrid-graph-rag)
- hybrid-rag-pipeline: reproducible eval science — nDCG/recall@k, verified citations, paired bootstrap CI95. — [anbsamsam17/hybrid-rag-pipeline](https://github.com/anbsamsam17/hybrid-rag-pipeline)
- rag-bench-2026q2: dated reproducible benchmark — Recall@5, MRR, NDCG@10, faithfulness, citation accuracy, latency p95, $/1k queries. — [paiteq-ai/rag-bench-2026q2](https://huggingface.co/datasets/paiteq-ai/rag-bench-2026q2)
- Paper already has tests/common/mod.rs with a deterministic offline HashEmbedder so retrieval logic never needs real weights. — [AGENTS.md Layout]( /Users/p0is0n/GitHub/enzo/AGENTS.md)
- Paper invariants: citations must resolve (path, line_start, line_end); answers are extractive; search never downloads a model. — [AGENTS.md Invariants](/Users/p0is0n/GitHub/enzo/AGENTS.md)

### Inferences
- Corpus composition for the golden set should mirror real household docs: warranties (with expiry dates), receipts (merchant/amount/date), tax PDFs, manuals — a mix of PDFs (if text-extractable) and plain text/markdown, ~50–200 documents, few thousand chunks, so exact KNN stays fast and the harness runs in CI.
- Question types map directly to Paper's gaps: exact identifier (invoice/model number) tests keyword early exit; paraphrase tests semantic; cross-lingual tests multilingual-e5 opt-in; aggregation tests structured extraction (not retrieval); unanswerable tests abstention.
- Citation accuracy metric: for each retrieved hit, verify path exists and line_start..line_end content contains the expected keyword/phrase — resolves invariant #2 deterministically without an LLM judge.
- Use the existing HashEmbedder in tests so CI needs zero downloads; optionally run a real-weight variant locally (ignored in CI, like model integration tests).

### Gaps
- No golden question set exists yet; needs ~60–100 labeled questions with relevant chunk ids and answerability flags.
- No harness code exists yet; needs a new tests/ or benches/ module with metric implementations from scratch (auditable, following ragcheck/hybrid-graph-rag precedent).
- No CI gate for retrieval quality yet; needs a threshold (e.g., nDCG@10 ≥ 0.7, recall@10 ≥ 0.85 on the golden set) before release candidate.

## Hallucination/abstention: when not to answer, defensible thresholds

### Takeaway
Abstain when there is no corroborating evidence: no keyword hit with full term coverage, no semantic hit ≥ floor (0.25), or top fused confidence is Low. Answer only when confidence is Medium or High. Do not answer aggregation questions from retrieval alone — route them to structured extraction/SQL (see Aggregation section). Thresholds should be penalty-aware: abstain unless P(correct) is high enough that the cost of a wrong answer exceeds the cost of silence.

### Cited Findings
- Penalty-aware evaluation: a system that believes its answer is correct with probability p has expected quality p − k(1−p) from answering vs 0 from abstaining; rational to answer iff p ≥ k/(k+1). With k=4 that is 80% confidence threshold; k∈{1,2,4,9} → 50%, 67%, 80%, 90%. — [Why RAGs Hallucinate: Penalty-Aware Evaluation](https://arxiv.org/html/2608.26385v1)
- Correct Abstention Rate (CAR): percentage of unanswerable questions on which the model acknowledges insufficient knowledge without a premise denial or unsupported answer. — [DeclineCraft: A RAG Abstention Benchmark](https://arxiv.org/pdf/2610.28340.pdf)
- Three-action framework: abstain when Conf(q)<0.35 ∧ Coverage(q)<0.30, or a NOT_ANSWERABLE classifier fires. — [A Three-Action Framework for Epistemically Calibrated QA](https://arxiv.org/html/2604.04565)
- Confidence-aware RAG integrates a judge with a confidence threshold of 0.6 for abstention decisions. — [Microsoft: Confidence-Aware RAG](https://techcommunity.microsoft.com/blog/azuredevcommunityblog/confidence-aware-rag-teaching-your-ai-pipeline-to-acknowledge-uncertainty/4515061)
- Paper's existing Confidence {High, Medium, Low} and confidence_for() already combine keyword + semantic corroboration — a defensible, rule-based abstention signal without an LLM judge. — [src/core/search/pipeline.rs confidence_for](src/core/search/pipeline.rs)

### Inferences
- Paper can implement abstention purely from its existing confidence signal: High/Medium → answer; Low → not_found (abstain). This is deterministic, explainable, and needs no model. Map Medium→answer but only if not aggregation-typed; Low→abstain.
- The penalty-aware formula gives a principled way to pick the threshold: choose k based on how costly a wrong answer is for household docs (warranty/tax advice → costly → high k → high threshold, e.g., 80%). Default to Medium-or-higher as the answer bar; tune k via eval on unanswerable questions.
- Citation accuracy and abstention are jointly evaluated: a cited answer that cites wrong lines is a hallucination even if the text is verbatim. The eval harness must check citation line content matches the claim.

### Gaps
- No NOT_ANSWERABLE classifier or query-type router exists; abstention is currently only "no hits → not_found". Low-confidence hits still produce an answer today.
- No cost/penalty parameter k exposed in config; abstention threshold is implicit in Confidence enum, not tunable.

## Prioritized recommendations (quality gain per complexity unit)

### Worth doing (high gain, low/moderate complexity)
1. **Eval harness first** — golden set + recall@k/MRR/nDCG@10/citation accuracy/abstention correctness, CI-gated, offline via HashEmbedder. Everything else is unmeasurable without this.
2. **Structure-aware chunking** — split on headings/pages, cap at 400 words + 40 overlap, populate `page`. Prevents mid-clause cuts and broken citations.
3. **Metadata filters as SQL WHERE** — file type, path root, date range on existing `chunks` columns (mtime, size, path). Low complexity, high usefulness for receipts/tax.
4. **Fix early exit** — don't skip semantic purely on `kw_hits >= 10`; exit only on High keyword confidence. Prevents suppressing the right doc for OR-heavy queries.
5. **Deterministic field extraction + SQL aggregation** — merchant/date/amount/expiry via regex into `metadata`/sidecar; SQL for "list warranties"/"total spent". Keeps answers extractive and citable.
6. **Abstention from existing confidence** — answer only on Medium/High; Low → not_found. Optional penalty parameter k for tuning.

### Keep as-is (already good / not worth changing)
- RRF(k=60) fusion — rank-based, avoids score-incompatibility; keep unless eval shows otherwise.
- Exact sqlite-vec KNN — fast enough (<10k chunks); don't add ANN/quantization unless measured >100 ms p95.
- Cross-encoder reranker — keep opt-in, top-20 candidates; measure delta before defaulting.
- bge-small default 134 MB — already local and pinned.

### NOT worth it (flag explicitly)
- Query expansion / LLM query rewriter — breaks extractive/no-LLM invariant, adds model, diminishing returns given bge already handles paraphrase.
- Spelling tolerance/fuzzy search — risks wrong docs for exact identifiers (invoice/model numbers); defer unless eval shows typo queries are common.
- Semantic chunking / late chunking / ColBERT / flashrank — require long-context encoders or multi-vector non-SQLite storage; high complexity, unproven gain for small household corpora.
- Weighted score fusion (α-normalized) — sensitive to BM25/cosine normalization; RRF already solves the problem it addresses.
- Recency weighting by default — can demote a still-correct old warranty clause; opt-in only if eval shows benefit.
- text-to-SQL / local LLM for aggregation — breaks extractive invariant; use deterministic extraction + SQL instead.

### Open gaps to close before release candidate
- Household golden question set (~60–100 labeled questions across the 5 question types).
- Harness code + CI thresholds (nDCG@10, recall@10, citation accuracy, abstention correctness).
- Measured cross-encoder nDCG/MRR delta and p95 latency at target corpus size.
- Extraction rules (amount/date/expiry regexes) and aggregation SQL surface.
- Abstention cost parameter k exposed in config.
