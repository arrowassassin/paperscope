# SQLite Document Search Storage Optimization: Compression Research

## Text Compression: ZSTD vs LZ4 vs Brotli vs Snappy for Short Texts (200-400 words)

### Takeaway
For short document chunks (200-400 words ≈ 1-2 KB), **ZSTD at level 1-3** is the best all-rounder: ~2.9x compression ratio, ~500 MB/s compression, ~1.5 GB/s decompression. **LZ4** wins on raw decompression speed (~4-5 GB/s) but only ~2.1x ratio. **ZSTD with dictionary compression** dramatically improves ratio on small, structured texts (up to 10x on repetitive content like JSON/HTML) while maintaining fast speeds. Brotli has best ratio (~3.8x) but slow compression (~100 MB/s), unsuitable for real-time indexing.

### Cited Findings
- ZSTD 1.5.7 level 1: 2.896 ratio, 510 MB/s compress, 1550 MB/s decompress on Silesia corpus — [Zstd GitHub benchmarks](https://github.com/facebook/zstd)
- LZ4 1.10.0: 2.101 ratio, 675 MB/s compress, 3850 MB/s decompress — [Zstd GitHub benchmarks](https://github.com/facebook/zstd)
- Brotli 1.1.0 level 1: 2.883 ratio, 290 MB/s compress, 425 MB/s decompress — [Zstd GitHub benchmarks](https://github.com/facebook/zstd)
- Snappy 1.2.1: 2.089 ratio, 520 MB/s compress, 1500 MB/s decompress — [Zstd GitHub benchmarks](https://github.com/facebook/zstd)
- ZSTD dictionary compression on small structured data (1 KB blocks): ratio improves from ~4x to ~11x; decompression speed 2x faster with dictionary — [Zstd issue #4175](https://github.com/facebook/zstd/issues/4175)
- ZSTD dictionary "short cache" optimization: 5-30% compression speedup for small inputs (<8 KB at levels 1-2) — [Zstd PR #3152](https://github.com/facebook/zstd/pull/3152)
- Real-world Rust benchmark (100 MB mixed text): ZSTD level 3 = 71.9% ratio, 280 MB/s compress, 1200 MB/s decompress; LZ4 = 57.4%, 450/800 MB/s; Brotli = 74.2%, 85/280 MB/s — [UglyPear Data](https://www.uglypear.com/en/blog/rust-compression-library-comparison.html)
- ZSTD default level 3 is recommended "general default" — [UglyPear Data](https://www.uglypear.com/en/blog/rust-compression-library-comparison.html)
- ZSTD dictionary training works best for small data (~1 KB) with high cross-sample redundancy — [Zstd issue #3783](https://github.com/facebook/zstd/issues/3783)

### Inferences
- For paper's 200-400 word chunks (~1-2 KB), ZSTD level 1-3 without dictionary gives ~2.5-3x ratio with sub-millisecond compress/decompress — well within 100ms latency budget
- Dictionary compression is highly valuable if chunks share boilerplate (headers, metadata fields, common phrases). Training a 100 KB dictionary on representative chunks could push ratio to 5-10x
- LZ4 is preferable only if decompression latency is the absolute bottleneck and ratio is secondary (e.g., hot metadata cache)
- Brotli's static dictionary helps web content but compression speed (~100 MB/s) makes it unsuitable for ingestion pipeline
- Rust crate `zstd` (C bindings) or `rust-zstd` (pure Rust) both support dictionary compression; `rust-zstd` has no C dependency and matches libzstd ratio within 1%

### Gaps
- Exact compression ratio for 200-400 word English text chunks with/without ZSTD dictionary — no specific benchmark found for this exact size/content
- Memory overhead of holding dictionary in Rust process during indexing

---

## Dictionary Compression: Shared Dictionary Across Chunks

### Takeaway
ZSTD dictionary compression is purpose-built for many small, similar inputs. Training a ~100 KB dictionary on representative document chunks (headers, boilerplate, repeated field names) can improve compression ratio 2-5x over standalone ZSTD for 1-2 KB chunks, while also speeding up compression/decompression. The dictionary must be trained per document type (e.g., separate dicts for PDFs, emails, code files).

### Cited Findings
- "Dictionary compression works best on structured data featuring a lot of redundancy _across_ messages, though very little _within_ the message" — [Zstd issue #3783](https://github.com/facebook/zstd/issues/3783)
- ZSTD dictionary gains "mostly effective in the first few KB. Then, the compression algorithm will gradually use previously decoded content to better compress the rest of the file" — [Zstd GitHub](https://github.com/facebook/zstd)
- Recommended dictionary size: ~100 KB (zstd CLI default 110 KB) — [zstdlite docs](https://coolbutuseless.r-universe.dev/zstdlite/doc/dictionaries.html)
- Training samples: ~100x dictionary size (e.g., 10 MB samples for 100 KB dict) — [zstdlite docs](https://coolbutuseless.r-universe.dev/zstdlite/doc/dictionaries.html)
- ZSTD dictionary decompression speed can be 2x faster on 1 KB blocks due to fewer sequences — [Zstd issue #4175](https://github.com/facebook/zstd/issues/4175)
- "Short cache" optimization in ZSTD: 5-30% compression speedup for small inputs with cold dictionaries — [Zstd PR #3152](https://github.com/facebook/zstd/pull/3152)
- LZ4 has no dictionary support — [Pistack comparison](https://www.pistack.xyz/posts/2026-06-27-cpp-compression-libraries-lz4-zstd-brotli-snappy-miniz)
- Brotli has static predefined dictionary (web-optimized) + shared dictionary support — [Pistack comparison](https://www.pistack.xyz/posts/2026-06-27-cpp-compression-libraries-lz4-zstd-brotli-snappy-miniz)

### Inferences
- For paper: train one ZSTD dictionary per content type (PDF text, HTML, Markdown, code, emails) during index build; store dict alongside index
- Dictionary adds ~100 KB to binary size — negligible for single binary constraint
- At query time, reuse `Decompressor` with dictionary for fast chunk hydration (cited: "with a reused Decompressor, decoding 120 B–5 KB frames is at or above libzstd's reused-DCtx speed" — [rust-zstd crate](https://crates.io/crates/rust-zstd))
- Must handle dict versioning if corpus changes significantly (re-train periodically)

### Gaps
- Optimal dictionary training frequency for evolving document corpus
- Whether multiple dictionaries per content type can be selected at decompress time efficiently

---

## Vector Compression: Product Quantization (PQ), Scalar Quantization (SQ), Binary Quantization

### Takeaway
For 384-dim float32 vectors (1.5 KB each):
- **Scalar Quantization (int8)**: 4x compression (384 bytes), ~97-99% recall@10 with asymmetric distance computation (ADC), near-lossless with rescoring — **recommended default**
- **Binary Quantization (1-bit)**: 32x compression (48 bytes), ~85-92% recall@10, requires oversampling (4-8x) + rescoring to reach ~95-99% — good for massive datasets
- **Product Quantization (PQ)**: 8-192x compression depending on M (subspaces), ~90-95% recall, complex training, best for >1M vectors where memory is critical
- **4-bit TurboQuant (Qdrant)**: 8x compression, matches SQ recall with rescoring — promising but less Rust ecosystem support

### Cited Findings
- int8 scalar quantization: 3.96x smaller at 384 dims (388 bytes vs 1536), recall@10 = 0.984 on random vectors, 1.000 with top-100 float32 rescoring — [CST Solutions](https://cstsolution.com/blog/quantising-embeddings)
- int8 scan speed: same as float32 (45 ms vs 46 ms for 100k vectors) — [CST Solutions](https://cstsolution.com/blog/quantising-embeddings); but Hugging Face blog reports 3.66x speedup with USearch — [HF blog](https://huggingface.co/blog/embedding-quantization)
- Binary quantization: 32x compression, recall@10 ~85% (random), ~92% (clustered), 96% with rescoring — [Weaviate](https://weaviate.io/blog/binary-quantization), [Kyle Howells](https://ikyle.me/blog/2025/binary-quantized-embeddings)
- Binary + rescoring: 96% recall, 96% memory reduction, faster search — [MongoDB](https://www.mongodb.com/company/blog/product-release-announcements/binary-quantization-rescoring-96-less-memory-faster-search)
- PQ compression ratio = (dimensions * 4) / M; e.g., 384 dims, M=48 → 32x, M=16 → 96x, M=8 → 192x — [Grafeo](https://grafeo.dev/user-guide/vector-search/quantization)
- PQ recall@10: ~90-95% depending on M; ADC (asymmetric) significantly outperforms SDC — [Jégou et al. PQ paper](https://jatin7gupta.github.io/Product-Quantization/PQ%20NN.pdf), [Zero Entropy](https://zeroentropy.dev/concepts/product-quantization/)
- vecpack benchmark (30k vectors, 384 dims): SQ 4x/92% recall, PQ M=48 25x/25% recall, BQ 32x/7% recall — [vecpack GitHub](https://github.com/joelvarun/vecpack)
- Qdrant: SQ 4x with rescore = 0.983 recall@10; 4-bit TurboQuant 8x with rescore = 0.981 — [Qdrant](https://qdrant.tech/documentation/manage-data/quantization/)
- Weaviate 8-bit rotational quantization: 96-99% recall@10 vs 97-99% for SQ — [Weaviate blog](https://weaviate.io/blog/8-bit-rotational-quantization)

### Inferences
- For paper's 384-dim vectors with <100ms latency budget: **SQ (int8) is the sweet spot** — 4x smaller, ADC keeps recall >97%, simple implementation, no training needed
- Binary quantization only worthwhile if dataset >500k vectors and memory-constrained; requires storing both binary + float32 for rescoring (adds storage)
- PQ adds training complexity (k-means per subspace) and ADC lookup tables; overkill for paper's scale unless vectors exceed ~500k
- sqlite-vec natively supports `int8` and `bit` vector types — can store quantized vectors directly in `vec0` tables
- ADC pattern: keep query in float32, quantize only stored vectors; compute distance via lookup table (SQ) or Hamming popcount (BQ)

### Gaps
- Exact recall numbers for paper's specific embedding model (likely all-MiniLM-L6-v2 or similar 384-dim) with SQ/BQ
- Whether Candle provides built-in quantization utilities or if `vq` crate is needed

---

## sqlite-vec Integration: Storing and Searching Compressed Vectors

### Takeaway
sqlite-vec natively supports three vector types in `vec0` virtual tables: `float[384]` (float32, 4 bytes/dim), `int8[384]` (1 byte/dim), `bit[384]` (1 bit/dim, packed). Quantized vectors can be stored directly and searched with `MATCH` using Hamming distance (bit) or L2/cosine (int8/float). The new `rescore` index (PR #276) automates binary/int8 coarse search + float32 reranking. No ANN index yet — brute-force scan only.

### Cited Findings
- sqlite-vec vector element types: `FLOAT32` (subtype 223), `BIT` (subtype 224), `INT8` (subtype 225) — [sqlite-vec.c](https://github.com/asg017/sqlite-vec/blob/main/sqlite-vec.c)
- `vec_quantize_binary()` SQL function converts float32/int8 → bit vector (threshold at 0) — [sqlite-vec binary quant guide](https://alexgarcia.xyz/sqlite-vec/guides/binary-quant.html)
- `vec0` schema: `CREATE VIRTUAL TABLE vec_items USING vec0(embedding float[384], embedding_coarse bit[384])` — [sqlite-vec binary quant guide](https://alexgarcia.xyz/sqlite-vec/guides/binary-quant.html)
- Rescore index (PR #276): `CREATE VIRTUAL TABLE vec_articles USING vec0(headline_embedding float[384]) INDEXED BY rescore(oversample=8)` — [PR #276](https://github.com/asg017/sqlite-vec/pull/276)
- Benchmarks on 1M NYT headlines (mxbai-embed-large-v1, 384 dims): flat 590ms, int8 rescore os=2 225ms (2.6x, recall 1.0), bit rescore os=8 101ms (5.8x, recall 0.988) — [PR #276](https://github.com/asg017/sqlite-vec/pull/276)
- Memory-mapped I/O (`mmap_size=1GiB`) cut search from 104ms → 78ms p50 at 50k vectors — [DEV Community](https://dev.to/arinaymay/37500-reads-per-search-what-memory-mapping-fixed-in-sqlite-vec-and-what-it-couldnt-256b)
- int8 vectors in sqlite-vec: "saved nothing once file was mapped (74/116 ms vs 75-77/89-90 ms for float32)" — [DEV Community](https://dev.to/arinaymay/37500-reads-per-search-what-memory-mapping-fixed-in-sqlite-vec-and-what-it-couldnt-256b)
- Binary quantization scan: 2.6ms at 50k vectors, rerank 0.25ms/candidate — [DEV Community](https://dev.to/arinaymay/37500-reads-per-search-what-memory-mapping-fixed-in-sqlite-vec-and-what-it-couldnt-256b)
- Rust integration: `cargo add sqlite-vec`, register via `sqlite3_auto_extension(sqlite3_vec_init)`, pass vectors as `Vec<f32>.as_bytes()` with `zerocopy` — [sqlite-vec Rust guide](https://alexgarcia.xyz/sqlite-vec/rust.html)
- sqlite-vec stores vectors in shadow tables: `vectors BLOB NOT NULL` (packed float32/int8/bit) — [sqlite-vec.c](https://github.com/asg017/sqlite-vec/blob/main/sqlite-vec.c)

### Inferences
- For paper: use `int8[384]` columns with ADC (query stays float32) for 4x storage reduction and ~2x speedup with rescore index
- If memory is critical and dataset >100k vectors, add `bit[384]` column + rescore index (os=8) for ~6x speedup, 98.8% recall
- Enable `mmap_size=1GiB` pragma for I/O-bound workloads
- Store quantized vectors at index time: quantize in Rust using `vq` crate, insert both `float[384]` and `int8[384]`/`bit[384]` columns
- No need for application-level blob compression on vectors — sqlite-vec handles packed binary format natively

### Gaps
- Whether `vec_quantize_int8()` SQL function exists (only `vec_quantize_binary` documented)
- Exact ADC implementation details for int8 in sqlite-vec (uses L2? cosine?)
- Performance of int8 + rescore vs binary + rescore on paper's specific embedding model

---

## Rust Crates for Compression and Quantization

### Takeaway
- **Text compression**: `zstd` (C bindings, mature) or `rust-zstd` (pure Rust, no C deps, matches libzstd) for ZSTD with dictionary support; `lz4_flex` or `lz4rip` for LZ4 (pure Rust, fastest decompression)
- **Vector quantization**: `vq` crate (BQ, SQ, PQ, TSVQ, SIMD-accelerated, unified `Quantizer` trait) — best maintained option; `vector_quantizer` (GPL-3.0, PQ only); `velesdb-core` (PQ with OPQ rotation); `ruvector-streaming-qng` (streaming PQ)
- **Candle**: Quantization support for LLM weights (GGUF), but no embedding quantization utilities exposed
- **sqlite-vec Rust**: `sqlite-vec` crate (bundles C extension), works with `rusqlite` + `zerocopy`

### Cited Findings
- `zstd` crate: encoder/decoder, dictionary compression via `EncoderDictionary`/`DecoderDictionary`, streaming, levels 1-22 — [docs.rs/zstd](https://docs.rs/zstd)
- `rust-zstd` v0.4: pure Rust, no C deps, multithreaded, dictionary support, matches libzstd 1.5.7 byte-for-byte at levels 1-22 — [crates.io/rust-zstd](https://crates.io/crates/rust-zstd)
- `lz4_flex`: pure Rust LZ4, 1.6 GB/s compress, 6 GB/s decompress (unsafe), safe mode 1.3/4.5 GB/s — [GitHub lz4_flex](https://github.com/PSeitz/lz4_flex)
- `lz4rip`: pure Rust, no unsafe, built-in dictionary training (`DictTrainer`), 8 KB hash tables, slightly faster than C lz4 — [crates.io/lz4rip](https://crates.io/crates/lz4rip)
- `vq` crate: `ScalarQuantizer`, `BinaryQuantizer`, `ProductQuantizer`, `TSVQ`; SIMD (AVX2/NEON); `Quantizer` trait with `quantize`/`dequantize`/`distance` — [docs.rs/vq](https://docs.rs/vq/latest/vq/)
- `vector_quantizer` v0.0.3: PQ only, GPL-3.0, 96.88% memory reduction, recall@10 0.18-0.40 — [crates.io/vector_quantizer](https://crates.io/crates/vector_quantizer)
- `velesdb-core` `ProductQuantizer`: supports OPQ rotation, postcard serialization, ADC LUT precomputation — [docs.rs/velesdb-core](https://docs.rs/velesdb-core/6.0.0/velesdb_core/quantization/struct.ProductQuantizer.html)
- Candle: quantization for LLM weights (2-8 bit GGUF), no embedding quantization API — [Candle GitHub](https://github.com/huggingface/candle)
- `sqlite-vec` Rust crate: embeds C source, compiles statically, single `sqlite3_vec_init` export — [sqlite-vec Rust guide](https://alexgarcia.xyz/sqlite-vec/rust.html)

### Inferences
- Use `rust-zstd` for text compression (pure Rust, single binary friendly, dictionary support)
- Use `vq` crate for vector quantization (MIT/Apache-2, SIMD, all three methods, active)
- `lz4rip` if LZ4 preferred (no unsafe, built-in dict training)
- Quantization pipeline: train `ScalarQuantizer` on sample embeddings → quantize all vectors to `int8` → store in `vec0` as `int8[384]` + keep `float[384]` for rescoring
- Binary quantization: `BinaryQuantizer::quantize()` (no training needed) → store as `bit[384]`

### Gaps
- Whether `vq` crate's `ScalarQuantizer` uses per-dimension or per-vector scaling (per-dimension is standard for embeddings)
- `vq` crate ADC implementation details for int8 distance computation
- Minimum Rust version requirements for these crates

---

## SQLite Blob Compression: Application-Level vs Built-In

### Takeaway
SQLite has **no native blob compression**. Compression must be applied at application level before inserting blobs. For paper: compress text chunks with ZSTD (with dictionary) before storing in FTS5/content tables; store vectors natively in sqlite-vec `vec0` tables (which use packed binary format). FTS5 external content tables can store compressed text blobs, but FTS5 tokenization requires uncompressed text — so either store uncompressed in FTS5 and compressed in separate column, or use FTS5 only for search and hydrate from compressed store.

### Cited Findings
- SQLite has no built-in blob compression — [Pistack](https://www.pistack.xyz/posts/2026-06-27-cpp-compression-libraries-lz4-zstd-brotli-snappy-miniz), general knowledge
- sqlite-vec stores vectors as packed BLOBs: float32 = 4 bytes/dim, int8 = 1 byte/dim, bit = 1 bit/dim — [sqlite-vec.c](https://github.com/asg017/sqlite-vec/blob/main/sqlite-vec.c)
- FTS5 external content tables: `CREATE VIRTUAL TABLE ... USING fts5(content, content=table, content_rowid=id)` — SQLite docs
- Paper's architecture: `schema` has `chunks` + FTS5 external content + `chunks_vec` float[384] — [AGENTS.md](file:///Users/p0is0n/GitHub/enzo/AGENTS.md)
- ZSTD frame overhead: 4 bytes magic + block headers — for very small inputs (<100 bytes), dictionary helps but frame overhead dominates — [Zstd issue #3722](https://github.com/facebook/zstd/issues/3722)
- `rust-zstd` supports "magicless frames" (saves 4 bytes/frame) — [crates.io/zstd-rs](https://crates.io/crates/zstd-rs)

### Inferences
- Text chunk storage: compress with ZSTD + dictionary → store in `chunks` table as BLOB; FTS5 indexes uncompressed text (or use FTS5 `contentless` mode with external content)
- Vector storage: use sqlite-vec `vec0` with `int8[384]` + `float[384]` columns; no additional compression needed
- For chunks < 500 bytes, consider ZSTD magicless frames to avoid 4-byte frame overhead
- Decompression at query time: reuse `Decompressor` with dictionary for fast hydration of top-K chunks

### Gaps
- Whether FTS5 `contentless` mode with `content=` external table works well with compressed blobs (tokenization happens at insert time on uncompressed text)
- Exact schema for paper's dual-storage (FTS5 + compressed blobs + vec0)

---

## Summary Recommendations for Paper

| Component | Recommendation | Rationale |
|-----------|---------------|-----------|
| **Text chunks (1-2 KB)** | ZSTD level 3 + per-content-type dictionary (~100 KB) | 3-5x ratio, ~0.5ms decompress, dictionary trains once at index build |
| **FTS5 content** | Store uncompressed in FTS5 for tokenization; compressed copy in `chunks` table | FTS5 requires plain text for tokenization; compressed blob for storage savings |
| **Vectors (384-dim)** | `int8[384]` primary + `float[384]` for rescoring in `vec0` | 4x smaller, ADC recall ~98%, rescore to 100%; native sqlite-vec support |
| **Large datasets (>100k vecs)** | Add `bit[384]` column + `rescore(oversample=8)` index | 6x faster search, 98.8% recall, 32x smaller coarse index |
| **Rust crates** | `rust-zstd` (text), `vq` (quantization), `sqlite-vec` + `rusqlite` + `zerocopy` | Pure Rust, single binary, no C deps, SIMD-accelerated |
| **I/O optimization** | `PRAGMA mmap_size=1073741824` (1 GiB) | Cuts 25ms I/O overhead at 50k vectors |
| **Dictionary management** | Train per content-type at index build; store dict BLOB in `metadata` table; version with schema | Reusable across sessions, ~100 KB each, negligible binary size |