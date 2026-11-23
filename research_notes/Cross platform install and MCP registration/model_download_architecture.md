# Install-time provisioning of ML model weights in a Rust application

Scope: Paper (Rust 2021, Candle, single binary, CLI + MCP stdio server) which needs a 134 MB embedder, an optional 487 MB reranker and small OCR weights from the Hugging Face Hub. Current behaviour is lazy download inside the daemon on first run. Desired behaviour: start the embedder download immediately after install, begin auto-indexing the moment the embedder is usable, and pull the reranker concurrently.

---

## Should we use `hf-hub` or a custom client?

### Takeaway

`hf-hub` 1.0 is a good *metadata and cache* client but it is **not a resumable downloader**: it re-downloads a file from byte 0 after any interruption, never hashes the bytes it writes, and does no disk-space preflight. Because Paper already hash-pins every model file, a thin custom downloader built on `reqwest` + `sha2` is the correct choice; `hf-hub` remains useful for repo metadata (`info`, `list_tree`, revision resolution) if we want to avoid hand-rolling JSON.

### Cited Findings

- `hf-hub` 1.0.0 (published 19 Nov 2026) is an async Rust client for the HF Hub API with typed repo handles, an optional `blocking` API, a `socks` feature, built-in retries on transient failures, Xet transfers via the `hf-xet` dependency, and a `progress` module for upload/download reporting — [docs.rs/hf-hub](https://docs.rs/hf-hub/latest/hf_hub/)
- Client configuration is resolved from `HF_TOKEN`, `HF_TOKEN_PATH`, `HF_ENDPOINT`, `HF_HOME`, `HF_HUB_CACHE`, `HF_HUB_DISABLE_IMPLICIT_TOKEN`, or explicitly via `HFClient::builder().token().endpoint().cache_dir()` — [docs.rs/hf-hub](https://docs.rs/hf-hub/latest/hf_hub/)
- Its cache is content-addressed under `HF_HUB_CACHE` with **on-disk locking so concurrent fetches of the same file deduplicate**, and errors are typed enough to match on (`RepoNotFound`, `EntryNotFound`, `AuthRequired`, `Forbidden`, `RateLimited`) — [docs.rs/hf-hub](https://docs.rs/hf-hub/latest/hf_hub/)
- `snapshot_download` fans out across files with `buffer_unordered(max_workers)` where **`max_workers` defaults to 8**, and runs Xet batch downloads concurrently with non-Xet files via `tokio::try_join!` — [hf-hub `repository/download.rs`](https://raw.githubusercontent.com/huggingface/hf-hub/main/hf-hub/src/repository/download.rs)
- **No resume:** the single-file path does a plain `GET`, then `File::create(dest)` (which truncates) on `<blob>.<etag>.incomplete`, streams the body in, and `std::fs::rename`s it into the blob path. There is no `Range` header on the download GET, only a conditional `HEAD` (`If-None-Match`) used to reuse a complete cached file via `304 Not Modified` — [hf-hub `repository/download.rs`](https://raw.githubusercontent.com/huggingface/hf-hub/main/hf-hub/src/repository/download.rs)
- **No content verification:** `stream_response_to_file_with_progress` writes chunks and flushes; the only integrity signal is the ETag-derived blob filename and `Content-Length`/`X-Linked-Size` — [hf-hub `repository/download.rs`](https://raw.githubusercontent.com/huggingface/hf-hub/main/hf-hub/src/repository/download.rs)
- Retry policy is 5 attempts, base 100 ms, exponential `2B, 4B, …` with `jitter` multiplying each delay by a random factor in `[0,1)` (jitter only shortens), retrying 408/429/500/502/503/504 plus connect and timeout errors — [hf-hub `retry.rs`](https://raw.githubusercontent.com/huggingface/hf-hub/main/hf-hub/src/retry.rs)
- The Rust client does **not** expose per-file expected digests, a global bandwidth cap, a disk-space check, or a resume offset; progress is emitted as in-process callbacks (`DownloadEvent::{Start,Progress,Complete}`), not persisted — [hf-hub `repository/download.rs`](https://raw.githubusercontent.com/huggingface/hf-hub/main/hf-hub/src/repository/download.rs); [docs.rs/hf-hub](https://docs.rs/hf-hub/latest/hf_hub/)
- Python `huggingface_hub` (the reference implementation) *does* resume: `http_get` accepts `resume_size`, sets `Range`, retries up to 5 times resuming from `new_resume_size`, and compares final size against `expected_size` — [huggingface_hub `file_download.py`](https://raw.githubusercontent.com/huggingface/huggingface_hub/main/src/huggingface_hub/file_download.py)

### Inferences

- Adopting `hf-hub` for downloads would silently violate Paper's invariant that every model file is hash-pinned and verified: the crate gives us an ETag-named blob, not a checked digest. We would still have to add our own hashing pass, and we would still lose resume.
- The useful parts of `hf-hub` for Paper are small: `HF_ENDPOINT` handling, auth headers, revision→commit resolution, `list_tree`. Those are ~200 lines against the Hub's JSON API; the 75 KB downloader module is the part we do not want.
- If we do keep `hf-hub` as a dependency for metadata only, we should call `.local_dir(...)`/streaming APIs and never its cache-download path, or the crate will create a second, unverified copy of the weights on disk.

### Gaps

- `hf-hub` 1.0.0's changelog was not read; whether resume is planned for 1.x is unknown.
- No source found describing `hf-hub`'s behaviour on `429` with `Retry-After` (it classifies `RateLimited`, but whether the delay is honoured is unverified).

---

## Download mechanics: streaming to `.part`, Range resume, If-Range, atomic rename, disk preflight, and servers that ignore Range

### Takeaway

Use one `.part` file per artifact, journal the validator (`ETag`, `Last-Modified`), total size and expected SHA-256 in a sidecar, resume with `Range` **plus `If-Range`**, treat any non-206 answer to a resume request as "start over", hash the finished file, `fsync`, and only then `rename` into place. Preflight free space on the destination filesystem before the first byte.

### Cited Findings

- `If-Range` makes a range request conditional: if the validator matches, the server returns `206 Partial Content` for the requested range; **if it does not match, the server returns the full resource with `200 OK`**. It accepts either `Last-Modified` or `ETag` (not both), and a weak validator (`W/`) **must not** be used — [MDN: If-Range](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/If-Range)
- Python's client handles the "server ignored Range" case explicitly: if it sent a Range and got `200`, it rewinds and truncates the temp file, zeroes `resume_size`, and re-downloads from scratch to avoid corrupting the file. A comment notes this happens "e.g. CloudFront with `Accept-Encoding: gzip`" — [huggingface_hub `file_download.py`](https://raw.githubusercontent.com/huggingface/huggingface_hub/main/src/huggingface_hub/file_download.py)
- Python sets `Accept-Encoding: identity` on metadata calls specifically so the reported size is the real file size rather than a compressed length — [huggingface_hub `file_download.py`](https://raw.githubusercontent.com/huggingface/huggingface_hub/main/src/huggingface_hub/file_download.py)
- Python's failure mode for size mismatch is a hard error telling the user to retry with `force_download=True` — i.e. size, not hash, is the consistency check in the HTTP path — [huggingface_hub `file_download.py`](https://raw.githubusercontent.com/huggingface/huggingface_hub/main/src/huggingface_hub/file_download.py)
- Python stores the partial as `<blob>.<etag>.incomplete` **under the same cache lock** as the final file, notes that "a `<etag>.incomplete` file corrupts the cache whenever the surrounding lock is not honored", and uses a UUID-suffixed temp name when the caller may be racing — [huggingface_hub `file_download.py`](https://raw.githubusercontent.com/huggingface/huggingface_hub/main/src/huggingface_hub/file_download.py)
- llama.cpp's `--hf` downloader: appends to `<path>.downloadInProgress`, writes the server ETag to a sibling `<path>.etag`, **discards the partial when the stored ETag differs from the server's**, and reports the resume offset so progress does not restart at 0 — [llama.cpp `common/download.cpp`](https://raw.githubusercontent.com/ggml-org/llama.cpp/master/common/download.cpp)
- llama.cpp also deletes the partial when a resume returns `416` or `200`, with an explicit comment that the kept partial "cannot be resumed: 416 for a complete or stale one, 200 from a server that ignores the range … so the next attempt starts over instead of wedging on the same answer" — [llama.cpp `common/download.cpp`](https://raw.githubusercontent.com/ggml-org/llama.cpp/master/common/download.cpp)
- llama.cpp probes `Accept-Ranges` on `HEAD` and only sends `Range` when the server advertises ranges (`Accept-Ranges != none`) — [llama.cpp `common/download.cpp`](https://raw.githubusercontent.com/ggml-org/llama.cpp/master/common/download.cpp)
- Hugging Face's own guidance: file bytes are served from **separate CDN/Xet hostnames** after redirects from `huggingface.co`, and a `ReadTimeoutError` partway through usually means the CDN host is blocked while the API host is allowed; hosts include `us.aws.cdn.hf.co`, `us.gcp.cdn.hf.co`, `cdn-lfs-us-1.hf.co`, `transfer.xethub.hf.co`, `cas-server.xethub.hf.co` — [HF: Downloading models](https://huggingface.co/docs/hub/en/models-downloading)
- The HF ETag semantics: "An object's ETag is: its git-sha1 if stored in git, or its **sha256** if stored in git-lfs" — [huggingface_hub `file_download.py` (`hf_hub_url` docs)](https://raw.githubusercontent.com/huggingface/huggingface_hub/main/src/huggingface_hub/file_download.py)
- A measured 2026 write-up on resumable HF downloads states the expected SHA-256 for the finished file came from the **`X-Linked-Etag`** response header, and that a 40-second network outage cost **0.5–0.8 GB of repeated transfer** despite resume support — [Big Iron: resumable HF downloads](https://www.bigiron.cc/guides/fast-resumable-hugging-face-model-downloads-hf-cli-xet-and-mirrors)
- `fs4` (fork of `fs2`, pure Rust via `rustix`) exposes `available_space(path)`, `free_space(path)`, `total_space(path)`, `statvfs`, allocation granularity, and `FileExt` preallocation + file locks, on Unix and Windows — [docs.rs/fs4](https://docs.rs/fs4/latest/fs4/)
- Python's disk check is advisory only: `_check_disk_space` warns if `shutil.disk_usage(path).free < expected_size`, walking up parents until a queryable path is found — [huggingface_hub `file_download.py`](https://raw.githubusercontent.com/huggingface/huggingface_hub/main/src/huggingface_hub/file_download.py)

### Inferences (concrete recommended mechanics)

- Redirect handling matters for `If-Range`. `huggingface.co/api/.../resolve/...` answers `302` to a CDN URL. Do `HEAD` on the resolve URL, keep `X-Linked-Etag`/`ETag`, `X-Linked-Size`/`Content-Length`, `Accept-Ranges`, and the final `Location`; then send resumed `GET`s **directly to the CDN URL** with `Range` + `If-Range`. On `403` (expired signed URL) re-resolve and restart the header negotiation, keeping the `.part` (the object is content-addressed, so the bytes are still valid).
- Because the sha256 may come from `X-Linked-Etag` while the CDN's own `ETag` can be a multipart-upload tag, **the pinned digest from Paper's spec is authoritative**; the header digest is only a cross-check. Two primary-ish sources disagree on which header carries the sha256 (HF docs say ETag == sha256 for LFS; the independent test used `X-Linked-Etag`), so verify empirically against the pinned digest on the first real download and log which header matched.
- Restart safety rule: never trust a journalled offset. Use `min(journal_offset, file_len_on_disk)` after `stat`, because buffered writes can be lost in a crash while the journal entry survives. If `file_len > journal_total`, the partial is junk — delete it.
- `fsync` the `.part` file and, after `rename`, `fsync` the containing directory. Neither Ollama nor llama.cpp shows an `fsync` before rename (Ollama closes then renames; llama.cpp `std::rename`s), so a power loss can leave a zero-length or truncated "complete" file. With a pinned hash check on next start this is detectable, but fsync avoids the re-download.
- Preflight: `required = sum(remaining_bytes) * 1.10 + 128 MiB`. Check with `fs4::available_space(dest_dir)` (the destination's filesystem, not `$HOME`). Fail with a distinct exit code (2, not 3) and a message naming the shortfall in MiB. Re-check before each artifact, since the reranker may start after the embedder finished and free space changed.
- Handle `ENOSPC` from `write` as terminal for that artifact (Ollama does exactly this: it returns immediately on `syscall.ENOSPC` rather than burning retries), mark `Failed{NoSpace}`, and surface it — [Ollama `server/download.go`](https://raw.githubusercontent.com/ollama/ollama/main/server/download.go)
- Treat `416 Range Not Satisfiable` as "partial is complete or garbage": hash it; if hash matches, promote; else delete and restart.
- Never enable transparent compression for weight downloads (`Accept-Encoding: identity`), because `Content-Length` then describes a compressed body and the resume offsets no longer refer to file bytes.
- `reqwest` streaming (`Response::bytes_stream()`, the same call `hf-hub` uses) into `tokio::fs::File` or a `spawn_blocking` writer with a 1 MiB buffer is sufficient; no need for `hyper` directly. Rate-limit the write side only if profiling shows it matters.

### Gaps

- No fetched source states whether HF's CDN guarantees `Accept-Ranges: bytes` for all storage backends (Xet-backed files are served through a different path than LFS). The safe assumption is "probe per file, per session".
- No authoritative statement found on whether HF honours `If-Range` on the CDN (it is standard S3/CloudFront behaviour, but unverified here). Fallback design must therefore treat `200` as normal, which the code already does.

---

## Parallelism: what can run concurrently, how to bound it, and whether parallel connections help

### Takeaway

Parallelise **across artifacts** (embedder, reranker, OCR) and use a **small number of range connections per file** (4–8, not 16). At HF's CDN extra connections mostly buy *consistency* against a bad edge, not extra peak throughput, and unbounded concurrency starves the rest of the user's machine. Bound everything with one global semaphore plus a per-host semaphore.

### Cited Findings

- A 2026 downloader benchmark against the HF CDN measured: 1 connection 2.4–17.4 MiB/s, 6 connections 6.8–24.0 MiB/s, 16 connections 19.2–27.3 MiB/s. The author's conclusion: "extra connections buy consistency, not headroom … the peak barely moves, but a single connection can draw a bad edge and crawl at a tenth of the achievable rate. Sixteen connections average that lottery out" — [Kinburg/ModelDL](https://github.com/Kinburg/ModelDL)
- The same project clamps connection count by destination media type because "parallel out-of-order writes are a win on NVMe and a seek storm on a spinning disk" — [Kinburg/ModelDL](https://github.com/Kinburg/ModelDL)
- HF's `hf_xet` uses adaptive concurrency: starts at 1 stream and scales "up to 64 concurrent streams as bandwidth permits"; `HF_XET_NUM_CONCURRENT_RANGE_GETS` defaults to 16; `HF_XET_HIGH_PERFORMANCE=1` raises bounds and buffer sizes — [HF: Downloading models](https://huggingface.co/docs/hub/en/models-downloading); [huggingface_hub issue #2118](https://github.com/huggingface/huggingface_hub/issues/2118)
- HF's own bandwidth tester opens **8 parallel HTTPS streams**, discards the first 1.5 s to skip TCP slow start, and reports the CDN PoP from the `x-hf-cdn-pop` header — [julien-c/hf-speedtest](https://github.com/julien-c/hf-speedtest)
- A 2023 report against the same CDN found a single connection capped near 500 Mbps and 8 connections ~90% faster (635 MiB/s vs 59 MiB/s) — [oobabooga issue #2841](https://github.com/oobabooga/text-generation-webui/issues/2841) (older infrastructure; treat as directional)
- Multi-connection clients "completely max out the connection's bandwidth" and starve other processes; HF's answer is to reduce worker/stream count rather than add a bandwidth limiter — [huggingface_hub issue #2118](https://github.com/huggingface/huggingface_hub/issues/2118)
- Ollama downloads each blob in **16 parts** (`numDownloadParts = 16`), with part size clamped to 100 MB–1000 MB, all parts written into one sparse file via `io.NewOffsetWriter`, and an `errgroup` limited to 16 concurrent part transfers — [Ollama `server/download.go`](https://raw.githubusercontent.com/ollama/ollama/main/server/download.go)
- Ollama also detects stalled parts: a 30 s `downloadStallTimeout` with a ticker that aborts and retries a part whose byte counter has not advanced — [Ollama `server/download.go`](https://raw.githubusercontent.com/ollama/ollama/main/server/download.go)
- `hf-hub`'s cross-file fan-out defaults to 8 workers — [hf-hub `repository/download.rs`](https://raw.githubusercontent.com/huggingface/hf-hub/main/hf-hub/src/repository/download.rs)
- llama.cpp downloads split GGUF shards and sidecars (mmproj, MTP, dflash, eagle3, dspark) **in parallel threads, one stream per file** (`std::async` per task) — [llama.cpp `common/download.go`](https://raw.githubusercontent.com/ggml-org/llama.cpp/master/common/download.cpp)

### Inferences (recommended policy)

- Concurrency plan for Paper's sizes:
  - Global `Semaphore(8)` for HTTP requests; `Semaphore(3)` for whole-artifact downloads; `Semaphore(4)` per artifact for range connections (so worst case 12 sockets, typical 4–8).
  - Part sizing: `parts = clamp(ceil(size / 32 MiB), 1, 8)`. 134 MB → 5 parts ≈ 27 MB; 487 MB → 8 parts ≈ 61 MB; OCR weights (a few MB) → 1 part, no ranges at all. Enter the multi-part path only above ~32 MiB.
  - Skip ranges entirely on non-SSD media if media type is detectable; `fs4` does not report media type, so use a config override rather than guessing.
  - Order: embedder at priority 0 (starts first, gets the per-artifact semaphore immediately), reranker at priority 1 in parallel, OCR last or deferred until the indexer is warm. Never start the reranker's 8 parts while the embedder is still ramping: the embedder is the gating artifact, so give it 2/3 of the socket budget for the first 10 s (simple token-bucket on connection acquisition, or just give the embedder 4 parts and reranker 2 until the embedder is `Ready`).
- Bandwidth bounding: a byte-rate limiter inside the copy loop is ~40 lines (token bucket over `Instant`) and is strictly better than "reduce concurrency" for a background provisioner that must not disturb a video call. HF itself does not offer server-side throttling per connection, so client-side is the only option.
- Parallel range connections for a 134 MB file are close to pointless on a 20 Mbit/s link (a single stream saturates it) and only help on ≥200 Mbit/s links. Make the connection count adaptive: start at 2, and add connections only if the observed rate per connection exceeds ~1 MiB/s and the file is >64 MiB. This is what `hf_xet` does in spirit (1 → 64) and avoids the known pathology where 16 connections on a slow link just multiply timeouts.
- Do not multipart (`multipart/byteranges`) — HF's CDN returns a `multipart/byteranges` body with a CloudFront boundary for multi-range requests, and a 2024 issue documents inconsistent `Content-Range` handling and wrong bodies across runtimes. One range per request is the only reliably parseable form — [huggingface_hub issue #2248](https://github.com/huggingface/huggingface_hub/issues/2248)
- HTTP/2 multiplexing does not remove the need for a socket cap: HF's edge supports h2, but flow-control windows make many concurrent streams on one connection behave similarly to many sockets for buffer purposes. Cap streams, not just sockets.

### Gaps

- No HF-published benchmark of per-connection caps on current infrastructure; the numbers above are third-party measurements on a single route.
- Whether `Accept-Ranges` is advertised for Xet-backed files served through the resolve endpoint was not verified.

---

## Gating and ordering: block indexing until the embedder exists, or index text immediately and backfill vectors?

### Takeaway

Index immediately, embed asynchronously, and gate only the **semantic tier**. At 100k chunks a full backfill is roughly **15 minutes to 1.5 hours of CPU** depending on hardware, which is far too long to make a first-run user wait for *any* search; keyword search over the same corpus is available in seconds. Production tools that block (Ollama, llama.cpp, `transformers`) block because the model *is* the product — for a document search tool the index is the product and the vectors are an enhancement.

### Cited Findings

- Embedding throughput on CPU for all-MiniLM-L6-v2-class models, batch 32, ~50-token inputs: 123.7 sent/s on a 4-core mini PC up to 847.3 sent/s on a 16-core desktop, a 6.85× spread across 8 machines — [Treeru CPU embedding benchmark](https://treeru.com/en/blog/cpu-embedding-benchmark-tool)
- For **chunk-length** inputs (~256 tokens) on an M1 Ultra CPU: 35 texts/s single-text and 112 texts/s batched, i.e. batching buys ~4.3× and long inputs are ~6× slower per item than short ones — [build.labs embedding pipeline](https://pushkarkumar-dev.github.io/system-design-labs/embedding-pipeline)
- Embeddings are deterministic for a given text and model, so cache invalidation is by **model version**, not time; upgrading the model makes all cached vectors stale — [build.labs](https://pushkarkumar-dev.github.io/system-design-labs/embedding-pipeline)
- Ollama's `pull` is a blocking, progress-reported operation that completes (or fails) before the model is usable; there is no "run now, download later" mode in the download path — [Ollama `server/download.go`](https://raw.githubusercontent.com/ollama/ollama/main/server/download.go)
- llama.cpp `-hf` downloads the plan (model + sidecars) and only then returns a path to load; failures throw and abort the run — [llama.cpp `common/download.go`](https://raw.githubusercontent.com/ggml-org/llama.cpp/master/common/download.cpp)
- Hugging Face's lazy alternative is `hf-mount`, which mounts a repo read-only and fetches only the bytes actually read — evidence that "usable before fully downloaded" is a recognised pattern for large artifacts — [HF: Downloading models](https://huggingface.co/docs/hub/en/models-downloading)

### Inferences (cost of a backfill at 100k chunks)

- Using the chunk-length numbers above (they are the closest match to Paper's ~200–400 token line-accurate chunks): 100k chunks at 112/s ≈ **15 minutes** on a fast 16-thread CPU; at 35/s ≈ **48 minutes**; a 4-core laptop scaling the 50-token figure down for ~5× longer inputs lands around 25–30 chunks/s, i.e. **~55–65 minutes**. Budget **15 min – 1.5 h**, plus SQLite writes.
- Storage cost of the backfill is small: 100k × 384 × 4 B ≈ **147 MB** of vectors, which is a single ~150 MB `chunks_vec` write — batching in 512-row transactions keeps WAL growth bounded.
- The backfill is resumable by construction if "needs embedding" is a queryable state, so a crash mid-backfill costs at most one batch.
- Recommended schema (SQLite, alongside the existing `chunks`/`chunks_vec`):
  ```sql
  -- one row per chunk that has (or needs) a vector
  CREATE TABLE chunk_embedding (
    chunk_id   INTEGER PRIMARY KEY REFERENCES chunks(id) ON DELETE CASCADE,
    model_key  TEXT NOT NULL,        -- e.g. 'minilm-l6-v2@sha256:ab12…:384'
    state      TEXT NOT NULL,        -- 'pending' | 'ready' | 'failed'
    attempts   INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL
  );
  CREATE INDEX chunk_embedding_pending ON chunk_embedding(state) WHERE state = 'pending';
  ```
  Insert the `pending` row in the *same transaction* as the chunk, so a crash cannot produce an unembedded chunk with no pending marker. Then the worker does `SELECT chunk_id FROM chunk_embedding WHERE state='pending' ORDER BY chunk_id LIMIT 512`, embeds, `INSERT INTO chunks_vec` + `UPDATE chunk_embedding SET state='ready'` in one transaction. `model_key` embeds the weights hash so a model upgrade flips every row back to `pending` in one `UPDATE`.
- Why not a nullable column on `chunks`? Because `chunks` is the FTS5 external-content table; adding and updating a column there churns the FTS index and the triggers. A side table keeps the hot path untouched and lets the partial index do the work.
- Search behaviour when vectors are missing: tier0 (FTS5) always runs; tier1 joins against `chunk_embedding WHERE state='ready'` (or simply tolerates absent rows in `chunks_vec`, since KNN over a partially populated vec table returns only present rows). The response carries `{"degraded":["semantic"],"pending_chunks":N}` and a hint. Exit code stays `0` when tier0 found something; `3` only when `--semantic` was explicitly requested (matching Paper's existing contract).
- Auto-indexing trigger: the provisioner exposes `watch::Receiver<ArtifactState>`; the indexer awaits `embedder == Ready`, then starts. This satisfies "index immediately after the embedder lands" without a polling loop and without blocking startup.
- The reranker must never gate indexing or tier0/tier1 search — it only gates the rerank stage, which should silently degrade to RRF order when absent (Paper already ranks by RRF; rerank is a reordering).

### Gaps

- I found **no** citable production tool that explicitly implements "index text now, backfill vectors later" for a local single-binary search tool. The pattern is standard in server-side search (add a field, reindex in background), but I could not source a concrete implementation to cite; treat the precedent claim as inference, not fact.
- The benchmark figures come from sentence-transformers/PyTorch; Candle on the same hardware may differ (different kernels, no `OMP_NUM_THREADS` equivalent), so the 15 min–1.5 h range should be re-measured in Paper with `--release` before it appears in user-facing docs.

---

## State machine: exact states, where to persist them, crash/reboot recovery, partial cleanup, corruption detection

### Takeaway

Persist a single JSON document (atomic replace under an exclusive `flock`) describing each artifact as a small state machine keyed by content hash, with the `.part` sidecar as the *only* source of resume truth. On startup, reconcile the journal against the filesystem, treat any `Downloading` state without a held lock as `Paused`, and treat size mismatch as `Corrupt → Absent`.

### Cited Findings

- llama.cpp keeps a `.downloadInProgress` partial plus a `.etag` sidecar, and its correctness rule is explicit: if the partial's stored ETag does not match the server's current ETag, **delete the partial** — [llama.cpp `common/download.cpp`](https://raw.githubusercontent.com/ggml-org/llama.cpp/master/common/download.cpp)
- llama.cpp takes a **file lock** (`<path>.lock`) before touching the cache "so two processes cannot race on the same file", waits if another process is downloading, then re-checks the cache under the lock — [llama.cpp `common/download.cpp`](https://raw.githubusercontent.com/ggml-org/llama.cpp/master/common/download.cpp)
- llama.cpp has a distinct offline path: with `opts.offline` it never touches the network, returns a synthetic `304` for a present file, and errors "required file is not available in cache (offline mode)" otherwise — [llama.cpp `common/download.cpp`](https://raw.githubusercontent.com/ggml-org/llama.cpp/master/common/download.cpp)
- Ollama journals per-part progress as JSON next to the partial (`<name>-partial-<N>`), rebuilds the part list by globbing those files on restart, recomputes `Total`/`Completed` from them, and truncates the destination to `Total` (sparse) — [Ollama `server/download.go`](https://raw.githubusercontent.com/ollama/ollama/main/server/download.go)
- Ollama's backoff is `n² × 10 ms` capped at 10 s, multiplied by a random factor in `[0.5, 1.5)` "to prevent accidental thundering herd problems", with `maxRetries = 6` and an explicit immediate return on `context.Canceled` and `ENOSPC` — [Ollama `server/download.go`](https://raw.githubusercontent.com/ollama/ollama/main/server/download.go)
- `hf-hub` deduplicates concurrent fetches of the same file with an on-disk lock and content-addressed blob names — [docs.rs/hf-hub](https://docs.rs/hf-hub/latest/hf_hub/)
- Python's lock is per-ETag, and its own comment says a `<etag>.incomplete` file "corrupts the cache whenever the surrounding lock is not honored" — i.e. the lock is load-bearing, not an optimisation — [huggingface_hub `file_download.py`](https://raw.githubusercontent.com/huggingface/huggingface_hub/main/src/huggingface_hub/file_download.py)
- At startup Python, when metadata cannot be fetched, falls back to the last known commit from `refs/<revision>` and returns the cached snapshot if the pointer exists — a "network down but cache good" path — [huggingface_hub `file_download.py`](https://raw.githubusercontent.com/huggingface/huggingface_hub/main/src/huggingface_hub/file_download.py)
- `fs4` provides `FileExt` advisory locks (blocking, `try_lock`, shared) plus `available_space`/`free_space`, on Unix and Windows, in pure Rust — [docs.rs/fs4](https://docs.rs/fs4/latest/fs4/)

### Inferences (concrete state machine)

Per artifact (identified by `(name, sha256)` — the hash is part of the identity, so a spec bump creates a new artifact rather than mutating one):

```
Absent
  └─(plan accepted, disk ok)────────────► Queued
Queued
  └─(lock acquired, HEAD 200/302)───────► Downloading{ offset, etag, total }
Downloading
  ├─(all part ranges complete)──────────► Verifying
  ├─(non-retryable error / attempts=0)──► Failed{ reason, attempts, next_retry_at }
  └─(process death)─────────────────────► Paused        (startup reconcile)
Paused
  └─(lock acquired, validator matches)──► Downloading{ resume_offset }
Verifying
  ├─(sha256 == pinned)──────────────────► Ready{ sha256, size, verified_at }
  └─(sha256 mismatch)───────────────────► Absent  (one auto-retry, then Failed{Checksum})
Ready
  └─(startup: size/mtime/hash mismatch)─► Absent | Verifying
Absent ─(offline)───────────────────────► Missing{ offline }
```

Persistence layout (under `PAPER_HOME`, never in the repo):

```
$PAPER_HOME/models/
  state.json                 # atomic: write state.json.tmp, fsync, rename
  state.lock                 # flock exclusive for writers, shared for readers
  state.jsonl                # append-only event log, rotate at 1 MiB
  artifacts/<sha256>/file    # only after verify (or keep HF-style layout if sharing)
  artifacts/<sha256>/.part   # in-progress bytes
  artifacts/<sha256>/.part.json  # {url, etag, last_modified, total, sha256, updated_at}
  artifacts/<sha256>/.lock   # per-artifact flock
```

`state.json` shape:

```json
{
  "schema_version": 1,
  "updated_at": 1730000000,
  "artifacts": {
    "embedder": {
      "sha256": "…", "size": 140509184,
      "state": "downloading", "bytes_done": 83886080,
      "etag": "\"…\"", "last_modified": "…",
      "url": "https://huggingface.co/…/resolve/<commit>/model.safetensors",
      "attempts": 1, "last_error": null, "next_retry_at": null
    },
    "reranker": { "state": "queued" },
    "ocr":      { "state": "ready", "sha256": "…", "verified_at": 1729999999 }
  }
}
```

Rules that make it crash-safe:

1. **One writer.** The provisioner (daemon) holds `state.lock` exclusively for the whole provisioning session. A CLI `paper models pull` that finds the lock held does not fight: it attaches as a reader and tails `state.json`/`state.jsonl` (this is exactly llama.cpp's "waiting for another process" behaviour, minus the deadlock risk).
2. **Filesystem is truth on resume.** On startup, for each artifact in `Downloading`/`Paused`: if the per-artifact lock is free and the process is gone, `stat` the `.part`, set `offset = min(bytes_done, len)`, and re-validate with `If-Range`. If the validator no longer matches, delete `.part` and restart at 0.
3. **Verify before promote.** `sha256` the `.part` (streaming, 1 MiB buffer), compare to the pinned digest, `fsync`, `rename` to `file`, `fsync` the directory, then flip state to `Ready` and *only then* persist — persisting `Ready` first would create a state that claims a file that may not exist.
4. **Promote idempotently.** `rename` over an existing verified file is fine; if `file` already exists and hashes correctly, discard `.part`.
5. **Startup corruption check is cheap by default.** Compare size and, if stored, mtime. Hash only when: state is `Verifying` (crash during verify), the manifest has no `verified_at`, size/mtime changed, or the user runs `paper models verify`. Store `(size, mtime_ns)` with `verified_at` to make this O(1) in the common case.
6. **Cleanup.** Delete `.part` + `.part.json` when: hash mismatch after full download (before retrying), validator change, `416` on a length-zero file, or the artifact is superseded by a new pinned digest. Never delete another digest's `.part` — a downgrade or rollback may still want it.
7. **Bootstrapping trust.** If `file` exists but `state.json` does not (user copied weights in, or state was deleted), hash it; if it matches the pinned digest, adopt it as `Ready` without network. This makes "offline after first provision" robust and makes `PAPER_HOME` portable.
8. **Failed is terminal for the session.** `Failed{Checksum}` must not auto-retry in a loop; it requires `paper models pull --force` or a spec change. `Failed{Network}` retries with capped exponential backoff + jitter, and resets `attempts` on a successful byte (which is what Python does: "Some data has been downloaded from the server so we reset the number of retries" — [file_download.py](https://raw.githubusercontent.com/huggingface/huggingface_hub/main/src/huggingface_hub/file_download.py)).

### Gaps

- No source found for how LM Studio persists download state (closed source); Ollama, llama.cpp and `huggingface_hub` are the only inspectable precedents.
- Whether `fs4`'s advisory locks behave correctly on network filesystems (SMB/NFS) was not verified; if `PAPER_HOME` can live on a network share, add a "lock unsupported → single-process only" degraded mode.

---

## Observability: how a detached provisioner reports progress to the CLI and MCP server

### Takeaway

Do not build an IPC channel. Write a small, versioned, atomically-replaced status document plus an append-only JSONL event log, guarded by `flock`; readers take a shared lock (or just read the whole file, which is always consistent thanks to `rename`). Both the CLI and the MCP server then have progress with zero coupling to the provisioner's lifetime, and "models not ready" is a field in normal search results rather than an error.

### Cited Findings

- Ollama reports progress through a callback invoked on a 60 ms ticker with `{status, digest, total, completed}` while the download runs detached (`go download.Run(context.Background(), …)`), so the HTTP handler can stream progress to the client independently of the transfer goroutine — [Ollama `server/download.go`](https://raw.githubusercontent.com/ollama/ollama/main/server/download.go)
- Ollama also deduplicates concurrent requests for the same blob through a `sync.Map` keyed by digest with reference counting, so two clients pulling the same model share one transfer — [Ollama `server/download.go`](https://raw.githubusercontent.com/ollama/ollama/main/server/download.go)
- llama.cpp reports a structured `common_download_progress { url, downloaded, total, cached }` through a `common_download_callback` with `on_start`/`on_update`/`on_done`, includes a `cached` flag for offline hits, and supports cancellation via `is_cancelled()` — [llama.cpp `common/download.cpp`](https://raw.githubusercontent.com/ggml-org/llama.cpp/master/common/download.cpp)
- llama.cpp explicitly reports the *resume offset* on start "so a resumed file does not start at 0", avoiding a progress bar that jumps backwards — [llama.cpp `common/download.cpp`](https://raw.githubusercontent.com/ggml-org/llama.cpp/master/common/download.cpp)
- `hf-hub` exposes `DownloadEvent::{Start{total_files,total_bytes}, Progress{files}, Complete}` and per-file `FileStatus::{Started,InProgress,Complete}` — a good shape to copy for the JSON document — [hf-hub `repository/download.rs`](https://raw.githubusercontent.com/huggingface/hf-hub/main/hf-hub/src/repository/download.rs)
- `fs4` provides blocking and `try_lock` advisory locks, plus `statvfs`, which is enough to publish free-space alongside progress — [docs.rs/fs4](https://docs.rs/fs4/latest/fs4/)

### Inferences (recommended shape)

- `state.json` is the contract. Fields per artifact: `state`, `bytes_done`, `bytes_total`, `rate_bps` (EWMA over 5 s), `eta_s`, `attempts`, `last_error{kind,message,at}`, `next_retry_at`. Top level: `schema_version`, `updated_at`, `overall` (`idle|provisioning|ready|degraded`), `free_bytes`.
- Write cadence: at most 4 Hz, and always on state transitions. A 200-byte JSON replace is cheaper than a log line; the ticker must not fsync on every write (rename is atomic enough for status; durability matters only for the artifact journal, which is written on transitions).
- `state.jsonl` carries transitions only (`{"t":…,"artifact":"embedder","from":"downloading","to":"verifying"}`), not per-chunk progress. This is what `paper models logs` tails and what a bug report attaches.
- MCP: add a `models_status` tool returning the same JSON (minus timing noise), and include a compact `models` object in search responses: `{"embedder":"ready","reranker":"downloading 41%","semantic":true}`. MCP clients can poll it; no notifications required. If we later want push, the MCP `notifications/tools/list_changed` pattern exists, but polling a 200-byte file is simpler and survives restarts.
- "Not ready" must not be an error: tier0 results return normally with `degraded:["semantic"]` and `hint:"paper models pull"`. Only `--semantic` (explicit) maps to exit code 3, preserving Paper's documented exit code contract.
- Logging: `tracing` with a `provision` span per artifact (`sha256`, `bytes_total`, `attempt`), so `RUST_LOG=paper::provision=debug` gives the same information as the JSONL without a second format. Keep the JSONL for post-hoc, since daemon logs may be discarded by the OS.
- Progress must be monotonic per attempt: on resume, seed `bytes_done` from the `.part` length *before* emitting the first event (llama.cpp's lesson), and clamp `bytes_done <= bytes_total` so a server that ignores Range cannot produce 143%.

### Gaps

- No source found for how LM Studio or the HF desktop tooling surfaces provisioning state to a UI; the design above is inferred from Ollama/llama.cpp/hf-hub shapes.
- Whether MCP clients in the wild poll `models_status` sensibly, or whether a resource subscription would be better, is untested.

---

## Offline and hostile networks: proxies, TLS interception, metered links, `PAPER_OFFLINE`, mirrors, retries, rate limits

### Takeaway

Make offline a first-class state, not an error path; treat the CDN as a separate trust/network domain from the API host; expose `HF_ENDPOINT`-style mirroring, an explicit CA bundle, and a global "do not prefetch" switch; and retry with capped exponential backoff plus jitter, honouring `Retry-After` on 429.

### Cited Findings

- HF file bytes come from separate hostnames after redirects (`us.aws.cdn.hf.co`, `us.gcp.cdn.hf.co`, `cdn-lfs-us-1.hf.co`, `transfer.xethub.hf.co`, `cas-server.xethub.hf.co`, plus EU variants); allowlisting only `huggingface.co` fails, and the list is published as machine-readable JSON at `https://huggingface.co/.well-known/meta.json` — [HF: Downloading models](https://huggingface.co/docs/hub/en/models-downloading)
- A `ReadTimeoutError` partway through "usually means the initial connection succeeded but a storage or CDN host is blocked" — a diagnostic worth reproducing in our error messages — [HF: Downloading models](https://huggingface.co/docs/hub/en/models-downloading)
- All HF API calls are subject to Hub-wide rate limits — [HF Hub API docs](https://huggingface.co/docs/hub/en/api)
- `hf-hub` classifies `RateLimited` and retries 429 with exponential backoff + jitter (5 attempts, base 100 ms) — [docs.rs/hf-hub](https://docs.rs/hf-hub/latest/hf_hub/); [hf-hub `retry.rs`](https://raw.githubusercontent.com/huggingface/hf-hub/main/hf-hub/src/retry.rs)
- Ollama's jitter is multiplicative over a quadratic schedule (`n²×10 ms`, `×[0.5,1.5)`, capped at 10 s) explicitly "to prevent accidental thundering herd problems" — [Ollama `server/download.go`](https://raw.githubusercontent.com/ollama/ollama/main/server/download.go)
- llama.cpp backs off 2 s then ×2 per attempt over 3 attempts, and **removes** the partial when the server does not support ranges — [llama.cpp `common/download.cpp`](https://raw.githubusercontent.com/ggml-org/llama.cpp/master/common/download.cpp)
- Mirrors are a first-class HF concept: `HF_ENDPOINT` reroutes every `huggingface_hub` call with no code change (e.g. `hf-mirror.com`), and community tooling wraps `aria2c -x16` for constrained routes — [Alaya NeW mirror playbook](https://docs.alayanew.com/en/docs/tech-blog/2026-05-05-hf-mirror-acceleration)
- A measured caveat: a caching proxy in front of `HF_ENDPOINT` **only works with Xet disabled** — with Xet on, nginx stored 0 bytes and every download went to the internet; with `HF_HUB_DISABLE_XET=1` the second download used 199 KB of upstream traffic. Also: "downloading the same file into a second directory reuses nothing, even on the same machine, and even with Xet" — [Big Iron](https://www.bigiron.cc/guides/fast-resumable-hugging-face-model-downloads-hf-cli-xet-and-mirrors)
- Xet is now the default for HF repos ("all models on the Model Hub are Xet-backed Git repositories") and `hf_xet` downloads chunks with local caching, so re-fetching a partially downloaded file can reuse chunks — [HF: Downloading models](https://huggingface.co/docs/hub/en/models-downloading); [huggingface_hub `xet_get`](https://raw.githubusercontent.com/huggingface/huggingface_hub/main/src/huggingface_hub/file_download.py)
- `hf-hub` exposes a `socks` feature that forwards to `reqwest/socks` — the only proxy knob it documents — [docs.rs/hf-hub](https://docs.rs/hf-hub/latest/hf_hub/)
- Python's `local_files_only` and `HF_HUB_OFFLINE`-style behaviour returns the cached snapshot without network, and its metadata path tolerates a failed HEAD by falling back to `refs/<revision>` — [huggingface_hub `file_download.py`](https://raw.githubusercontent.com/huggingface/huggingface_hub/main/src/huggingface_hub/file_download.py)

### Inferences (policy)

- Network-domain separation: probe `huggingface.co` (API/HEAD) and the CDN host separately, and produce distinct diagnostics — "cannot reach huggingface.co" vs "reached the Hub but the CDN host `us.aws.cdn.hf.co` timed out (proxy allowlist?)". This is the single most useful support message for corporate networks, per HF's own tip.
- Proxies: rely on `reqwest`'s environment proxy detection (`HTTP_PROXY`/`HTTPS_PROXY`/`ALL_PROXY`/`NO_PROXY`), which is default behaviour when no explicit proxy is set; add `PAPER_PROXY` as an explicit override and a `--no-proxy` escape hatch. (Inference from reqwest's documented default; not re-verified in this session — confirm against the pinned reqwest version.)
- TLS interception: build the client with rustls + **native roots** (`rustls-tls-native-roots`) so the OS trust store — and therefore a corporate MITM root CA installed by IT — is honoured, and add `PAPER_CA_BUNDLE=/path/ca.pem` for the case where the root lives in a Java keystore only. Do not ship `webpki-roots` only; that silently breaks every corporate laptop. (Inference/engineering recommendation; needs a build-feature check.)
- Metered connections: there is no portable OS API for "this is a metered link" (Windows has one via WinRT; macOS/Linux do not expose it portably). Therefore: default to auto-provision on first run, but expose `provision.auto = false` and `paper models pull` as the explicit path, and never re-download the reranker opportunistically on a metered link — the user asked for a 134 MB download implicitly, not a 621 MB one. Show total bytes in the install message so the cost is visible.
- `PAPER_OFFLINE=1` semantics: skip all network attempts, resolve artifacts from state/disk, and if the embedder is absent set `Missing{offline}`. Search still works at tier0; `--semantic` exits 3. Never let offline mode delete or truncate a `.part` — a later online run resumes it.
- Retry budget: 5 attempts, base 250 ms, `min(base * 2^n, 30 s)`, full jitter `sleep = rand(0, delay)`, plus `Retry-After` override when present on 429/503. Reset the attempt counter after any successful read (Python's rule), otherwise a long download with intermittent stalls exhausts its budget at 80%. Cap total wall-clock per artifact (e.g. 30 min without progress) so a wedged socket does not hang the provisioner forever; Ollama's 30 s stall detector is the cheaper version of the same idea.
- Mirrors: implement `PAPER_MODEL_ENDPOINT` (or honour `HF_ENDPOINT` for compatibility) and a per-artifact URL template, because a mirror may serve a different CDN with different range/ETag behaviour. Since the pinned SHA-256 is verified locally, a mirror cannot substitute content — this is the main reason hash-pinning is worth the effort.
- Hostile-network fallbacks in order: direct → mirror from config → user-supplied local file (`paper models add --from ./model.safetensors --sha256 …`). The last one is what makes air-gapped installs possible and costs ~30 lines given the verify path already exists.

### Gaps

- No reliable source on HF's exact 429 limits or whether `Retry-After` is always present; treat it as optional.
- Whether reqwest's default proxy detection covers macOS system proxy settings (as opposed to env vars) was not verified.
- Metered-connection detection: no portable API; no source found for a Rust crate that solves it cross-platform.

---

## Precedent: how Ollama, llama.cpp, huggingface_hub, `hf-hub`, whisper.cpp, LM Studio and `uv` handle this

### Takeaway

Four distinct designs exist, and the split is instructive: **Ollama** = 16-way range + JSON part journal + stall detection (fast, complex, no validator check); **llama.cpp** = single stream + `.etag` sidecar + file lock (simple, resumable, no hashing); **Python `huggingface_hub`** = per-etag `.incomplete` + lock + 5 retries (robust, no hashing); **Rust `hf-hub`** = no resume at all (simple, fastest to ship, worst on flaky links). Nobody in the inspectable set verifies a pinned SHA-256 of the bytes they wrote — that is the gap Paper must close.

### Cited Findings

| Tool | Resume | Integrity | Parallelism | Progress | Offline |
|---|---|---|---|---|---|
| Ollama | Yes, 16 parts, JSON part files, `io.NewOffsetWriter` into one sparse file — [src](https://raw.githubusercontent.com/ollama/ollama/main/server/download.go) | Content-addressed blob names (digest); no end-of-download hash in the download path | 16 part goroutines + `errgroup` limit; dedupe by digest | 60 ms callback ticker `{total, completed}`; stall detector 30 s | n/a (registry-backed) |
| llama.cpp `-hf` | Yes, append to `.downloadInProgress`, `.etag` sidecar, discard on etag mismatch — [src](https://raw.githubusercontent.com/ggml-org/llama.cpp/master/common/download.cpp) | ETag equality + `Content-Length`; no hashing | One stream per file, files in parallel (`std::async`) | `on_start`/`on_update`/`on_done` with resume offset, TTY bar, cancellation | `opts.offline` → synthetic `304`, error if absent |
| `huggingface_hub` | Yes, `Range` + resume on ConnectionError/ReadTimeout, 5 attempts, resets on progress — [src](https://raw.githubusercontent.com/huggingface/huggingface_hub/main/src/huggingface_hub/file_download.py) | Size check vs `expected_size`; ETag-named blob; no content hash | 1 stream/file; `thread_map` across files; Xet chunk cache | tqdm bars, per-file and aggregated; dry-run API | `local_files_only`, refs fallback |
| `hf-hub` (Rust) | **No** — truncates `.incomplete`, restarts at 0 — [src](https://raw.githubusercontent.com/huggingface/hf-hub/main/hf-hub/src/repository/download.rs) | None (ETag-named blob, size only) | `buffer_unordered(8)` across files; Xet batch concurrently | In-process `DownloadEvent` callbacks | `local_files_only` + cache-only resolution |
| whisper.cpp | Not verified in this session | — | — | — | — |
| LM Studio | Not verifiable (closed source) | — | — | — | — |
| `uv` | Not verified in this session; `uv` does verify wheel hashes against the lockfile | Hash-checking of distributions is a documented `uv` feature | — | — | `--offline` |

- Additional Ollama details: retry `maxRetries = 6`, backoff `n²×10 ms` × `[0.5,1.5)` capped at 10 s, immediate abort on `context.Canceled` and `syscall.ENOSPC`, and a direct-URL discovery step with redirect-origin validation before any CDN request — [Ollama `server/download.go`](https://raw.githubusercontent.com/ollama/ollama/main/server/download.go)
- llama.cpp's retry is 3 attempts with a 2 s delay that doubles, plus a `--offline` short-circuit before any network call — [llama.cpp `common/download.cpp`](https://raw.githubusercontent.com/ggml-org/llama.cpp/master/common/download.cpp)
- `huggingface_hub`'s documented cache layout is `blobs/` (content-addressed), `refs/<revision>` → commit, and `snapshots/<commit>/<filename>` symlinks into blobs; symlink-unsupported filesystems get copies instead — [huggingface_hub `file_download.py`](https://raw.githubusercontent.com/huggingface/huggingface_hub/main/src/huggingface_hub/file_download.py)
- HF now defaults to Xet for model repos, so "resume" at the file level is increasingly the wrong abstraction: chunk-level dedup/caching is what upstream is optimising — [HF: Downloading models](https://huggingface.co/docs/hub/en/models-downloading)

### Inferences

- Ollama's design is the right shape for *large* files on *fast* links but is over-engineered for a 134 MB embedder, and it never validates the finished bytes against an expected digest, so a corrupted CDN edge would be silently accepted. Paper should take Ollama's stall detection and part journalling idea, not its 16-connection default.
- llama.cpp's `.etag` sidecar is the single cheapest correctness win available: it makes "is this partial still the same object?" a string comparison instead of a heuristic. Copy that, add `Last-Modified` and the pinned sha256.
- Python's "reset retry counter on any progress" is the difference between a download that survives a flaky train ride and one that dies at 80%. Copy it.
- The absence of hash verification in every inspectable tool is a real gap, not an oversight to imitate: Paper's constraints (must work offline after first provision, citations must resolve, hash-pinned specs) make a bad weight file a silent-quality bug, so verify.

### Gaps

- whisper.cpp's `models/download-ggml-model.sh` is a shell script using `curl`/`wget`; I did not fetch it, so its resume/verify behaviour is unconfirmed.
- LM Studio is closed source; no public description of its downloader was found.
- `uv`'s network downloader internals were not located in this session (the `uv-distribution/src/download.rs` file now contains wheel-local logic, not the HTTP layer), so its resume/parallelism behaviour is unverified; only its hash-verification posture is known.

---

## Consolidated recommendation

### Sequencing

1. `paper install` (or first `paper` run) writes specs, then spawns a detached provisioner (double-fork/`setsid`, or a short-lived child that the daemon adopts) that immediately begins the **embedder** download. Print one line with total bytes and the `paper models status` hint; do not block the shell.
2. Provisioner takes the global state lock, preflights disk, resolves the embedder URL via `HEAD` (`X-Linked-Etag`, `X-Linked-Size`, `Accept-Ranges`, `Location`), and starts 4 range connections.
3. As soon as the embedder verifies, it flips `embedder: ready` and starts the **reranker** (2 connections, raised to 4 when the embedder is done) and the **OCR weights** (single stream).
4. Indexer awaits `watch::Receiver<ArtifactState>` for `embedder == ready`, then indexes text immediately; embedding runs in the background with `chunk_embedding.state='pending'`.
5. Semantic search is enabled per-row as vectors land; the response reports `degraded` only while `pending_chunks > 0`.

### Crate set

`reqwest` (rustls + native roots, `stream` feature) · `tokio` (rt-multi-thread, fs, sync) · `futures` · `sha2` · `fs4` (`available_space`, `FileExt::lock_exclusive`) · `serde`/`serde_json` · `rusqlite` + `sqlite-vec` (already present) · `tracing`. No `hf-hub` for downloads; optionally for `list_tree`/revision resolution only.

### Non-negotiables (mirroring Paper's invariants)

- Search never downloads: tier0 always runs, tier1 degrades, exit 3 only for explicit `--semantic`.
- Every byte is verified against a pinned SHA-256 before it is renamed into place; no `UNPINNED` digests.
- `.part` + sidecar + flock is the only resume state that matters; the JSON state file is a *view*, reconciled from disk at startup.
- Offline (`PAPER_OFFLINE=1`) never touches the network and never destroys resumable bytes.
