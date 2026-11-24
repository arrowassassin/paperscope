use super::{single, Block, Capabilities, ExtractedDocument, Extractor, MAX_EXTRACTED_BYTES};
use anyhow::Result;
use quick_xml::events::Event;
use quick_xml::Reader;
use std::io::{Cursor, Read, Seek};
use zip::ZipArchive;

fn open_zip(bytes: &[u8]) -> Result<ZipArchive<Cursor<&[u8]>>> {
    Ok(ZipArchive::new(Cursor::new(bytes))?)
}

fn read_entry<R: Read + Seek>(zip: &mut ZipArchive<R>, name: &str) -> Option<String> {
    let mut file = zip.by_name(name).ok()?;
    if file.size() as usize > MAX_EXTRACTED_BYTES {
        return None;
    }
    let mut out = String::new();
    file.by_ref()
        .take((MAX_EXTRACTED_BYTES + 1) as u64)
        .read_to_string(&mut out)
        .ok()?;
    if out.len() > MAX_EXTRACTED_BYTES {
        return None;
    }
    Some(out)
}

fn zip_has_entry(bytes: &[u8], name: &str) -> bool {
    match open_zip(bytes) {
        Ok(mut zip) => zip.by_name(name).is_ok(),
        Err(_) => false,
    }
}

fn zip_has_prefix(bytes: &[u8], prefix: &str) -> bool {
    match open_zip(bytes) {
        Ok(mut zip) => {
            for i in 0..zip.len().min(64) {
                let Ok(file) = zip.by_index(i) else { continue };
                let fname = file.name().map(|cow| cow.into_owned()).unwrap_or_default();
                if fname.starts_with(prefix) {
                    return true;
                }
            }
            false
        }
        Err(_) => false,
    }
}

fn attr_value(e: &quick_xml::events::BytesStart<'_>, key: &str) -> Option<String> {
    for attr in e.attributes().flatten() {
        if attr.key.as_ref() == key {
            return Some(attr.value.to_string());
        }
    }
    None
}

pub struct Docx;

impl Extractor for Docx {
    fn name(&self) -> &'static str {
        "docx"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["docx"]
    }

    fn sniff(&self, bytes: &[u8]) -> bool {
        zip_has_entry(bytes, "word/document.xml")
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            max_file_bytes: 128 * 1024 * 1024, // 128 MB for DOCX
            max_pages: 1,
            max_entries: 1,
            streaming: false,
            thread_safe: true,
            requires_external: false,
        }
    }

    fn extract(&self, name: &str, bytes: &[u8]) -> Result<Option<Vec<ExtractedDocument>>> {
        let mut zip = open_zip(bytes)?;
        let Some(xml) = read_entry(&mut zip, "word/document.xml") else {
            return Ok(None);
        };

        let mut reader = Reader::from_str(&xml);
        let mut buf = Vec::new();
        let mut paragraphs: Vec<Paragraph> = Vec::new();
        let mut current = String::new();
        let mut in_text = false;
        let mut level: Option<u8> = None;

        let finish_paragraph =
            |paragraphs: &mut Vec<Paragraph>, text: String, level: Option<u8>| {
                paragraphs.push(Paragraph { text, level });
            };

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(e)) => {
                    if e.name().as_ref() == "w:pStyle" {
                        level = style_level(&e);
                    } else if e.name().as_ref() == "w:t" {
                        in_text = true;
                    }
                }
                Ok(Event::Empty(e)) if e.name().as_ref() == "w:pStyle" => {
                    level = style_level(&e);
                }
                Ok(Event::Text(e)) if in_text => {
                    current.push_str(e.xml10_content().as_ref());
                }
                Ok(Event::End(e)) => {
                    if e.name().as_ref() == "w:t" {
                        in_text = false;
                    } else if e.name().as_ref() == "w:p" {
                        finish_paragraph(&mut paragraphs, std::mem::take(&mut current), level);
                        level = None;
                    }
                }
                Ok(Event::Eof) => break,
                Err(err) => {
                    return Ok(Some(single(
                        name,
                        "docx",
                        Vec::new(),
                        vec![format!("malformed document.xml: {err}")],
                    )))
                }
                _ => {}
            }
            buf.clear();
        }
        let blocks = merge_paragraphs(paragraphs, DOCX_MERGE_MAX_WORDS);
        Ok(Some(single(name, "docx", blocks, Vec::new())))
    }
}

