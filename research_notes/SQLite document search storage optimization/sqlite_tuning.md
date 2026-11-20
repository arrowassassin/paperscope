# SQLite Document Search Storage Optimization

## Page Size: 4096 vs 8192 vs 16384

### Takeaway
For FTS5 + vec0 workloads with 384-dimension vectors (1.5KB per vector) and text chunks, **8192 or 16384 bytes** is optimal. Default 4096 wastes space on page headers for larger records; 65536 is excessive. Must be set at database creation (before any tables) and requires VACUUM to change later.

### Cited Findings
- SQLite supports page sizes 512–65536 bytes (power of 2). Default is 4096 since 2016 [Devart](https://www.devart.com/blog/increasing-sqlite-performance.html)
- Benchmarks show minimal performance difference on macOS across 4096–16384; larger pages help Windows [Devart](https://www.devart.com/blog/increasing-sqlite-performance.html)
- **DB Pro Blog recommends 8192 or 16384 for SSDs and modern workloads**, "larger pages for read-mostly databases with big rows" [DB Pro](https://www.dbpro.app/blog/how-to-fix-slow-sqlite-queries)
- Fossil SCM uses 512-byte pages for repositories but notes **page_size changes don't work in WAL mode** during rebuild — must switch to DELETE mode first [Fossil Forum](https://fossil-scm.org/forum/forumpost/6fb39432ed)
- Page size must be set **before creating tables**; changing later requires VACUUM [SQLite VACUUM docs](https://sqlite.org/lang_vacuum.html)

### Inferences
- For `paper`'s schema (chunks ~1-4KB text + 384-dim float32 vectors = ~1.5KB), 8192 or 16384 reduces page header overhead (~4 bytes/page) and fits more records per page
- Since `paper` uses WAL mode (required for concurrent read/write), page_size must be chosen at `CREATE DATABASE` time — cannot change via rebuild in WAL
- 16384 may be slightly better for vec0's chunked storage (vectors grouped in fixed-size blocks)

### Gaps
- No direct benchmark comparing 8192 vs 16384 for mixed FTS5 + vec0 workloads
- Exact vec0 internal chunk size unknown — affects optimal page alignment

---

## WAL Mode + mmap_size + cache_size for Read-Heavy Search

### Takeaway
**Essential baseline PRAGMAs per connection:** `journal_mode=WAL`, `synchronous=NORMAL`, `cache_size=-64000` (64MB), `mmap_size=268435456` (256MB+), `temp_store=MEMORY`, `busy_timeout=5000`. WAL enables concurrent readers/writers; mmap eliminates syscalls for reads; large cache keeps working set hot.

### Cited Findings
- **WAL mode required for concurrent read/write** — readers never block writers, writers never block readers [SQLite WAL docs](https://www.sqlite.org/wal.html); [Phiresky](https://phiresky.github.io/blog/2020/sqlite-performance-tuning)
- **synchronous=NORMAL** in WAL: fsyncs at checkpoint only, not every commit — "removes major I/O bottleneck" for read-heavy [Dev.to](https://dev.to/lovestaco/high-performance-sqlite-reads-in-a-go-server-4on3)
- **cache_size=-64000** (64MB per connection) — default is ~2MB, "nowhere near enough for server workloads" [Dev.to](https://dev.to/lovestaco/high-performance-sqlite-reads-in-a-go-server-4on3)
- **mmap_size=268435456** (256MB) or higher — "shines on read-heavy workloads", OS pages in only what's needed [DB Pro](https://www.dbpro.app/blog/how-to-fix-slow-sqlite-queries); [Phiresky uses 30GB](https://phiresky.github.io/blog/2020/sqlite-performance-tuning)
- **temp_store=MEMORY** — avoids disk for temp tables/indices during queries [DB Pro](https://www.dbpro.app/blog/how-to-fix-slow-sqlite-queries)
- **wal_autocheckpoint=10000** — checkpoint every 10k pages (default 1000) to bound WAL file growth [OneUptime](https://oneuptime.com/blog/post/2026-03-02-how-to-optimize-sqlite-performance-on-ubuntu/view)
- **locking_mode=EXCLUSIVE** + WAL works well for single-process multi-threaded apps [Dev.to](https://dev.to/lovestaco/high-performance-sqlite-reads-in-a-go-server-4on3)
- `PRAGMA optimize` updates query planner stats cheaply — run periodically [Coddy](https://coddy.tech/docs/sqlite/pragma-settings)

### Inferences
- These PRAGMAs are **per-connection** (except `journal_mode` which persists) — must apply on every `open()`
- For `paper`'s CLI (short-lived) and MCP server (long-lived), apply at connection init
- mmap_size can be set to multiple GBs on 64-bit — virtual address space reservation, not physical RAM
- `synchronous=NORMAL` is safe in WAL: power loss only loses last uncommitted transaction

### Gaps
- Optimal `mmap_size` for mixed FTS5 + vec0 — vec0 scans may benefit more than FTS5
- Whether `PRAGMA threads=4` helps vec0's SIMD-accelerated distance computation

---

## FTS5: detail=none vs detail=column vs full

### Takeaway
**Use `detail=column` for paper** — enables BM25 ranking and snippet() with ~54% space savings vs `detail=full` (340MB vs 743MB in email corpus test). `detail=none` saves 82% but **breaks ranking and phrase search** — only supports LIKE/GLOB.

### Cited Findings
- **detail=full (default)**: stores rowid + column + offset per term — 743MB for 1.6GB email corpus [SQLite FTS5](https://sqlite.org/fts5.html)
- **detail=column**: stores rowid + column only — 340MB (54% reduction), **supports BM25 ranking and snippet()** [SQLite FTS5](https://sqlite.org/fts5.html)
- **detail=none**: stores rowid only — 134MB (82% reduction), **no ranking, no phrase search, no snippet()** [SQLite FTS5](https://sqlite.org/fts5.html); [Andrew Mara](https://andrewmara.com/blog/faster-sqlite-like-queries-using-fts5-trigram-indexes)
- FTS5 `optimize` command merges segment b-trees into single b-tree — minimum space, fastest query [SQLite FTS5](https://sqlite.org/fts5.html)
- `pgsz` option (default 4050) controls FTS5 segment blob size — can tune for page alignment [SQLite FTS5](https://sqlite.org/fts5.html)

### Inferences
- `paper` needs BM25 ranking for tier0 (FTS5) results → **must use `detail=column` or `detail=full`**
- `detail=column` is sweet spot: keeps ranking, saves ~50% index space
- With external content table (already done), FTS5 doesn't store document text — only inverted index
- `pgsz=4096` or `8192` may align better with database page size

### Gaps
- Exact size difference for `paper`'s corpus (code/docs vs emails)
- Whether `detail=column` affects tier1 (vec0) hybrid search fusion quality

---

## External Content Tables: Already Optimal

### Takeaway
`paper` already uses the optimal pattern: `chunks` table stores text once; FTS5 virtual table with `content='chunks'` indexes it without duplication. No changes needed.

### Cited Findings
- **External content mode** (`content='table'`, `content_rowid='id'`): FTS5 stores only index, retrieves text from source table on demand [SQLite FTS5](https://sqlite.org/fts5.html); [Medium](https://medium.com/@johnidouglasmarangon/full-text-search-in-sqlite-a-practical-guide-80a69c3f42a4)
- **Contentless mode** (`content=''`) saves more space but cannot retrieve text — not suitable for `paper` which needs citations [DB Pro](https://www.dbpro.app/blog/how-to-fix-slow-sqlite-queries)
- Triggers required to keep FTS5 in sync with content table on INSERT/UPDATE/DELETE [Medium](https://medium.com/@johnidouglasmarangon/full-text-search-in-sqlite-a-practical-guide-80a69c3f42a4)
- `UNINDEXED` columns in FTS5 store metadata without growing index [Medium](https://medium.com/@johnidouglasmarangon/full-text-search-in-sqlite-a-practical-guide-80a69c3f42a4)

### Inferences
- Current schema is correct: `chunks` (path, line_start, line_end, text) + FTS5 on text with `content='chunks'`
- Ensure triggers exist for incremental indexing (add/update/delete chunks)
- Consider `UNINDEXED` for path/metadata columns if added to FTS5 table directly

### Gaps
- None — pattern is verified optimal

---

## vec0: Float32 Storage Format & Compression

### Takeaway
`vec0` stores vectors in **chunked internal format** (fixed-size blocks grouped by partition). Supports **float32 (4 bytes/dim), int8 quantization (1 byte/dim, 4x smaller), and bit vectors (1 bit/dim, 32x smaller)**. Quantization functions: `vec_quantize_int8()`, `vec_quantize_binary()`. Float32 = 1536 bytes for 384-dim; int8 = 384 bytes; bit = 48 bytes.

### Cited Findings
- `vec0` virtual table: "chunked internal storage" — vectors grouped into chunks, reduces fragmentation [Alex Garcia](https://alexgarcia.xyz/blog/2024/sqlite-vec-stable-release/index.html)
- **Vector types**: `float` (float32), `float[384]` (fixed-dim), `int8`, `bit` [sqlite-vec README](https://github.com/asg017/sqlite-vec)
- **Quantization**: `vec_quantize_int8(vec_f32(...), 'unit')` scales to [-1,1] preserving cosine; `vec_quantize_binary()` for Hamming [Stephen Collins](https://medium.com/@stephenc211/how-sqlite-vec-works-for-storing-and-querying-vector-embeddings-165adeeeceea)
- **Storage**: `vec_f32('[...]')` packs Python list to compact binary (little-endian floats) [Shiny.NET](https://shinylib.net/efcore/sqlite-vector)
- **No HNSW/IVF** in stable release — flat (exact) index, brute-force scan with SIMD [Shiny.NET](https://shinylib.net/efcore/sqlite-vector); [Alex Garcia](https://alexgarcia.xyz/blog/2024/sqlite-vec-stable-release/index.html)
- **Auxiliary columns** (`+aux`) can store metadata in vec0 but plain table + join is easier to inspect [DEV Community](https://dev.to/gabrielanhaia/rag-in-one-db-file-nodesqlite-plus-sqlite-vec-no-vector-database-2dl2)

### Inferences
- For `paper`'s 384-dim embeddings: float32 = 1.5KB/vector, int8 = 384B (4x), bit = 48B (32x)
- **Recommendation**: Store primary `float[384]` for accuracy; add `int8` or `bit` column for coarse filtering if scale demands
- vec0's chunked storage means page_size=16384 may align better with internal chunk boundaries
- No built-in compression beyond quantization — consider `int8` for tier1 if recall@k acceptable

### Gaps
- Exact vec0 chunk size (not documented) — affects optimal page_size alignment
- Quantization recall@k for 384-dim BERT embeddings (paper's use case)
- Whether `vec0` supports partitioning by `source` (path) for metadata filtering

---

## PRAGMA Settings Summary

### Takeaway
**Persistent (set once at DB creation):** `page_size=16384`, `journal_mode=WAL`, `auto_vacuum=INCREMENTAL`. **Per-connection:** `synchronous=NORMAL`, `cache_size=-64000`, `mmap_size=268435456`, `temp_store=MEMORY`, `busy_timeout=5000`, `wal_autocheckpoint=10000`, `foreign_keys=ON`.

### Cited Findings
- **Persistent PRAGMAs** (stored in DB header): `journal_mode`, `page_size`, `auto_vacuum` [Dev.to](https://dev.to/lovestaco/understanding-sqlite-pragma-and-how-better-sqlite3-makes-it-nicer-1ap0)
- **Per-connection PRAGMAs**: `cache_size`, `mmap_size`, `temp_store`, `foreign_keys`, `synchronous` [Dev.to](https://dev.to/lovestaco/understanding-sqlite-pragma-and-how-better-sqlite3-makes-it-nicer-1ap0)
- `auto_vacuum=INCREMENTAL` requires **VACUUM once after setting** to build metadata; then `PRAGMA incremental_vacuum(N)` reclaims N pages [SQLite auto_vacuum](https://blogs.gnome.org/jnelson/2015/01/06/sqlite-vacuum-and-auto_vacuum); [OneUptime](https://oneuptime.com/blog/post/2026-09-08-reclaim-sqlite-disk-space-vacuum-auto-vacuum/view)
- `PRAGMA optimize` updates query planner stats incrementally — run periodically [Coddy](https://coddy.tech/docs/sqlite/pragma-settings)
- `analysis_limit=1000` speeds up ANALYZE on large DBs [SQLiteCloud](https://docs.sqlitecloud.io/docs/sqlite/lang_analyze)

### Inferences
- `paper` should apply persistent PRAGMAs in `Connection::new()` before any schema creation
- Per-connection PRAGMAs in `Connection::configure()` called on every open
- `auto_vacuum=INCREMENTAL` + periodic `incremental_vacuum(1000)` during idle/maintenance
- `wal_autocheckpoint=10000` bounds WAL file to ~160MB (10k pages × 16KB)

### Gaps
- Whether `PRAGMA mmap_size` persists or is per-connection (conflicting reports)
- Optimal `incremental_vacuum` page count for `paper`'s write pattern

---

## Indexes: Essential vs Bloat

### Takeaway
**Essential indexes for `paper`:** FTS5 internal (auto), vec0 internal (auto), `idx_chunks_path` (unique path lookups), `idx_chunks_source_mtime` (incremental indexing by source+mtime). **Potential bloat:** separate `idx_chunks_source` + `idx_chunks_mtime` if composite covers both. Use `PRAGMA index_list`, `PRAGMA index_info` to audit.

### Cited Findings
- SQLite only has B-trees (plus FTS5/R-Tree extensions) — index strategy similar to other DBs [DB Pro](https://www.dbpro.app/blog/how-to-fix-slow-sqlite-queries)
- `.expert` mode suggests indexes for queries — "starting point, not answer" [DB Pro](https://www.dbpro.app/blog/how-to-fix-slow-sqlite-queries)
- **Composite indexes** cover multiple query patterns; single-column indexes often redundant [DB Pro](https://www.dbpro.app/blog/how-to-fix-slow-sqlite-queries)
- `dbstat` virtual table shows per-table/index disk usage [PowerSync](https://docs.powersync.com/maintenance-ops/client-database-diagnostics)
- FTS5 and vec0 maintain their own internal indexes — no external indexes needed on virtual tables

### Inferences
- `paper`'s current indexes: `idx_chunks_path` (unique), `idx_chunks_source`, `idx_chunks_mtime`
- **Consolidate**: `CREATE INDEX idx_chunks_source_mtime ON chunks(source, mtime)` covers incremental indexing scans
- `idx_chunks_path` needed for `read_source` by chunk_id → path lookup (MCP requirement)
- FTS5/vec0 virtual tables: **no additional indexes** — they manage internal segment b-trees
- Run `ANALYZE` / `PRAGMA optimize` after bulk indexing to update stats

### Gaps
- Actual query plans for `paper`'s search pipeline (tier0 FTS5 → tier1 vec0 → RRF fusion)
- Whether `idx_chunks_source_mtime` is used by incremental indexer's `SELECT ... WHERE source=? AND mtime>?`

---

## Vacuum Strategy: Incremental vs Full

### Takeaway
**Use `auto_vacuum=INCREMENTAL` + scheduled `PRAGMA incremental_vacuum(1000)`** during maintenance windows. Full `VACUUM` only for major schema changes or when freelist > 20% of pages. Incremental avoids long locks; full rewrites entire DB.

### Cited Findings
- **auto_vacuum=NONE** (default): file only grows, deletions leave free pages — must `VACUUM` manually [DB Pro](https://www.dbpro.app/blog/how-to-fix-slow-sqlite-queries)
- **auto_vacuum=FULL**: truncates freelist on every commit — expensive, fragments DB [SQLite auto_vacuum](https://blogs.gnome.org/jnelson/2015/01/06/sqlite-vacuum-and-auto_vacuum)
- **auto_vacuum=INCREMENTAL**: stores reverse-map metadata; `PRAGMA incremental_vacuum(N)` reclaims up to N pages in bounded time [SQLite auto_vacuum](https://blogs.gnome.org/jnelson/2015/01/06/sqlite-vacuum-and-auto_vacuum)
- **Critical**: `PRAGMA auto_vacuum=INCREMENTAL` is **no-op on existing DB** — must `VACUUM` once after setting to rebuild with metadata [OneUptime](https://oneuptime.com/blog/post/2026-09-08-reclaim-sqlite-disk-space-vacuum-auto-vacuum/view)
- **Freelist check**: `PRAGMA freelist_count` × `page_size` = wasted bytes; if large fraction of `page_count`, run vacuum [Coddy](https://coddy.tech/docs/sqlite/analyze-and-vacuum)
- **WAL + auto_vacuum**: truncation happens after checkpoint completes [SQLite Forum](https://sqlite.org/forum/info/6453c201a2837d33167310bea792ac10000a515ccff7dc0a57b6f97dbab7f278)

### Inferences
- `paper` should: set `auto_vacuum=INCREMENTAL` at DB creation, run one `VACUUM` after schema init
- Schedule `incremental_vacuum(1000)` during low-traffic (CLI: on close; MCP: periodic timer)
- Full `VACUUM` only when `freelist_count / page_count > 0.2` or before backup
- `wal_checkpoint(TRUNCATE)` before size measurement merges WAL into main DB [PowerSync](https://docs.powersync.com/maintenance-ops/client-database-diagnostics)

### Gaps
- Optimal `incremental_vacuum` page count for `paper`'s typical delete rate (re-indexing replaces chunks)
- Whether `paper`'s incremental indexing (delete old chunks + insert new) generates enough freelist to matter

---

## Storage Analysis: Tools & Metrics

### Takeaway
**Key diagnostics**: `PRAGMA page_count`, `freelist_count`, `page_size`, `auto_vacuum`; `dbstat` virtual table for per-object breakdown; `sqlite3_analyzer` for detailed report. Check `freelist_count / page_count` ratio to decide on vacuum.

### Cited Findings
- `page_count × page_size` = current file size [Coddy](https://coddy.tech/docs/sqlite/analyze-and-vacuum)
- `freelist_count × page_size` = bytes wasted on unused pages [Coddy](https://coddy.tech/docs/sqlite/analyze-and-vacuum)
- `dbstat` virtual table: low-level physical storage per table/index (includes B-tree overhead, page headers, free space) [PowerSync](https://docs.powersync.com/maintenance-ops/client-database-diagnostics)
- `sqlite3_analyzer` tool: comprehensive DB stats (page usage, btree depth, payload vs overhead) [SQLite Analyzer](https://sqlite.org/analyzer.html)
- `PRAGMA integrity_check`, `foreign_key_check` after vacuum [OneUptime](https://oneuptime.com/blog/post/2026-09-08-reclaim-sqlite-disk-space-vacuum-auto-vacuum/view)

### Inferences
- Add `paper stats` command: runs `PRAGMA page_count`, `freelist_count`, `page_size`, `auto_vacuum`; queries `dbstat` for chunks/fts/vec0 breakdown
- Monitor `freelist_ratio = freelist_count / page_count` — trigger incremental_vacuum if > 0.1
- `dbstat` shows FTS5 internal tables (`chunks_fts_data`, `chunks_fts_idx`, `chunks_fts_content`) and vec0 storage separately

### Gaps
- `sqlite3_analyzer` output format parsing for programmatic use
- Whether `dbstat` includes WAL file pages or only main DB

---

## Fossil SCM Patterns (Reference)

### Takeaway
Fossil uses SQLite heavily (single-file repos, 75-85% artifacts compressed with zlib+delta). Uses **512-byte pages** for repos but **switches to 8192 for WAL mode** during rebuild. Employs aggressive compression (zlib + delta) — not directly applicable to `paper` but shows SQLite scales to GBs.

### Cited Findings
- Fossil stores artifacts as zlib-compressed BLOBs with delta compression — 80:1 compression ratio [Fossil Tech Overview](https://fossil-scm.org/home/doc/tip/www/tech_overview.wiki)
- **Page size 512 for repositories**, but rebuild to 8192 requires switching out of WAL mode first [Fossil Forum](https://fossil-scm.org/forum/forumpost/6fb39432ed)
- SQLite used in "stupidly huge databases" (750GB mentioned) [Fossil Forum](https://www.fossil-scm.org/forum/forumpost/62e8a1595aeb03d1)
- Fossil's `blob` table is core storage — one row per artifact/file version [Fossil Not Relational](https://fossil-scm.org/home/doc/trunk/www/fossil-is-not-relational.md)

### Inferences
- Fossil's 512-byte pages optimize for many small artifacts; `paper` has fewer, larger records → larger pages better
- Fossil's delta compression not applicable to `paper` (chunks are independent)
- Confirms SQLite + WAL handles GB-scale single files reliably

### Gaps
- Fossil's exact PRAGMA config (not publicly documented in detail)

---

## Consolidated Recommendations for `paper`

### At Database Creation (`Connection::new()`)
```sql
PRAGMA page_size = 16384;
PRAGMA journal_mode = WAL;
PRAGMA auto_vacuum = INCREMENTAL;
-- Create schema...
VACUUM;  -- Build auto_vacuum metadata
```

### Per Connection (`Connection::configure()`)
```sql
PRAGMA synchronous = NORMAL;
PRAGMA cache_size = -64000;        -- 64MB
PRAGMA mmap_size = 268435456;      -- 256MB+
PRAGMA temp_store = MEMORY;
PRAGMA busy_timeout = 5000;
PRAGMA wal_autocheckpoint = 10000;
PRAGMA foreign_keys = ON;
```

### FTS5 Table
```sql
CREATE VIRTUAL TABLE chunks_fts USING fts5(
    text,
    content='chunks',
    content_rowid='id',
    detail='column',        -- BM25 + snippet, ~50% space vs full
    tokenize='porter unicode61',
    pgsz=4096               -- Align with page_size/4
);
```

### vec0 Table
```sql
CREATE VIRTUAL TABLE chunks_vec USING vec0(
    embedding float[384],
    distance_metric=cosine
);
-- Optional quantized column for coarse filter:
-- embedding_int8 int8[384],
```

### Indexes
```sql
CREATE UNIQUE INDEX idx_chunks_path ON chunks(path);
CREATE INDEX idx_chunks_source_mtime ON chunks(source, mtime);
-- Drop idx_chunks_source, idx_chunks_mtime if composite covers queries
```

### Maintenance
- **On close / idle**: `PRAGMA incremental_vacuum(1000); PRAGMA optimize;`
- **Weekly / before backup**: Check `freelist_count / page_count`; full `VACUUM` if > 0.2
- **After bulk indexing**: `INSERT INTO chunks_fts(chunks_fts) VALUES('optimize');`

### Monitoring Command
```sql
-- paper stats
PRAGMA page_count;
PRAGMA freelist_count;
PRAGMA page_size;
PRAGMA auto_vacuum;
SELECT name, page_count, avg_payload FROM dbstat WHERE name IN ('chunks', 'chunks_fts_data', 'chunks_fts_idx', 'chunks_vec');
```

---

## Source Summary

| Topic | Key Sources |
|-------|-------------|
| Page size | Devart, DB Pro, Fossil Forum, SQLite VACUUM docs |
| WAL/mmap/cache | SQLite WAL, Phiresky, DB Pro, Dev.to, OneUptime, Coddy |
| FTS5 detail | SQLite FTS5, Andrew Mara, SQLite Forum, Darksi.de |
| External content | SQLite FTS5, Medium, DB Pro |
| vec0 storage | Alex Garcia, Stephen Collins, Shiny.NET, sqlite-vec README |
| PRAGMAs | Dev.to, SQLite Pragma docs, Coddy, SQLiteCloud |
| Indexes | DB Pro, PowerSync, SQLite Expert mode |
| Vacuum | Gnome blogs, OneUptime, SQLite Forum, Coddy |
| Storage analysis | Coddy, PowerSync, SQLite Analyzer |
| Fossil patterns | Fossil Tech Overview, Fossil Forum, Fossil Not Relational |