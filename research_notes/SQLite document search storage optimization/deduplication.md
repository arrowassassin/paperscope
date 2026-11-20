# Content Deduplication Algorithms for SQLite-Based Document Search Systems

## State-of-the-Art Content-Defined Chunking (CDC) Algorithms

### Takeaway
FastCDC (2016/2020) is the current state-of-the-art for CDC, offering ~10x speedup over Rabin-based CDC while achieving nearly identical deduplication ratios. It uses Gear hashing with normalized chunking (dual-mask) to produce tight chunk-size distributions. Pure Rust implementations exist (`fastcdc` crate, `chunkrs`, `cdchunking`).

### Cited Findings
- FastCDC is ~10x faster than best open-source Rabin-based CDC and ~3x faster than Gear/AE-based CDC, with nearly the same deduplication ratio as classic Rabin [USENIX ATC 2016 paper](https://www.usenix.org/conference/atc16/technical-sessions/presentation/xia)
- FastCDC v2020 adds "rolling two bytes each time" optimization and normalized chunking with dual masks, producing identical cut points to v2016 but faster [fastcdc crate docs](https://docs.rs/fastcdc)
- Gear hashing: single left-shift + table lookup per byte (`fp = (fp << 1) + G[b]`), dramatically faster than Rabin's polynomial arithmetic [Joshleeb's Gear Hashing blog](https://joshleeb.com/posts/gearhash.html)
- Rabin fingerprinting computes polynomial rolling hash byte-by-byte with modulo irreducible polynomial, causing heavy CPU overhead [FastCDC paper](https://www.usenix.org/system/files/conference/atc16/atc16-paper-xia.pdf)
- FastCDC's normalized chunking (dual-mask) clusters chunks tightly around target size, avoiding exponential distribution of basic CDC [A Deep Dive into FastCDC](https://rickwinfrey.com/writings/content-defined-chunking-part-2)
- cdchunking-rs crate provides ZPAQ-based CDC (used in restic) with Iterator API [cdchunking-rs](https://gitmemories.com/remram44/cdchunking-rs)
- chunkrs crate: FastCDC + BLAKE3, streaming-first, `#![forbid(unsafe_code)]`, determinism guarantees [chunkrs on lib.rs](https://lib.rs/crates/chunkrs)

### Inferences
- For a local-first document search system, FastCDC v2020 via the `fastcdc` crate is the best choice: pure Rust, mature, deterministic, and fast
- Gear hashing (via `gearhash` crate) is the core building block; FastCDC adds normalized chunking on top
- Rabin is outdated for this use case — slower with no deduplication advantage
- ZPAQ (cdchunking) is used by restic but FastCDC is faster with equivalent ratios

### Gaps
- No direct benchmarks on *text corpora specifically* (most benchmarks use binary data, VM images, source code)
- Optimal chunk size parameters for document search (vs backup) not well documented

---

## Fixed-Size vs Variable-Size CDC Tradeoffs

### Takeaway
Variable-size CDC (FastCDC) provides superior deduplication for text/documents because it handles insertions/deletions without boundary shift. Fixed-size chunking is simpler but fails when content shifts. For document search with versioned/edited content, CDC is strongly preferred.

### Cited Findings
- Fixed-size chunking: boundary-shift problem — inserting 1 byte shifts all subsequent chunk boundaries, destroying deduplication [Content-Defined Chunking emergentmind](https://www.emergentmind.com/topics/content-defined-chunking-cdc)
- Variable-size CDC: boundaries determined by content patterns; local edits only affect nearby chunks [Restic Foundation CDC blog](https://restic.net/blog/2015-09-12/restic-foundation1-cdc)
- Duplicacy forum: fixed-size better for DB/VM images (aligned), variable-size better for general files [Duplicacy forum](https://forum.duplicacy.com/t/deduplicaction-and-variable-vs-fixed-size-chunk-size/995)
- ResearchGate: variable-size chunking achieves higher deduplication ratio than fixed-size [ResearchGate figure](https://www.researchgate.net/figure/Comparison-of-variable-size-chunking-fixed-size-chunking-and-three-way-chunking-The_fig2_327525835)
- Metadata overhead: CDC produces more variable chunk counts, requiring more index entries [Deduplication in Action part 3](https://rickwinfrey.com/writings/content-defined-chunking-part-3)
- Borg/kopia/restic all use CDC (variable-size) for backup deduplication [Kopia vs restic comparison](https://stackharbor.com/en/knowledge-base/restic-borg-kopia-duplicacy-comparison)

### Inferences
- For document search (where files are edited, not just appended), CDC is essential
- The metadata overhead of CDC is acceptable given SQLite's ability to handle millions of rows
- Fixed-size chunking only wins for strictly aligned data (databases, VM images)

### Gaps
- Quantitative deduplication ratio comparison on *document corpora* (Markdown, PDF text, code) lacking
- Optimal average chunk size for semantic search chunks (current paper uses ~512-1024 token chunks)

---

## SQLite Implementation: Chunk Hashes, Reference Counting, Content-Addressable Storage

### Takeaway
Store chunk hashes (BLAKE3) in a dedicated `chunks` table with `UNIQUE` constraint on hash, reference count column, and BLOB storage for content. Use SQLite's `INSERT OR IGNORE` + `UPDATE ref_count` pattern for deduplication at ingestion. This integrates naturally with existing `chunks` + `chunks_vec` schema.

### Cited Findings
- Tormentnexus benchmark: `chunks` table with `chunk_hash TEXT UNIQUE NOT NULL` (SHA256) for deduplication, vector stored as BLOB [Benchmarking local AI pipeline](https://tormentnexus.site/blog/tormentnexus/benchmarking-the-local-ai-pipeline-achieving-sub-10ms-semantic-search-with-sqlite)
- TechInterview LLD: `FileBlob` table with `sha256_hash CHAR(64) UNIQUE`, `ref_count INT DEFAULT 0`, `BlobChunk` join table for file→chunk mapping [File Deduplication LLD](https://www.techinterview.org/post/3233468290/lld-file-deduplication)
- Shaken Fist blob storage roadmap: composite blobs referencing chunk hashes, `blob_hashes` table with `idx_hash_lookup` index [Shaken Fist roadmap](https://shakenfist.com/plans/blob-storage-roadmap)
- SQLite BLOB storage best practice: put BLOB column last in table definition to avoid reading it unless needed [SQLite forum](https://sqlite.org/forum/forumpost/d73b37b6b5?t=h&unf=)
- Content-addressable storage: hash = address; deduplication happens naturally at insert via UNIQUE constraint [CAS explained](https://stonefly.com/blog/content-addressable-storage-enterprise-guide)

### Inferences
- Schema pattern:
  ```sql
  CREATE TABLE chunks (
    id INTEGER PRIMARY KEY,
    hash TEXT NOT NULL UNIQUE,        -- BLAKE3 hex, content address
    content BLOB NOT NULL,            -- raw chunk text (last column)
    ref_count INTEGER NOT NULL DEFAULT 1,
    line_start INTEGER NOT NULL,      -- for citation resolution
    line_end INTEGER NOT NULL,
    doc_id INTEGER NOT NULL,          -- FK to documents
    chunk_order INTEGER NOT NULL      -- position in document
  );
  CREATE INDEX idx_chunks_hash ON chunks(hash);
  ```
- Ingestion: `INSERT OR IGNORE INTO chunks (hash, content, ...) VALUES (?, ?, ...)` then `UPDATE chunks SET ref_count = ref_count + 1 WHERE hash = ?`
- `sqlite-vec` virtual table (`chunks_vec`) references `chunks.id` — no duplication of vectors
- BLAKE3 preferred over SHA-256: faster (parallelizable), same 256-bit security, Rust `blake3` crate is pure Rust + SIMD

### Gaps
- How to handle garbage collection when `ref_count` reaches 0 (delete chunk + vector)
- Whether to store chunk content as TEXT or BLOB (UTF-8 text → TEXT is fine, but BLOB is more general)
- Impact on `sqlite-vec` KNN performance with deduplicated chunks (fewer vectors = faster search)

---

## Rust Crates for CDC and Deduplication

### Takeaway
`fastcdc` (v2020) is the recommended crate: pure Rust, mature (200+ stars), deterministic, supports streaming. `chunkrs` is a modern alternative with stricter safety (`#![forbid(unsafe_code)]`) and BLAKE3 integration. `gearhash` provides the low-level rolling hash with SIMD acceleration. `blake3` for content hashing.

### Cited Findings
- `fastcdc` crate: implements FastCDC v2016 and v2020, pure Rust, streaming `StreamCDC`, deterministic [fastcdc docs](https://docs.rs/fastcdc)
- `chunkrs`: FastCDC + BLAKE3, streaming-first, async-agnostic, zero-copy, `forbid(unsafe_code)` [chunkrs on lib.rs](https://lib.rs/crates/chunkrs)
- `gearhash` crate: SIMD-accelerated (SSE4.2, AVX2, NEON) Gear hash, building block for FastCDC [gearhash docs](https://docs.rs/gearhash)
- `cdchunking`: ZPAQ algorithm (used by restic), Iterator API [cdchunking-rs](https://gitmemories.com/remram44/cdchunking-rs)
- `blake3` crate: pure Rust, SIMD, parallelizable, faster than SHA-256 [BLAKE3 research](https://guptadeepak.com/research/blake2-blake3-hashing)
- `rustic_cdc` crate: Rabin64 implementation (used by rustic backup) [rustic_cdc](https://crates.io/crates/rustic_cdc)

### Inferences
- `fastcdc` + `blake3` is the minimal, well-supported stack for paper
- `chunkrs` could replace both if its API stabilizes (newer, fewer stars)
- No C dependencies — all pure Rust, aligns with paper's single-binary constraint
- `gearhash` SIMD acceleration automatically used on supported CPUs

### Gaps
- `chunkrs` maturity vs `fastcdc` (200+ stars vs 2 stars)
- Streaming vs in-memory chunking performance for typical document sizes (< 1 MB)

---

## Integration with Paper's Existing Architecture

### Takeaway
Paper's current schema (`chunks` + `chunks_vec` + `chunks_fts`) can accommodate deduplication by adding a `hash` column with `UNIQUE` constraint and `ref_count`. The CDC chunker replaces the current line-based chunker. Semantic search continues to work — fewer unique chunks means faster KNN.

### Cited Findings
- Paper's current schema: `chunks` table with `id`, `path`, `line_start`, `line_end`, `body`; `chunks_vec` virtual table with `float[384]` vectors [implementation-status.md](/Users/p0is0n/GitHub/enzo/docs/implementation-status.md)
- `sqlite-vec` stores vectors in shadow tables, supports exact KNN up to ~500k chunks [sqlite-vec blog](https://alexgarcia.xyz/blog/2024/building-new-vector-search-sqlite/index.html)
- Binary quantization (1-bit) reduces vector storage 32x with modest recall loss after re-ranking [Local-First RAG with sqlite-vec](https://joinloop.co.in/blog/local-first-rag-sqlite-vec-fts5-hybrid-search)
- Paper uses `PAPER_HOME` for data dir, `PAPER_OFFLINE=1` for no-network — deduplication must work fully offline [implementation-status.md](/Users/p0is0n/GitHub/enzo/docs/implementation-status.md)

### Inferences
- Migration path: add `hash` + `ref_count` columns to `chunks`, backfill with BLAKE3 of `body`, create UNIQUE index
- CDC chunker runs at index time: `FastCDC::new(&content, min, avg, max)` → for each chunk, compute BLAKE3, upsert into `chunks`
- Line-number fidelity (Invariant #2) maintained by tracking `line_start`/`line_end` per CDC chunk
- Deduplication reduces vector count → faster semantic search, smaller DB
- Cross-document deduplication: identical paragraphs across files stored once

### Gaps
- How to handle chunk boundary changes on re-index (document edited): old chunks' `ref_count` decremented, new chunks inserted
- Whether CDC chunk boundaries align with semantic boundaries (paragraphs, sections) for citation quality
- Optimal `min`/`avg`/`max` chunk sizes for document search (current: ~500 tokens ≈ 2KB)

---

## Summary Recommendations

| Decision | Recommendation | Rationale |
|----------|---------------|-----------|
| CDC Algorithm | FastCDC v2020 (`fastcdc` crate) | 10x faster than Rabin, same dedup ratio, pure Rust, deterministic |
| Chunk Hash | BLAKE3 (`blake3` crate) | Fastest cryptographic hash, SIMD, pure Rust, 256-bit |
| Chunk Size | min=512, avg=2048, max=4096 bytes | Aligns with ~500 token semantic chunks; tune empirically |
| Schema | Add `hash TEXT UNIQUE`, `ref_count INTEGER` to `chunks` | Native SQLite deduplication via UNIQUE constraint |
| Ingestion | `INSERT OR IGNORE` + `UPDATE ref_count` | Atomic, no race conditions in single-writer CLI |
| Vector Storage | Unchanged (`chunks_vec` references `chunks.id`) | Fewer unique chunks = smaller vec index = faster KNN |
| GC | `DELETE FROM chunks WHERE ref_count = 0` + vec cleanup | Run on `paper index --prune` or explicit command |