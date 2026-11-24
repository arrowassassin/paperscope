use super::{
    extension_of, extract_member, is_supported, ExtractedDocument, Extractor, MAX_ARCHIVE_ENTRIES,
    MAX_EXTRACTED_BYTES,
};
use anyhow::Result;
use std::io::{Cursor, Read};

/// Per-archive cap, applied separately to the bytes members inflate to and to
/// the text kept from them, so a small archive cannot expand without bound.
const MAX_ARCHIVE_BYTES: usize = 16 * MAX_EXTRACTED_BYTES;

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
        extract_within(name, bytes, MAX_ARCHIVE_BYTES)
    }
}

/// Bytes a document keeps in memory beyond its path: every block's text,
/// section and anchor (a section is copied into each block under it, so a long
/// heading over many blocks is as costly as the text) and the warnings.
fn kept_bytes(docs: &[ExtractedDocument]) -> usize {
    docs.iter()
        .map(|doc| {
            let blocks: usize = doc
                .blocks
                .iter()
                .map(|block| {
                    block.text.len()
                        + block.section.as_ref().map_or(0, String::len)
                        + block.anchor.as_ref().map_or(0, String::len)
                })
                .sum();
            blocks + doc.warnings.iter().map(String::len).sum::<usize>()
        })
        .sum()
}

/// Extract members one at a time, stopping once `budget` is used up.
fn extract_within(
    name: &str,
    bytes: &[u8],
    budget: usize,
) -> Result<Option<Vec<ExtractedDocument>>> {
    let mut docs: Vec<ExtractedDocument> = Vec::new();
    let mut kept_left = budget;
    let mut visit = |member: &str, data: &[u8]| {
        let member = member.trim_start_matches('/');
        let virtual_path = format!("{name}!/{member}");
        if let Ok(Some(mut extracted)) = extract_member(&virtual_path, data) {
            // Each document's path is the same virtual path, kept once per document.
            let kept = kept_bytes(&extracted) + extracted.len() * virtual_path.len();
            if kept > kept_left {
                return true;
            }
            kept_left -= kept;
            for doc in &mut extracted {
                doc.path = virtual_path.clone();
            }
            docs.extend(extracted);
        }
        false
    };
    let exhausted = if extension_of(name) == "tar" {
        read_tar(bytes, budget, &mut visit)?
    } else {
        read_zip(bytes, budget, &mut visit)?
    };
    if exhausted {
        let warning = format!("archive budget of {budget} bytes reached; later members skipped");
        // `extract_named` drops documents without blocks; an error is how a
        // whole-file warning survives when no member has any.
        match docs.iter_mut().rev().find(|doc| !doc.blocks.is_empty()) {
            Some(doc) => doc.warnings.push(warning),
            None => anyhow::bail!(warning),
        }
    }
    Ok((!docs.is_empty()).then_some(docs))
}

/// Nested archives are not followed, and unsupported members are never inflated.
fn is_indexable_member(name: &str) -> bool {
    let ext = extension_of(name);
    is_supported(name) && ext != "zip" && ext != "tar"
}

/// Read one member, charging every inflated byte to `left`. `None` skips the
/// member; `None` with `left == 0` means the budget is used up.
fn read_member(reader: impl Read, size: usize, left: &mut usize) -> Option<Vec<u8>> {
    let limit = MAX_EXTRACTED_BYTES.min(*left);
    let mut data = Vec::with_capacity(size.min(limit));
    let ok = reader.take(limit as u64 + 1).read_to_end(&mut data).is_ok();
    *left = left.saturating_sub(data.len());
    (ok && data.len() <= limit).then_some(data)
}

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

/// Walk zip members through `visit` (which returns `true` to stop). Returns
/// `true` when a budget stopped the walk.
fn read_zip(
    bytes: &[u8],
    budget: usize,
    visit: &mut dyn FnMut(&str, &[u8]) -> bool,
) -> Result<bool> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes))?;
    let mut left = budget;
    for i in 0..zip.len().min(MAX_ARCHIVE_ENTRIES) {
        let Ok(mut file) = zip.by_index(i) else {
            continue;
        };
        if file.is_dir() || file.size() as usize > MAX_EXTRACTED_BYTES {
            continue;
        }
        let name = file.name().map(|cow| cow.into_owned()).unwrap_or_default();
        if !is_safe_member_name(&name) || !is_indexable_member(&name) {
            continue;
        }
        let size = file.size() as usize;
        match read_member(&mut file, size, &mut left) {
            Some(data) if visit(&name, &data) => return Ok(true),
            None if left == 0 => return Ok(true),
            _ => {}
        }
    }
    Ok(false)
}