struct Paragraph {
    text: String,
    level: Option<u8>,
}

fn style_level(e: &quick_xml::events::BytesStart<'_>) -> Option<u8> {
    let value = attr_value(e, "w:val")?.to_ascii_lowercase();
    if value == "title" {
        return Some(1);
    }
    if !value.starts_with("heading") {
        return None;
    }
    value
        .trim_start_matches("heading")
        .trim()
        .parse::<u8>()
        .ok()
        .or(Some(1))
}

/// Maximum number of words per merged block when combining consecutive docx
/// paragraphs into a single extracted block.
const DOCX_MERGE_MAX_WORDS: usize = 350;

fn merge_paragraphs(paragraphs: Vec<Paragraph>, max_words: usize) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut path: Vec<(u8, String)> = Vec::new();
    let mut bucket = String::new();
    let mut first = 1usize;
    let mut words = 0usize;

    let mut current_line = 1i64;
    let flush = |blocks: &mut Vec<Block>,
                 bucket: &mut String,
                 first: usize,
                 last: usize,
                 path: &[(u8, String)],
                 current_line: &mut i64| {
        if bucket.trim().is_empty() {
            return;
        }
        let anchor = if first == last {
            format!("para {first}")
        } else {
            format!("para {first}-{last}")
        };
        let text = std::mem::take(bucket);
        let line_count = text.lines().count().max(1) as i64;
        let start = *current_line;
        let end = start + line_count - 1;
        *current_line += line_count;
        blocks.push(Block {
            text,
            page: 0,
            anchor: Some(anchor),
            section: section_of(path),
            line_start: start,
            line_end: end,
        });
    };

    for (idx, paragraph) in paragraphs.iter().enumerate() {
        let number = idx + 1;
        if let Some(level) = paragraph.level {
            flush(
                &mut blocks,
                &mut bucket,
                first,
                number - 1,
                &path,
                &mut current_line,
            );
            words = 0;
            path.retain(|(existing, _)| *existing < level);
            path.push((level, paragraph.text.trim().to_string()));
            first = number;
        }
        if bucket.trim().is_empty() {
            first = number;
        }
        let paragraph_words = paragraph.text.split_whitespace().count();
        bucket.push_str(&paragraph.text);
        bucket.push('\n');
        words += paragraph_words;
        if words >= max_words {
            flush(
                &mut blocks,
                &mut bucket,
                first,
                number,
                &path,
                &mut current_line,
            );
            words = 0;
        }
    }
    flush(
        &mut blocks,
        &mut bucket,
        first,
        paragraphs.len().max(1),
        &path,
        &mut current_line,
    );
    blocks
}

fn section_of(path: &[(u8, String)]) -> Option<String> {
    if path.is_empty() {
        return None;
    }
    Some(
        path.iter()
            .map(|(_, title)| title.as_str())
            .collect::<Vec<_>>()
            .join(" / "),
    )
}

pub struct Pptx;

impl Extractor for Pptx {
    fn name(&self) -> &'static str {
        "pptx"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["pptx"]
    }

    fn sniff(&self, bytes: &[u8]) -> bool {
        zip_has_prefix(bytes, "ppt/slides/slide")
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            max_file_bytes: 256 * 1024 * 1024, // 256 MB for PPTX
            max_pages: 1_000,                  // max slides
            max_entries: 1_000,
            streaming: false,
            thread_safe: true,
            requires_external: false,
        }
    }

    fn extract(&self, name: &str, bytes: &[u8]) -> Result<Option<Vec<ExtractedDocument>>> {
        self.extract_within(name, bytes, MAX_PPTX_BYTES)
    }
}

/// Per-deck cap on slide bytes inflated plus text kept, so a small .pptx cannot
/// expand without bound. 16 slides at the per-slide cap is far above real decks.
const MAX_PPTX_BYTES: usize = 16 * MAX_EXTRACTED_BYTES;

