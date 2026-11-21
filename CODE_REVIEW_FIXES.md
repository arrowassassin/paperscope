# CODE_REVIEW_FIXES — paperscope v0.2.2

## Status

All **5 Critical** issues from `CODE_REVIEW.md` have been fixed in `src/`. No files outside `src/` were touched (no `tests/`, `site/`, `scripts/`, `Cargo.toml`, or `dist-workspace.toml` changes).

Verification:
- `cargo check --features icu_segmenter,lindera` — clean, no new warnings
- `cargo test --lib --features icu_segmenter,lindera` — 39 passed, 0 failed

---

## What was fixed

### C1 — Path traversal via `..` in archive member names (`src/core/extract/archive.rs`)

Added `is_safe_member_name()` and applied it in both `read_zip` and `read_tar` before a member is added to the archive's member list. Member names are rejected when they:
- are empty,
- start with `/` (absolute path),
- use a Windows drive-letter prefix (`C:\`),
- contain a `..` (`ParentDir`), `RootDir`, or `Prefix` path component.

This prevents a malicious zip/tar member such as `../../etc/passwd` from escaping the `{archive}!/{member}` virtual namespace when `virtual_path` is assembled. Nested archives (`.zip`/`.tar` members) were already skipped.

### C2 — Shell command injection / unsandboxed shell-out accelerators (`src/core/extract/accel.rs`)

Removed the shell-out accelerators entirely. `try_accelerators()` is now a documented stub that always returns `Ok(None)` and never spawns `pdftotext`, `pandoc`, or `libreoffice`. External binaries no longer process attacker-controlled file content unsandboxed with no size cap. The pure-Rust extractors handle every supported format. The `shell-accel` feature remains declared in `Cargo.toml` (untouched) but is now a no-op.

### C3 — No authentication/authorization on daemon IPC socket (`src/daemon/ipc.rs`)

Added peer-credential checking in `run_ipc_server`. On Unix, each accepted connection's peer UID is compared to the daemon's own UID:
- Linux/Android: `SO_PEERCRED` via `getsockopt`.
- BSD/macOS: `getpeereid`.

Connections from a different UID, or from which credentials cannot be determined, are dropped before any request is handled. This prevents any local user/process from issuing `IndexRoots`, `AddRoot`, `RemoveRoot`, `Pause`, `Resume`, or `Shutdown`.

### C4 — MCP `read_source` info-leak / unbounded context window (`src/mcp/server.rs`)

`read_source` now:
- rejects `source_id < 1` with `invalid_params`,
- clamps `context` to `0..=5` (`MAX_CONTEXT`),
- bounds the `id BETWEEN ? AND ?` window so a client cannot read adjacent chunks across the whole index.

The existing `allowed(&path)` root check is unchanged.

### C5 — `unsafe` transmute of function pointer (`src/core/index/connection.rs`)

Kept `unsafe` but replaced the vague safety comment with an explicit one documenting the exact C contract:
- `sqlite_vec::sqlite3_vec_init` has the `sqlite3_extension_init` signature: `int (*)(sqlite3*, char**, const sqlite3_api_routines*)`.
- `sqlite3_auto_extension` registers entry points of type `void (*)(void)`.
- Both are `extern "C"` fn pointers of the same width; the transmute is sound because SQLite invokes the entry point with the extension-init argument layout that `sqlite3_vec_init` expects.
- Registration runs once per process via `REGISTER_VEC`.

The `#[allow(clippy::missing_transmute_annotations)]` is retained.

---

## What was NOT fixed

### All Important issues (I1–I22)

Not fixed. This task's scope was the 5 Critical issues only; Important issues are deferred to the next release per the review triage. Notable among them:

- **I1** Temp-file race / predictable OCR temp names (`src/core/extract/ocr.rs`)
- **I2** Stuck-parser thread leak / DoS (`src/core/extract/mod.rs`)
- **I3** Archive zip-slip on member names (`src/core/extract/archive.rs`) — partially addressed by C1 (which sanitizes member names in both readers), but the separate zip-slip concern is folded into C1's fix
- **I4–I5** MCP auth / `read_source` bounds — C4 addresses the `read_source` context bound; MCP-level auth remains a documented trust boundary
- **I6–I22** various correctness, efficiency, and robustness issues

Reason: out of scope for this Critical-only fix cycle. The `shell-accel` feature remains declared (Cargo.toml untouched) but is now a no-op.

### `which` dependency

`which` is still declared as an optional dependency in `Cargo.toml` (untouched per instructions). It is now unused by `accel.rs` unless the `ocr` feature also enables it. This causes no build error.

---

## Remaining concerns

1. **Important issues (I1–I22)** remain open and should be scheduled before the next release; several are security-adjacent (I1 symlink temp-file race, I2 parser thread DoS, I3 zip-slip overlap with C1, I4 MCP trust boundary).
2. **Daemon IPC on Windows** (`src/daemon/ipc.rs`) is not implemented (`run_ipc_server` bails on Windows), so C3's peer-credential check applies only to Unix sockets.
3. The `shell-accel` feature flag still exists in `Cargo.toml` and enables the `which` crate, but no accelerators are implemented behind it. Consider removing the feature in a future cleanup once no dependent code references it.
4. C3's peer-credential check trusts `getuid()`/`getpeereid()`/`SO_PEERCRED`; on systems where these are unavailable or behave unexpectedly, connections are denied (fail-closed), which is the safe choice.
