use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

#[cfg(feature = "shell-accel")]
use which;

use crate::core::extract::{single, ExtractedDocument, MAX_EXTRACTED_BYTES};

/// Shell-out accelerator for faster extraction using system binaries.
///
/// This is feature-gated behind `shell-accel` and provides faster extraction
/// for formats where system tools (pdftotext, pandoc, libreoffice) are available.
/// Falls back to pure-Rust extractors if binaries are not found or timeout occurs.
#[derive(Debug, Clone)]
pub struct Accelerator {
    pub name: &'static str,
    pub binary: &'static str,
    pub args: &'static [&'static str],
    pub extensions: &'static [&'static str],
    pub priority: u8, // Lower = higher priority (tried first)
}

/// Known accelerators. Ordered by priority (lower = tried first).
pub fn accelerators() -> Vec<Accelerator> {
    vec![
        // pdftotext for PDF (fastest for text extraction)
        Accelerator {
            name: "pdftotext",
            binary: "pdftotext",
            args: &["-layout", "-nopgbrk", "-"],
            extensions: &["pdf"],
            priority: 10,
        },
        // pandoc for DOCX, EPUB, HTML (good for structured extraction)
        Accelerator {
            name: "pandoc",
            binary: "pandoc",
            args: &["-f", "auto", "-t", "plain", "--wrap=none", "-"],
            extensions: &["docx", "epub", "html", "htm", "md", "markdown"],
            priority: 20,
        },
        // libreoffice for DOCX, XLSX, PPTX (comprehensive but slower)
        Accelerator {
            name: "libreoffice",
            binary: "libreoffice",
            args: &[
                "--headless",
                "--convert-to",
                "txt:Text",
                "--outdir",
                "/dev/stdout",
                "-",
            ],
            extensions: &["docx", "xlsx", "pptx", "odt", "ods", "odp"],
            priority: 30,
        },
    ]
}

/// Check if a binary exists in PATH.
#[cfg(feature = "shell-accel")]
pub fn find_binary(name: &str) -> Option<std::path::PathBuf> {
    which::which(name).ok()
}

#[cfg(not(feature = "shell-accel"))]
pub fn find_binary(_name: &str) -> Option<std::path::PathBuf> {
    None
}

/// Extract using a shell-out accelerator.
///
/// Returns `Ok(Some(docs))` on success, `Ok(None)` if accelerator not available
/// or failed, `Err` for unexpected errors.
pub fn extract_with_accelerator(
    accelerator: &Accelerator,
    path: &Path,
) -> anyhow::Result<Option<Vec<ExtractedDocument>>> {
    let binary_path = find_binary(accelerator.binary);
    let Some(binary_path) = binary_path else {
        return Ok(None); // Binary not found, fall back
    };

    // Build command with file path as stdin (using `-` for stdin)
    let mut cmd = Command::new(&binary_path);
    for arg in accelerator.args {
        cmd.arg(arg);
    }
    // Replace `-` placeholder with actual file path for stdin
    cmd.stdin(Stdio::from(std::fs::File::open(path)?))
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    // Run with 30s timeout
    let mut child = cmd.spawn()?;
    let timeout = Duration::from_secs(30);
    let start = std::time::Instant::now();

    let output = loop {
        match child.try_wait()? {
            Some(_status) => break child.wait_with_output()?,
            None if start.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Ok(Some(single(
                    &path.to_string_lossy(),
                    accelerator.name,
                    Vec::new(),
                    vec!["accelerator timed out after 30s".to_string()],
                )));
            }
            None => std::thread::sleep(Duration::from_millis(100)),
        }
    };

    if !output.status.success() {
        return Ok(Some(single(
            &path.to_string_lossy(),
            accelerator.name,
            Vec::new(),
            vec![format!("accelerator exited with {}", output.status)],
        )));
    }

    let text = String::from_utf8_lossy(&output.stdout).to_string();
    if text.trim().is_empty() {
        return Ok(None);
    }

    // Truncate if too large
    let text = if text.len() > MAX_EXTRACTED_BYTES {
        let mut cut = MAX_EXTRACTED_BYTES;
        while cut > 0 && !text.is_char_boundary(cut) {
            cut -= 1;
        }
        text[..cut].to_string()
    } else {
        text
    };

    Ok(Some(single(
        &path.to_string_lossy(),
        accelerator.name,
        vec![crate::core::extract::Block::text(text)],
        Vec::new(),
    )))
}

/// Try all accelerators for a given file extension.
/// Returns the first successful result, or `Ok(None)` if none work.
pub fn try_accelerators(path: &Path, ext: &str) -> anyhow::Result<Option<Vec<ExtractedDocument>>> {
    let mut accelerators = accelerators();
    accelerators.retain(|a| a.extensions.contains(&ext));
    accelerators.sort_by_key(|a| a.priority);

    for accelerator in accelerators {
        if let Ok(Some(docs)) = extract_with_accelerator(&accelerator, path) {
            if !docs.is_empty() {
                tracing::info!(
                    "extracted {} using accelerator: {}",
                    path.display(),
                    accelerator.name
                );
                return Ok(Some(docs));
            }
        }
    }
    Ok(None)
}