impl Pptx {
    /// Parse each slide as soon as it is read, charging every inflated byte and
    /// every kept byte to `budget`, and stop once it is used up.
    fn extract_within(
        &self,
        name: &str,
        bytes: &[u8],
        budget: usize,
    ) -> Result<Option<Vec<ExtractedDocument>>> {
        let mut zip = open_zip(bytes)?;
        let mut left = budget;
        let mut exhausted = false;
        let mut blocks = Vec::new();
        for i in 0..zip.len().min(self.capabilities().max_entries) {
            let Ok(mut file) = zip.by_index(i) else {
                continue;
            };
            if file.size() as usize > MAX_EXTRACTED_BYTES {
                continue;
            }
            let fname = file.name().map(|cow| cow.into_owned()).unwrap_or_default();
            let Some(rest) = fname.strip_prefix("ppt/slides/slide") else {
                continue;
            };
            let Some(number) = rest
                .strip_suffix(".xml")
                .and_then(|n| n.parse::<u32>().ok())
            else {
                continue;
            };
            let limit = MAX_EXTRACTED_BYTES.min(left);
            let mut data = Vec::new();
            let read = file.by_ref().take(limit as u64 + 1).read_to_end(&mut data);
            left = left.saturating_sub(data.len());
            let xml = match String::from_utf8(data) {
                Ok(xml) if read.is_ok() && xml.len() <= limit => xml,
                _ if left == 0 => {
                    exhausted = true;
                    break;
                }
                _ => continue,
            };
            let mut reader = Reader::from_str(&xml);
            let mut buf = Vec::new();
            let mut text = String::new();
            let mut in_text = false;
            loop {
                match reader.read_event_into(&mut buf) {
                    Ok(Event::Start(e)) => {
                        if e.name().as_ref() == "a:t" {
                            in_text = true;
                        } else if e.name().as_ref() == "a:p"
                            && !text.is_empty()
                            && !text.ends_with('\n')
                        {
                            text.push('\n');
                        }
                    }
                    Ok(Event::Text(e)) if in_text => {
                        text.push_str(e.xml10_content().as_ref());
                    }
                    Ok(Event::End(e)) => {
                        if e.name().as_ref() == "a:t" {
                            in_text = false;
                        } else if e.name().as_ref() == "a:p" {
                            text.push('\n');
                        }
                    }
                    Ok(Event::Eof) => break,
                    Err(_) => break,
                    _ => {}
                }
                buf.clear();
            }
            if text.trim().is_empty() {
                continue;
            }
            let anchor = format!("slide {number}");
            let kept = text.len() + anchor.len();
            if kept > left {
                exhausted = true;
                break;
            }
            left -= kept;
            let line_count = text.lines().count().max(1) as i64;
            blocks.push(Block {
                text,
                page: number as i64,
                anchor: Some(anchor),
                section: None,
                line_start: 1,
                line_end: line_count,
            });
        }
        // Stable, so blocks keep the order sorting slides by number gave.
        blocks.sort_by_key(|block| block.page);
        let mut warnings = Vec::new();
        if exhausted {
            let warning =
                format!("pptx budget of {budget} bytes reached; remaining slides skipped");
            // `extract_named` drops a document without blocks; an error is how
            // the warning survives when no slide had any.
            if blocks.is_empty() {
                anyhow::bail!(warning);
            }
            warnings.push(warning);
        }
        Ok(Some(single(name, "pptx", blocks, warnings)))
    }
}

pub struct Epub;

impl Extractor for Epub {
    fn name(&self) -> &'static str {
        "epub"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["epub"]
    }

    fn sniff(&self, bytes: &[u8]) -> bool {
        zip_has_entry(bytes, "META-INF/container.xml")
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            max_file_bytes: 128 * 1024 * 1024, // 128 MB for EPUB
            max_pages: 5_000,                  // spine items
            max_entries: 5_000,
            streaming: false,
            thread_safe: true,
            requires_external: false,
        }
    }

    fn extract(&self, name: &str, bytes: &[u8]) -> Result<Option<Vec<ExtractedDocument>>> {
        self.extract_within(name, bytes, MAX_EPUB_BYTES)
    }
}

/// Per-book cap on chapter bytes inflated plus paths and text kept, so a spine
/// that repeats one chapter cannot expand without bound. Far above real books.
const MAX_EPUB_BYTES: usize = 16 * MAX_EXTRACTED_BYTES;

