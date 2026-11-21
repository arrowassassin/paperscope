use std::path::Path;

use crate::core::extract::ExtractedDocument;

/// Shell-out accelerators have been removed for security (CODE_REVIEW C2).
///
/// External binaries (pdftotext, pandoc, libreoffice) process
/// attacker-controlled file content unsandboxed, with no size cap and no
/// privilege separation. They have been dropped entirely; the pure-Rust
/// extractors handle every supported format. The `shell-accel` feature is
/// retained as a no-op so existing feature expressions keep compiling.

/// Try all accelerators for a given file extension. Always returns `Ok(None)`:
/// no external binary is ever spawned.
pub fn try_accelerators(_path: &Path, _ext: &str) -> anyhow::Result<Option<Vec<ExtractedDocument>>> {
    Ok(None)
}