/// Tar counterpart of [`read_zip`].
fn read_tar(
    bytes: &[u8],
    budget: usize,
    visit: &mut dyn FnMut(&str, &[u8]) -> bool,
) -> Result<bool> {
    let mut archive = tar::Archive::new(Cursor::new(bytes));
    let mut left = budget;
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
        if !is_safe_member_name(&name) || !is_indexable_member(&name) {
            continue;
        }
        match read_member(&mut entry, size, &mut left) {
            Some(data) if visit(&name, &data) => return Ok(true),
            None if left == 0 => return Ok(true),
            _ => {}
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::super::extract_named;
    use super::*;
    use std::io::Write;

    const BUDGET: usize = 64 * 1024;

    fn zip_of(members: &[(&str, &[u8])]) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, data) in members {
            zip.start_file(*name, options).unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    fn docx(text: &str) -> Vec<u8> {
        let xml = format!(
            "<w:document><w:body><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:body></w:document>"
        );
        zip_of(&[("word/document.xml", xml.as_bytes())])
    }

    fn doc_paths(docs: &[ExtractedDocument]) -> Vec<&str> {
        docs.iter().map(|doc| doc.path.as_str()).collect()
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

    #[test]
    fn a_zip_bomb_stops_at_the_decompression_budget() {
        let zeros = vec![0u8; BUDGET / 2];
        let members: [(&str, &[u8]); 4] = [
            ("z0.txt", &zeros),
            ("z1.txt", &zeros),
            ("z2.txt", &zeros),
            ("late.txt", b"late text"),
        ];
        // No member yields text, so the warning must surface as an error.
        let err = extract_within("bomb.zip", &zip_of(&members), BUDGET).unwrap_err();
        assert!(err.to_string().contains("archive budget"), "{err}");
    }

    #[test]
    fn extracted_text_is_charged_to_the_budget() {
        let text = "x".repeat(20_000);
        let good = docx(&text);
        let zip = zip_of(&[
            ("d1.docx", &good),
            ("d2.docx", &good),
            ("d3.docx", &good),
            ("bad.docx", b"not a zip"),
            ("d4.docx", &good),
            ("d5.docx", &good),
        ]);
        // Each member inflates to a few hundred bytes; only its text can exhaust 64 KiB.
        let docs = extract_within("t.zip", &zip, BUDGET).unwrap().unwrap();
        assert_eq!(
            doc_paths(&docs),
            [
                "t.zip!/d1.docx",
                "t.zip!/d2.docx",
                "t.zip!/d3.docx",
                "t.zip!/bad.docx"
            ]
        );
        assert!(kept_bytes(&docs) <= BUDGET);
        assert!(docs[2]
            .warnings
            .iter()
            .any(|w| w.contains("archive budget")));
        // The block-less `bad.docx` is kept (F19), so the warning goes on d3.
        assert!(!docs[3]
            .warnings
            .iter()
            .any(|w| w.contains("archive budget")));
    }

    /// The section path is copied into every block under a heading, so a long
    /// Heading1 over many Heading2s costs far more than the text itself.
    #[test]
    fn copied_sections_are_charged_to_the_budget() {
        let h1 = "H".repeat(4_000);
        let mut body = format!(
            "<w:p><w:pPr><w:pStyle w:val=\"Heading1\"/></w:pPr><w:r><w:t>{h1}</w:t></w:r></w:p>"
        );
        for n in 0..10 {
            body.push_str(&format!(
                "<w:p><w:pPr><w:pStyle w:val=\"Heading2\"/></w:pPr><w:r><w:t>h{n}</w:t></w:r></w:p>\
                 <w:p><w:r><w:t>t{n}</w:t></w:r></w:p>"
            ));
        }
        let xml = format!("<w:document><w:body>{body}</w:body></w:document>");
        let member = zip_of(&[("word/document.xml", xml.as_bytes())]);
        let outer = zip_of(
            &(1..=6)
                .map(|n| (format!("d{n}.docx"), member.clone()))
                .collect::<Vec<_>>()
                .iter()
                .map(|(name, data)| (name.as_str(), data.as_slice()))
                .collect::<Vec<_>>(),
        );

        let docs = extract_within("s.zip", &outer, BUDGET).unwrap().unwrap();
        let sections: usize = docs
            .iter()
            .flat_map(|d| &d.blocks)
            .map(|b| b.section.as_ref().map_or(0, String::len))
            .sum();
        // Text alone is ~50 bytes a member; sections are what stopped the walk.
        assert!(sections > 10 * 4_000, "sections kept: {sections}");
        assert!(docs.len() < 6, "{:?}", doc_paths(&docs));
        assert!(kept_bytes(&docs) <= BUDGET);
        assert!(docs
            .last()
            .unwrap()
            .warnings
            .iter()
            .any(|w| w.contains("archive budget")));
    }

    #[test]
    fn unsupported_zip_members_are_not_charged_to_the_budget() {
        let big = vec![0u8; 4 * BUDGET];
        let zip = zip_of(&[
            ("blob.bin", &big),
            ("nested.zip", &big),
            ("a.txt", b"hello archive"),
        ]);
        let docs = extract_within("u.zip", &zip, BUDGET).unwrap().unwrap();
        assert_eq!(doc_paths(&docs), ["u.zip!/a.txt"]);
        assert!(docs[0].warnings.is_empty());
    }

    #[test]
    fn tar_members_are_still_extracted_one_by_one() {
        let mut builder = tar::Builder::new(Vec::new());
        for (name, data) in [
            ("a.txt", &b"first member"[..]),
            ("inner.zip", b"PK\x03\x04"),
            ("b.md", b"# Title\nsecond member"),
        ] {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_cksum();
            builder.append_data(&mut header, name, data).unwrap();
        }
        let tar = builder.into_inner().unwrap();
        let docs = Archive.extract("t.tar", &tar).unwrap().unwrap();
        assert_eq!(doc_paths(&docs), ["t.tar!/a.txt", "t.tar!/b.md"]);
        assert!(docs.iter().all(|doc| doc.warnings.is_empty()));
    }
}