impl Epub {
    /// Read the spine (at most `max_pages` items), charging every path kept, every
    /// chapter byte inflated and every text byte kept to `budget`; stop once it is
    /// used up.
    fn extract_within(
        &self,
        name: &str,
        bytes: &[u8],
        budget: usize,
    ) -> Result<Option<Vec<ExtractedDocument>>> {
        let mut zip = open_zip(bytes)?;
        let container = read_entry(&mut zip, "META-INF/container.xml").unwrap_or_default();
        let opf_path = container_path(&container);

        let (mut spine, mut manifest) = (Vec::new(), Vec::new());
        if let Some(opf) = opf_path.as_deref().and_then(|p| read_entry(&mut zip, p)) {
            parse_opf(&opf, &mut manifest, &mut spine);
        }
        let base = opf_path
            .as_deref()
            .and_then(|p| p.rsplit_once('/').map(|(dir, _)| dir.to_string()))
            .unwrap_or_default();

        let max_pages = self.capabilities().max_pages as usize;
        let mut blocks = Vec::new();
        let mut order: Vec<String> = Vec::new();
        let mut order_bytes = 0usize;
        for idref in &spine {
            // A spine may repeat an item without limit: stop one past the page
            // cap (to detect it), or once the paths alone outgrow the budget.
            if order.len() > max_pages || order_bytes > budget {
                break;
            }
            if let Some(href) = manifest.iter().find(|(id, _)| id == idref).map(|(_, h)| h) {
                let path = join_path(&base, href);
                order_bytes += path.len();
                order.push(path);
            }
        }
        if order.is_empty() {
            for i in 0..zip.len() {
                let Ok(f) = zip.by_index(i) else { continue };
                let fname = f.name().map(|cow| cow.into_owned()).unwrap_or_default();
                if fname.ends_with(".xhtml") || fname.ends_with(".html") || fname.ends_with(".htm")
                {
                    order.push(fname);
                }
            }
            order.sort();
        }

        let mut warnings = Vec::new();
        if order.len() > max_pages {
            order.truncate(max_pages);
            warnings.push(format!(
                "epub spine longer than {max_pages} items; remaining items skipped"
            ));
        }
        let budget_warning =
            || format!("epub budget of {budget} bytes reached; remaining chapters skipped");
        // The paths stay alive for the whole loop, so they count as kept bytes.
        let Some(mut left) = budget.checked_sub(order.iter().map(String::len).sum()) else {
            anyhow::bail!(budget_warning());
        };
        let mut exhausted = false;
        for (index, name) in order.iter().enumerate() {
            let Ok(mut file) = zip.by_name(name) else {
                continue;
            };
            if file.size() as usize > MAX_EXTRACTED_BYTES {
                continue;
            }
            let limit = MAX_EXTRACTED_BYTES.min(left);
            let mut data = Vec::new();
            let read = file.by_ref().take(limit as u64 + 1).read_to_end(&mut data);
            left = left.saturating_sub(data.len());
            let raw = match String::from_utf8(data) {
                Ok(raw) if read.is_ok() && raw.len() <= limit => raw,
                _ if left == 0 => {
                    exhausted = true;
                    break;
                }
                _ => continue,
            };
            let text = html2text::from_read(raw.as_bytes(), 100).unwrap_or(raw);
            if text.trim().is_empty() {
                continue;
            }
            let anchor = format!("spine {}: {name}", index + 1);
            let kept = text.len() + anchor.len();
            if kept > left {
                exhausted = true;
                break;
            }
            left -= kept;
            let line_count = text.lines().count().max(1) as i64;
            blocks.push(Block {
                text,
                page: (index + 1) as i64,
                anchor: Some(anchor),
                section: None,
                line_start: 1,
                line_end: line_count,
            });
        }
        if exhausted {
            warnings.push(budget_warning());
        }
        // `extract_named` drops a document without blocks; an error is how the
        // warnings survive when no chapter had any text.
        if blocks.is_empty() && !warnings.is_empty() {
            anyhow::bail!(warnings.join("; "));
        }
        Ok(Some(single(name, "epub", blocks, warnings)))
    }
}

