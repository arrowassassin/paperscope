use super::{
    extension_of, extract_named, is_supported, ExtractedDocument, Extractor, MAX_ARCHIVE_ENTRIES,
    MAX_EXTRACTED_BYTES,
};
use anyhow::Result;
use std::io::{Cursor, Read};

/// Zip and tar archives.
///
/// Members are indexed as `archive.zip!/inner/path`, one document each. Nested
/// archives are deliberately not followed: unbounded recursion on hostile input
/// is a denial-of-service waiting to happen, and a nested container is better
/// indexed as its own file.
pub struct Archive;

impl Extractor for Archive {
    fn name(&self) -> &'static str {
        "archive"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["zip", "tar"]
    }

    fn sniff(&self, bytes: &[u8]) -> bool {
        bytes.starts_with(b"PK\x03\x04") || bytes.get(257..262) == Some(b"ustar")
    }

    fn extract(&self, name: &str, bytes: &[u8]) -> Result<Option<Vec<ExtractedDocument>>> {
        let members = if extension_of(name) == "tar" {
            read_tar(bytes)?
        } else {
            read_zip(bytes)?
        };
        let mut docs = Vec::new();
        for (member, data) in members {
            let member = member.trim_start_matches('/');
            if !is_supported(member) {
                continue;
            }
            let ext = extension_of(member);
            if ext == "zip" || ext == "tar" {
                continue;
            }
            let virtual_path = format!("{name}!/{member}");
            if let Ok(Some(mut extracted)) = extract_named(&virtual_path, &data, false) {
                for doc in &mut extracted {
                    doc.path = virtual_path.clone();
                }
                docs.extend(extracted);
            }
        }
        Ok((!docs.is_empty()).then_some(docs))
    }
}

type Member = (String, Vec<u8>);

fn read_zip(bytes: &[u8]) -> Result<Vec<Member>> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes))?;
    let mut out = Vec::new();
    for i in 0..zip.len().min(MAX_ARCHIVE_ENTRIES) {
        let Ok(mut file) = zip.by_index(i) else {
            continue;
        };
        if file.is_dir() || file.size() as usize > MAX_EXTRACTED_BYTES {
            continue;
        }
        let name = file.name().map(|cow| cow.into_owned()).unwrap_or_default();
        let mut data = Vec::with_capacity(file.size().min(MAX_EXTRACTED_BYTES as u64) as usize);
        if file.by_ref().take((MAX_EXTRACTED_BYTES + 1) as u64).read_to_end(&mut data).is_ok() {
            if data.len() <= MAX_EXTRACTED_BYTES {
                out.push((name, data));
            }
        }
    }
    Ok(out)
}

fn read_tar(bytes: &[u8]) -> Result<Vec<Member>> {
    let mut archive = tar::Archive::new(Cursor::new(bytes));
    let mut out = Vec::new();
    for entry in archive.entries()?.take(MAX_ARCHIVE_ENTRIES) {
        let Ok(mut entry) = entry else { continue };
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let size = entry.header().size().unwrap_or(0) as usize;
        if size > MAX_EXTRACTED_BYTES {
            continue;
        }
        let name = entry
            .path()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        let mut data = Vec::with_capacity(size.min(MAX_EXTRACTED_BYTES));
        if entry.by_ref().take((MAX_EXTRACTED_BYTES + 1) as u64).read_to_end(&mut data).is_ok() {
            if data.len() <= MAX_EXTRACTED_BYTES {
                out.push((name, data));
            }
        }
    }
    Ok(out)
}