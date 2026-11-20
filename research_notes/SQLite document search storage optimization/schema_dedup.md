# SQLite Document Search Storage Optimization — Schema Deduplication Design

## Content-Addressable Storage with `content_hash` (BLAKE3) as Primary Key

### Takeaway
Using BLAKE3 content hashes as primary keys for a deduplicated `content` table is a proven pattern (used by qmd, restic, borg, Git). The `chunks` table becomes a citation/occurrence table referencing content by hash. FTS5 external-content tables **require** an integer `rowid`/`content_rowid` — they cannot directly use a BLAKE3 hash as the rowid. The workaround: keep `chunks.id INTEGER PRIMARY KEY` as the FTS5/vec0 key, add `content_hash BLOB NOT NULL` FK to a new `content` table, and use triggers to sync FTS5.

### Cited Findings
- **qmd's content-addressable schema**: `content(hash TEXT PRIMARY KEY, doc TEXT, created_at)` + `documents(id INTEGER PK, hash TEXT FK REFERENCES content(hash))` with FTS5 `documents_fts` external-content over `documents` (not `content`) — triggers join to `content` to fetch `doc` for indexing. [qmd-book Chapter 4](https://qmd-book.myhubs.dev/chapters/04-schema-design.html); [QMD Indexing Flow](https://mintlify.wiki/tobi/qmd/architecture/indexing.md)
- **SQLite FTS5 `content_rowid` constraint**: Must be an INTEGER PRIMARY KEY column (or implicit `rowid`). TEXT primary keys cause "database disk image is malformed" at query time. [SQLite Forum: content_rowid must be integer](https://sqlite.org/forum/info/4061e0d705d7bf40); [SQLite Forum: external content with TEXT PK fails](https://sqlite.org/forum/info/32aceaec2c29a351)
- **FTS5 external-content query pattern**: `SELECT <content_rowid>, <cols> FROM <content> WHERE <content_rowid> = ?` — FTS5 binds the matched rowid and expects integer lookup. [SQLite FTS5 docs §4.4.3](https://sqlite.org/fts5.html#external_content_tables)
- **BLAKE3 for CAS in SQL**: Zenodo paper "Content-Addressed Storage with BLAKE3 for SQL Database Value Deduplication" (2026) describes CAS column attribute `STORAGE CONTENT_ADDRESSED` with BLAKE3, threshold 1024 bytes, transparent on INSERT/UPDATE/SELECT. [Zenodo record 19244285](https://zenodo.org/records/19244285)
- **restic/borg chunk deduplication**: Content-defined chunking (CDC) with rolling hash (Rabin/buzhash/Gear), chunk hash = BLAKE3/SHA-256, reference counting for GC. [restic CDC design](https://restic.net/blog/2015-09-12/restic-foundation1-cdc); [Borg chunkers](https://borgbackup.readthedocs.io/en/latest/internals/chunker.html)

### Inferences
- The `content` table should use `hash BLOB PRIMARY KEY` (32-byte BLAKE3) — not TEXT — for storage efficiency and exact binary comparison.
- `chunks` retains `id INTEGER PRIMARY KEY` as the stable rowid for FTS5 (`content_rowid='id'`) and `vec0` (rowid = chunks.id).
- `chunks.content_hash BLOB NOT NULL REFERENCES content(hash)` enables deduplication: `INSERT OR IGNORE INTO content(hash, doc) VALUES (?, ?)` then insert chunk rows pointing to that hash.
- FTS5 triggers on `chunks` continue to work unchanged; they index `fts_text` from `chunks`, not from `content`. The `content` table is only for deduplication storage, not FTS5 content source.
- `vec0` primary key remains `rowid` (== `chunks.id`). One vector per **chunk occurrence**, not per unique content — this avoids the qmd issue #188 where `hash_seq` collisions occurred when same content appeared at different paths.

### Gaps
- No direct benchmark comparing "one vector per unique content" vs "one vector per chunk occurrence" for retrieval quality when same paragraph appears in 5 PDFs with different surrounding context.

---

## Same Paragraph in 5 PDFs → One Content Row, 5 Citation Rows

### Takeaway
This is the standard content-addressable pattern: `content` table stores unique text once; `chunks` (or `documents` in qmd) stores 5 rows with `content_hash` FK, each carrying file-specific citation metadata (path, page, line_start, line_end, anchor, section). FTS5 and vec0 index each occurrence separately so citations resolve to the correct location.

### Cited Findings
- **qmd design**: "When the same content appears at multiple paths… step 2 [INSERT OR IGNORE INTO content] won't produce duplicate writes, step 5 [embeddings] also won't recalculate… storage space and LLM API calls both saved." But qmd's `vectors_vec` key is `hash_seq` (content-hash based), causing UNIQUE constraint failures when same content at different paths. [qmd issue #188](https://github.com/tobi/qmd/issues/188)
- **OpenClaw schema**: `chunks` table has `hash TEXT` for deduplication via `embedding_cache(hash PK, vector)` — identical content shares embedding. [OpenClaw Database Schema](https://altinum.github.io/docfx_softeng/softeng2/openclaw/openclaw_database.html)
- **CodeGraph schema**: `files(content_hash TEXT NOT NULL)` + `chunks` with `file_id FK` — deduplication at file level, not chunk level. [CodeGraph Data Model](https://firlab.app/codegraph/en/dev/data-model)
- **restic/borg**: "Identical chunks across files, machines, snapshots stored once. Snapshot = list of chunk refs." [restic vs Borg](https://linuxdork.com/blog/restic-vs-borg-backup)

### Inferences
- **Citation metadata lives in `chunks`**: `path`, `source_path`, `page`, `line_start`, `line_end`, `anchor`, `section` — all per-occurrence. The `content` table holds only `hash` + `doc` (raw text).
- **FTS5 indexes `chunks.fts_text` per occurrence** (via triggers on `chunks`). This means the same paragraph indexed 5 times in FTS5 — acceptable because FTS5 index is compact (postings only). Snippet/highlight pulls from `chunks.content` via `content='chunks'`.
- **vec0 indexes each chunk occurrence** (rowid = `chunks.id`). Same paragraph → 5 vectors. Slightly larger vector index but correct citation semantics and no UNIQUE collisions.
- **Embedding deduplication can be optional**: If same content → same embedding, cache at application level (`embedder.embed()` checks `content_hash` in a local map or `embedding_cache` table). Not required at schema level.

### Gaps
- Whether FTS5 posting list bloat from 5x duplicate paragraphs is measurable at scale (100k+ docs). qmd accepts this tradeoff; restic/borg accept it at chunk level.

---

## FTS5 with Content-Addressable: Can Virtual Table Join to Content Table by Hash?

### Takeaway
**No — FTS5 `content=` option cannot join on a hash column.** It requires `content_rowid` to be an integer column (explicit INTEGER PRIMARY KEY or implicit `rowid`). The content table for FTS5 must be the occurrence table (`chunks`), not the deduplicated `content` table. FTS5 reads columns via `SELECT content_rowid, cols FROM content_table WHERE content_rowid = ?`.

### Cited Findings
- **SQLite FTS5 docs**: "An external content FTS5 table is created by setting the content option to the name of a table… Whenever column values are required by FTS5, it queries the content table as follows: `SELECT <content_rowid>, <cols> FROM <content> WHERE <content_rowid> = ?`" [SQLite FTS5 §4.4.3](https://sqlite.org/fts5.html#external_content_tables)
- **SQLite Forum**: `content_rowid` "does not have to be of type 'integer', but it must contain only integer values." TEXT PK causes corruption. [SQLite Forum 4061e0d705d7bf40](https://sqlite.org/forum/info/4061e0d705d7bf40)
- **qmd workaround**: FTS5 `documents_fts` has `content='documents'` (integer `id` PK), triggers `SELECT c.doc FROM content c WHERE c.hash = new.hash` to fetch text for indexing. [qmd-book Chapter 4](https://qmd-book.myhubs.dev/chapters/04-schema-design.html)
- **Paper current schema**: `chunks_fts` uses `content='chunks', content_rowid='id'` — already correct for FTS5.

### Inferences
- Keep `chunks_fts` as external-content over `chunks` (current design). No change needed.
- The `content` table is invisible to FTS5; it only serves deduplication storage and embedding cache.
- If you want FTS5 to read from `content` directly, you'd need a view with integer rowid mapping to hash — adds complexity, no benefit.

### Gaps
- None — this is a hard SQLite constraint, well documented.

---

## Tradeoff: Deduplication Saves Space but Complicates Citation

### Takeaway
Deduplication at the `content` table level is orthogonal to citation. Citation metadata (file, page, line, anchor) lives in `chunks` — one row per occurrence. The `content` table has no citation info. This is the correct separation of concerns: **content = what the text says; chunks = where it appears**.

### Cited Findings
- **qmd**: `content` table = deduplicated text; `documents` table = path, title, collection, hash FK. "By storing content separately from metadata, QMD ensures identical files across different paths or collections only consume storage once." [QMD Database Schema](https://instagit.com/tobi/qmd/database-schema-qmd-index.md)
- **OpenClaw**: `chunks` has `path, start_line, end_line, hash`; `embedding_cache(hash PK, vector)` deduplicates embeddings. [OpenClaw Database Schema](https://altinum.github.io/docfx_softeng/softeng2/openclaw/openclaw_database.html)
- **CodeGraph**: `files(path PK, content_hash)` + `chunks(file_id FK)` — deduplication at file granularity. [CodeGraph Data Model](https://firlab.app/codegraph/en/dev/data-model)
- **restic/borg**: Snapshot = manifest of chunk hashes; chunk metadata (size, hash) separate from file metadata (path, mode, chunk list). [restic design](https://github.com/restic/restic/blob/master/doc/design.rst); [Borg internals](https://borgbackup.readthedocs.io/en/latest/internals/chunker.html)

### Inferences
- **No citation complication**: `chunks` already has all citation fields. Adding `content_hash FK` doesn't change that.
- **Space savings**: For boilerplate (licenses, headers, repeated paragraphs), `content` table stores once. `chunks` rows are small (metadata + 32-byte hash).
- **FTS5 index size**: Unchanged — still indexes `chunks.fts_text` per occurrence. FTS5 postings are compact.
- **vec0 index size**: Grows with chunk occurrences, not unique content. Acceptable tradeoff for correct citations.

### Gaps
- Quantitative space savings for typical document corpora (legal, technical, academic) with repeated boilerplate.

---

## Anchor/Section Metadata: Per Occurrence vs Per Unique Content

### Takeaway
**Anchor/section must be per occurrence (`chunks` table).** The same paragraph in different documents has different anchors (page 3 vs page 7, slide 2 vs slide 5, Sheet1!A1 vs Sheet2!A1) and different section paths (Warranty/Coverage vs Terms/Liability). Storing them in `content` table would lose citation accuracy.

### Cited Findings
- **Paper indexer**: `Chunk` struct has `page, line_start, line_end, anchor, section` — all from source document structure. "Blocks are never merged across anchors or sections, so a citation can always name the exact page/cell/slide and the section it belongs to." [paper indexer.rs lines 37-49, 399-429](file:///Users/p0is0n/GitHub/enzo/src/core/index/indexer.rs)
- **qmd**: `documents` table has `path, title` per occurrence; `content` table has only `doc` (full text). No anchor/section in `content`.
- **OpenClaw**: `chunks` has `path, start_line, end_line`; `embedding_cache` has only `hash, vector`.

### Inferences
- `content` table: `hash BLOB PK, doc TEXT, created_at` — **no** anchor, section, page, line numbers.
- `chunks` table: retains all citation fields + `content_hash BLOB FK`.
- `fts_text` generation in indexer combines `section + '\n' + content` per chunk occurrence — correct because section differs per occurrence.

### Gaps
- None — clear from existing Paper architecture.

---

## Reference Counting for Garbage Collection

### Takeaway
Reference counting is the standard approach for CAS garbage collection. Maintain `ref_count` on `content` table (or a separate `content_refs` table), increment on `INSERT` into `chunks`, decrement on `DELETE` from `chunks`. When `ref_count = 0`, content row and its vectors can be deleted. Batch/offline GC is acceptable for a local-first tool.

### Cited Findings
- **restic/borg**: "Reference counts incremented on write, decremented on delete. GC runs periodically." [restic design](https://github.com/restic/restic/blob/master/doc/design.rst)
- **Google Patent 9,747,320**: "Efficient reference counting in content addressable storage" — reference table with (content_hash, object_id), decrement on delete, GC when count reaches zero. [US Patent 9747320](https://patents.justia.com/patent/9747320)
- **Marionette (chunlea/marionette)**: `UPDATE chunks SET ref_count = ref_count - 1 WHERE hash = ?; DELETE FROM chunks WHERE ref_count = 0 AND deleted_at < NOW() - INTERVAL '7 days'` — two-phase with grace period. [Marionette storage.md](https://github.com/chunlea/marionette/blob/main/docs/reference/storage.md)
- **Content-addressed-store (Python)**: "Garbage Collection: Reference-counted cleanup of unreferenced chunks." [PyPI content-addressed-store](https://pypi.org/project/content-addressed-store)
- **Academic**: "Focused Garbage Collection (FGC)… Austere Reference Graph with low memory cost… 1.3×-7.34× faster Mark phase vs Mark&Sweep." [FGC paper](https://www.computer.org/csdl/proceedings-article/iccd/2022/618600a315/1JeG6q7f5rG)

### Inferences
- Add `ref_count INTEGER NOT NULL DEFAULT 1` to `content` table.
- On `INSERT INTO chunks`: `UPDATE content SET ref_count = ref_count + 1 WHERE hash = ?` (or `INSERT ... ON CONFLICT(hash) DO UPDATE SET ref_count = ref_count + 1`).
- On `DELETE FROM chunks`: `UPDATE content SET ref_count = ref_count - 1 WHERE hash = ?`; then `DELETE FROM content WHERE ref_count = 0`.
- `chunks_vec` rows deleted by FK cascade or trigger when `chunks` row deleted.
- For Paper's local-first single-user model, immediate ref-count update in same transaction is fine — no concurrent writer complexity.

### Gaps
- Whether to use a separate `content_refs(content_hash, chunk_id)` table for auditability vs single `ref_count` integer. Single integer is simpler and sufficient for GC.

---

## Versioning: Extractor Upgrade Changes Text Slightly → New Hash → Old Content Retained?

### Takeaway
Yes — content hash changes when extraction changes (even whitespace). Old `content` rows become unreferenced (`ref_count = 0`) and are GC'd. Paper already handles this via `INDEXER_VERSION` in `index_meta` — on version mismatch, it wipes `chunks` and `chunks_vec` entirely (`invalidate_on_upgrade`). With deduplication, old `content` rows linger until GC runs.

### Cited Findings
- **Paper `invalidate_on_upgrade`**: "Bump when extraction, chunking or the FTS projection changes meaning. A mismatch invalidates every stored chunk… A full reindex is the honest outcome; it happens once per upgrade." [paper indexer.rs lines 51-54, 291-318](file:///Users/p0is0n/GitHub/enzo/src/core/index/indexer.rs)
- **qmd**: No explicit versioning in schema; relies on re-embedding when model changes.
- **restic/borg**: Repository format version in config; chunking algorithm fixed per repo. Upgrade = new repo.
- **Zenodo CAS paper**: "Copy-on-write branches share single physical copy" — implies versioning at branch level, not content level.

### Inferences
- Keep `INDEXER_VERSION` mechanism — it correctly forces full reindex on extractor/chunker change.
- After reindex, old `content` rows have `ref_count = 0` (no `chunks` point to them). GC cleans them up.
- No need for explicit version column in `content` table — hash is the version.
- If user wants to keep old extractions (e.g., for audit), they can disable GC or export before upgrade.

### Gaps
- GC trigger timing: immediate after reindex? Background? CLI command `paper gc`?

---

## SQL Patterns: `INSERT OR IGNORE INTO content`, `chunks` Has `content_hash` FK

### Takeaway
Standard upsert pattern for deduplication:
```sql
INSERT OR IGNORE INTO content (hash, doc, created_at, ref_count) VALUES (?, ?, ?, 1);
INSERT INTO chunks (..., content_hash, ...) VALUES (..., ?, ...);
UPDATE content SET ref_count = ref_count + 1 WHERE hash = ?;
```
Or combine with `ON CONFLICT`:
```sql
INSERT INTO content (hash, doc, created_at, ref_count) VALUES (?, ?, ?, 1)
ON CONFLICT(hash) DO UPDATE SET ref_count = ref_count + 1;
```

### Cited Findings
- **qmd**: `INSERT OR IGNORE INTO content (hash, doc, created_at) VALUES (?, ?, ?)` — silent skip on duplicate. [qmd Indexing Flow](https://mintlify.wiki/tobi/qmd/architecture/indexing.md)
- **SQLite `ON CONFLICT`**: Supports `DO UPDATE SET` for upsert. [SQLite UPSERT docs](https://sqlite.org/lang_UPSERT.html)
- **Paper indexer**: Already computes `blake3::hash(&bytes)` per file, stores in `chunks.content_hash`. [paper indexer.rs lines 160, 229](file:///Users/p0is0n/GitHub/enzo/src/core/index/indexer.rs)

### Inferences
- Use `INSERT INTO content (hash, doc, created_at, ref_count) VALUES (?, ?, ?, 1) ON CONFLICT(hash) DO UPDATE SET ref_count = ref_count + 1` — single statement, atomic.
- `chunks.content_hash` is `BLOB NOT NULL` (32 bytes), FK to `content(hash)`.
- `chunks` retains `id INTEGER PRIMARY KEY` for FTS5/vec0.
- Transaction wraps content insert + chunk inserts + vec0 inserts.

### Gaps
- Whether to store `doc` (full text) in `content` or keep `content` in `chunks` only. qmd stores full doc in `content`; Paper currently stores per-chunk `content` in `chunks`. For chunk-level deduplication, `content` table should store chunk text, not file text.

---

## Impact on `chunks_vec`: One Vector Per Unique Content or Per Chunk?

### Takeaway
**One vector per chunk occurrence (per `chunks` row), not per unique content.** This avoids the qmd issue #188 (UNIQUE constraint failure on `hash_seq` when same content at different paths). Citation correctness requires per-occurrence vectors. Embedding computation can be deduplicated at application level via hash cache.

### Cited Findings
- **qmd issue #188**: `vectors_vec` PK = `hash_seq` (content-hash based). "Multiple documents with identical content share the same hash… UNIQUE error when same content at different paths." Fix proposed: make vector PK document-scoped (`doc_id_seq`). [qmd issue #188](https://github.com/tobi/qmd/issues/188)
- **Paper current schema**: `chunks_vec` is `vec0(embedding float[384])` with implicit `rowid` = `chunks.id`. One vector per chunk. [paper schema.rs lines 57-59](file:///Users/p0is0n/GitHub/enzo/src/core/index/schema.rs)
- **OpenClaw**: `embedding_cache(hash PK, vector)` deduplicates embeddings; `chunks_vec` (vec0) has `id FK` to `chunks`. [OpenClaw Database Schema](https://altinum.github.io/docfx_softeng/softeng2/openclaw/openclaw_database.html)
- **sqlite-vec vec0**: PK is `rowid` (integer) or explicit PK column. Supports `partition key` for sharding. [sqlite-vec vec0 docs](https://alexgarcia.xyz/sqlite-vec/features/vec0)

### Inferences
- Keep `chunks_vec` as `vec0(embedding float[384])` with `rowid` = `chunks.id` — no schema change.
- Add optional `embedding_cache(hash BLOB PRIMARY KEY, embedding BLOB, model TEXT, created_at)` table for embedding deduplication at application level.
- Indexer: before `embedder.embed()`, check `embedding_cache` for `content_hash`; if hit, reuse vector; else compute, store in cache, insert into `chunks_vec`.
- This preserves citation correctness (one vec0 row per chunk) while avoiding redundant embedding computation.

### Gaps
- Embedding cache invalidation when model changes — need `model` column in cache, or rely on `INDEXER_VERSION` wipe.

---

## Proposed Schema Migration (v2 → v3)

```sql
-- New content table for deduplicated chunk storage
CREATE TABLE IF NOT EXISTS content (
    hash        BLOB PRIMARY KEY,           -- BLAKE3 32 bytes
    doc         TEXT NOT NULL,              -- chunk text (original bytes)
    created_at  INTEGER NOT NULL,           -- unix ms
    ref_count   INTEGER NOT NULL DEFAULT 1  -- GC reference count
);

-- Embedding cache (optional, avoids recompute)
CREATE TABLE IF NOT EXISTS embedding_cache (
    hash        BLOB PRIMARY KEY,           -- content hash
    model       TEXT NOT NULL,              -- embedding model id
    embedding   BLOB NOT NULL,              -- serialized f32[384]
    created_at  INTEGER NOT NULL,
    FOREIGN KEY (hash) REFERENCES content(hash) ON DELETE CASCADE
);

-- chunks: add content_hash FK (already exists as BLOB), keep id INTEGER PK
-- ALTER TABLE chunks ADD COLUMN content_hash BLOB NOT NULL DEFAULT ''; -- already there
-- CREATE INDEX idx_chunks_content_hash ON chunks(content_hash);

-- FTS5 unchanged: content='chunks', content_rowid='id'
-- vec0 unchanged: rowid = chunks.id

-- Triggers for ref_count maintenance
CREATE TRIGGER chunks_ai_ref AFTER INSERT ON chunks BEGIN
    UPDATE content SET ref_count = ref_count + 1 WHERE hash = new.content_hash;
END;
CREATE TRIGGER chunks_ad_ref AFTER DELETE ON chunks BEGIN
    UPDATE content SET ref_count = ref_count - 1 WHERE hash = old.content_hash;
    DELETE FROM content WHERE ref_count = 0;
END;

-- GC command (manual or periodic)
-- DELETE FROM content WHERE ref_count = 0;
-- DELETE FROM embedding_cache WHERE hash NOT IN (SELECT hash FROM content);
```

### Upgrade Path from Current v2
1. `CREATE TABLE content ...`
2. `INSERT INTO content (hash, doc, created_at, ref_count) SELECT DISTINCT content_hash, content, mtime, 1 FROM chunks;` (or compute per-hash ref_count via subquery)
3. `CREATE TABLE embedding_cache ...`
4. Add ref_count triggers.
5. Bump `SCHEMA_VERSION` to 3.
6. `invalidate_on_upgrade` will wipe and reindex cleanly on next run.