use super::{
    extension_of, extract_member, is_supported, ExtractedDocument, Extractor, MAX_ARCHIVE_ENTRIES,
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
            if let Ok(Some(mut extracted)) = extract_member(&virtual_path, &data) {
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

/// Return `true` if `name` is safe to use as an archive member in a virtual
/// namespace. Rejects `..` components, absolute paths, and drive-letter
/// prefixes so a member name can never escape the `{archive}!/{member}`
/// virtual namespace (C1: path traversal via archive member names).
fn is_safe_member_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    // Absolute paths escape the virtual namespace.
    if name.starts_with('/') {
        return false;
    }
    // Drive-letter prefixes (e.g. `C:\`) on Windows escape the namespace.
    if cfg!(target_os = "windows") {
        let bytes = name.as_bytes();
        if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
            return false;
        }
    }
    // `..` components escape the virtual namespace; `RootDir` and `Prefix`
    // components are absolute/escaping as well. `CurDir` and `Normal` are safe.
    for component in std::path::Path::new(name).components() {
        match component {
            std::path::Component::ParentDir
            | std::path::Component::RootDir
            | std::path::Component::Prefix(_) => return false,
            std::path::Component::CurDir | std::path::Component::Normal(_) => {}
        }
    }
    true
}

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
        if !is_safe_member_name(&name) {
            continue;
        }
        let mut data = Vec::with_capacity(file.size().min(MAX_EXTRACTED_BYTES as u64) as usize);
        if file
            .by_ref()
            .take((MAX_EXTRACTED_BYTES + 1) as u64)
            .read_to_end(&mut data)
            .is_ok()
            && data.len() <= MAX_EXTRACTED_BYTES
        {
            out.push((name, data));
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
        if !is_safe_member_name(&name) {
            continue;
        }
        let mut data = Vec::with_capacity(size.min(MAX_EXTRACTED_BYTES));
        if entry
            .by_ref()
            .take((MAX_EXTRACTED_BYTES + 1) as u64)
            .read_to_end(&mut data)
            .is_ok()
            && data.len() <= MAX_EXTRACTED_BYTES
        {
            out.push((name, data));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::super::extract_named;
    use std::io::Write;

    fn zip_of(members: &[(&str, &[u8])]) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, data) in members {
            zip.start_file(*name, options).unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    fn paths(outer: &[u8]) -> Vec<String> {
        let docs = extract_named("outer.zip", outer, false).unwrap().unwrap();
        docs.into_iter().map(|d| d.path).collect()
    }

    #[test]
    fn a_member_that_sniffs_as_an_archive_is_not_opened_again() {
        let inner = zip_of(&[("note.txt", b"nested words")]);
        let outer = zip_of(&[("inner.txt", &inner), ("plain.txt", b"top words")]);
        assert_eq!(paths(&outer), ["outer.zip!/plain.txt"]);
    }

    #[test]
    fn container_and_text_members_still_extract() {
        let docx = zip_of(&[(
            "word/document.xml",
            b"<w:document><w:body><w:p><w:r><w:t>docx words</w:t></w:r></w:p></w:body></w:document>",
        )]);
        let outer = zip_of(&[("report.docx", &docx), ("notes.txt", b"plain words")]);
        assert_eq!(
            paths(&outer),
            ["outer.zip!/report.docx", "outer.zip!/notes.txt"]
        );
    }

    #[test]
    fn a_text_member_starting_with_zip_magic_is_not_indexed() {
        let outer = zip_of(&[("a.txt", b"PK\x03\x04garbage not a zip")]);
        assert!(extract_named("outer.zip", &outer, false).unwrap().is_none());
    }
}