fn container_path(xml: &str) -> Option<String> {
    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) if e.name().as_ref() == "rootfile" => {
                if let Some(path) = attr_value(&e, "full-path") {
                    return Some(path);
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    None
}

fn parse_opf(xml: &str, manifest: &mut Vec<(String, String)>, spine: &mut Vec<String>) {
    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                if e.name().as_ref() == "item" {
                    if let (Some(id), Some(href)) = (attr_value(&e, "id"), attr_value(&e, "href")) {
                        manifest.push((id, href));
                    }
                } else if e.name().as_ref() == "itemref" {
                    if let Some(idref) = attr_value(&e, "idref") {
                        spine.push(idref);
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
}

fn join_path(base: &str, href: &str) -> String {
    let href = href.split('#').next().unwrap_or(href);
    if base.is_empty() {
        href.to_string()
    } else {
        format!("{base}/{href}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    const BUDGET: usize = 64 * 1024;

    fn zip_of(members: &[(String, Vec<u8>)]) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, data) in members {
            zip.start_file(name.as_str(), options).unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    /// An EPUB whose spine lists `spine` (manifest ids) over `chapters` (id, body).
    fn epub(chapters: &[(&str, String)], spine: &[&str]) -> Vec<u8> {
        let manifest: String = chapters
            .iter()
            .map(|(id, _)| format!(r#"<item id="{id}" href="{id}.xhtml"/>"#))
            .collect();
        let itemrefs: String = spine
            .iter()
            .map(|id| format!(r#"<itemref idref="{id}"/>"#))
            .collect();
        let mut members = vec![
            (
                "META-INF/container.xml".to_string(),
                r#"<container><rootfiles><rootfile full-path="OEBPS/content.opf"/></rootfiles></container>"#.to_string(),
            ),
            (
                "OEBPS/content.opf".to_string(),
                format!("<package><manifest>{manifest}</manifest><spine>{itemrefs}</spine></package>"),
            ),
        ];
        // An empty body leaves the item in the manifest but out of the zip.
        for (id, body) in chapters.iter().filter(|(_, body)| !body.is_empty()) {
            members.push((
                format!("OEBPS/{id}.xhtml"),
                format!("<html><body>{body}</body></html>"),
            ));
        }
        let members: Vec<_> = members
            .into_iter()
            .map(|(name, body)| (name, body.into_bytes()))
            .collect();
        zip_of(&members)
    }

    fn slide(number: usize, body: &str) -> (String, Vec<u8>) {
        let xml = format!("<p:sld>{body}</p:sld>");
        (format!("ppt/slides/slide{number}.xml"), xml.into_bytes())
    }

    fn para(text: &str) -> String {
        format!("<a:p><a:r><a:t>{text}</a:t></a:r></a:p>")
    }

    #[test]
    fn a_slide_bomb_stops_at_the_budget() {
        let padding = " ".repeat(BUDGET / 8);
        let mut members = vec![slide(1, &para("first"))];
        members.extend((2..40).map(|n| slide(n, &padding)));
        members.push(slide(40, &para("late")));
        let docs = Pptx
            .extract_within("bomb.pptx", &zip_of(&members), BUDGET)
            .unwrap()
            .unwrap();
        // Textless slides still cost what they inflate to, so slide 40 is never read.
        let anchors: Vec<_> = docs[0].blocks.iter().map(|b| b.anchor.clone()).collect();
        assert_eq!(anchors, [Some("slide 1".to_string())]);
        assert!(docs[0].warnings.iter().any(|w| w.contains("pptx budget")));

        // Without any citable slide the warning must surface as an error.
        let err = Pptx
            .extract_within("bomb.pptx", &zip_of(&members[1..]), BUDGET)
            .unwrap_err();
        assert!(err.to_string().contains("pptx budget"), "{err}");
    }

    #[test]
    fn kept_slide_text_is_charged_to_the_budget() {
        let text = para(&"x".repeat(20_000));
        let members: Vec<_> = (1..=5).map(|n| slide(n, &text)).collect();
        // Inflated bytes alone would admit three slides; their text admits one.
        let docs = Pptx
            .extract_within("text.pptx", &zip_of(&members), BUDGET)
            .unwrap()
            .unwrap();
        assert_eq!(docs[0].blocks.len(), 1);
        assert!(docs[0].warnings.iter().any(|w| w.contains("pptx budget")));
    }

    #[test]
    fn a_normal_deck_is_extracted_unchanged() {
        let members = vec![
            ("[Content_Types].xml".to_string(), b"<Types/>".to_vec()),
            slide(2, &format!("{}{}", para("A"), para("B"))),
            slide(1, &para("Title")),
            slide(3, "<a:p></a:p>"),
            (
                "ppt/slides/_rels/slide1.xml.rels".to_string(),
                b"<Relationships/>".to_vec(),
            ),
        ];
        let docs = Pptx
            .extract("deck.pptx", &zip_of(&members))
            .unwrap()
            .unwrap();
        assert_eq!(docs.len(), 1);
        assert!(docs[0].warnings.is_empty());
        let block = |text: &str, page: i64, lines: i64| Block {
            text: text.to_string(),
            page,
            anchor: Some(format!("slide {page}")),
            section: None,
            line_start: 1,
            line_end: lines,
        };
        assert_eq!(
            docs[0].blocks,
            [block("Title\n", 1, 1), block("A\nB\n", 2, 2)]
        );
    }

    fn anchors(doc: &ExtractedDocument) -> Vec<String> {
        doc.blocks.iter().filter_map(|b| b.anchor.clone()).collect()
    }

    #[test]
    fn a_spine_bomb_stops_at_the_budget() {
        let chapters = [
            ("first", "<p>first</p>".to_string()),
            ("pad", " ".repeat(BUDGET / 8)),
            ("late", "<p>late</p>".to_string()),
        ];
        // One textless chapter named over and over: each reference still costs
        // what it inflates to, so "late" is never read.
        let mut spine = vec!["first"];
        spine.extend(["pad"; 40]);
        spine.push("late");
        let docs = Epub
            .extract_within("bomb.epub", &epub(&chapters, &spine), BUDGET)
            .unwrap()
            .unwrap();
        assert_eq!(anchors(&docs[0]), ["spine 1: OEBPS/first.xhtml"]);
        assert!(docs[0].warnings.iter().any(|w| w.contains("epub budget")));

        // Without any citable chapter the warning must surface as an error.
        let err = Epub
            .extract_within("bomb.epub", &epub(&chapters, &spine[1..]), BUDGET)
            .unwrap_err();
        assert!(err.to_string().contains("epub budget"), "{err}");
    }

    #[test]
    fn kept_chapter_text_is_charged_to_the_budget() {
        let chapters = [("text", format!("<p>{}</p>", "x".repeat(20_000)))];
        // Inflated bytes alone would admit three references; their text admits one.
        let docs = Epub
            .extract_within("text.epub", &epub(&chapters, &["text"; 5]), BUDGET)
            .unwrap()
            .unwrap();
        assert_eq!(docs[0].blocks.len(), 1);
        assert!(docs[0].warnings.iter().any(|w| w.contains("epub budget")));
    }

    #[test]
    fn a_spine_longer_than_max_pages_is_truncated() {
        let chapters = [
            ("first", "<p>first</p>".to_string()),
            ("gone", String::new()),
            ("late", "<p>late</p>".to_string()),
        ];
        let mut spine = vec!["first"];
        spine.extend(["gone"; 4_999]);
        spine.push("late");
        let bytes = epub(&chapters, &spine);
        let docs = Epub.extract("long.epub", &bytes).unwrap().unwrap();
        assert_eq!(anchors(&docs[0]), ["spine 1: OEBPS/first.xhtml"]);
        assert!(docs[0].warnings.iter().any(|w| w.contains("5000 items")));
    }

    #[test]
    fn a_normal_epub_is_extracted_unchanged() {
        let chapters = [
            ("c1", "<h1>One</h1><p>Alpha</p>".to_string()),
            ("c2", "<p>Beta</p>".to_string()),
            ("blank", "<p> </p>".to_string()),
        ];
        // A repeated item is emitted each time, as before.
        let bytes = epub(&chapters, &["c1", "blank", "c2", "c1"]);
        let docs = Epub.extract("book.epub", &bytes).unwrap().unwrap();
        assert_eq!(docs.len(), 1);
        assert!(docs[0].warnings.is_empty());
        let got: Vec<_> = docs[0]
            .blocks
            .iter()
            .map(|b| (b.text.as_str(), b.page, b.anchor.as_deref(), b.line_end))
            .collect();
        assert_eq!(
            got,
            [
                ("# One\n\nAlpha\n", 1, Some("spine 1: OEBPS/c1.xhtml"), 3),
                ("Beta\n", 3, Some("spine 3: OEBPS/c2.xhtml"), 1),
                ("# One\n\nAlpha\n", 4, Some("spine 4: OEBPS/c1.xhtml"), 3),
            ]
        );
    }
}
