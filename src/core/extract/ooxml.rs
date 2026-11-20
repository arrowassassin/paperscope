use super::{single, Block, ExtractedDocument, Extractor, Capabilities, MAX_EXTRACTED_BYTES};
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
    file.by_ref().take((MAX_EXTRACTED_BYTES + 1) as u64).read_to_string(&mut out).ok()?;
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
        let blocks = merge_paragraphs(paragraphs, 350);
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
            flush(&mut blocks, &mut bucket, first, number - 1, &path, &mut current_line);
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
            flush(&mut blocks, &mut bucket, first, number, &path, &mut current_line);
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
            max_pages: 1_000, // max slides
            max_entries: 1_000,
            streaming: false,
            thread_safe: true,
            requires_external: false,
        }
    }

    fn extract(&self, name: &str, bytes: &[u8]) -> Result<Option<Vec<ExtractedDocument>>> {
        let mut zip = open_zip(bytes)?;
        let mut slides: Vec<(u32, String)> = Vec::new();
        for i in 0..zip.len().min(1000) {
            let Ok(mut file) = zip.by_index(i) else { continue };
            if file.size() as usize > MAX_EXTRACTED_BYTES {
                continue;
            }
            let fname = file.name().map(|cow| cow.into_owned()).unwrap_or_default();
            let Some(rest) = fname.strip_prefix("ppt/slides/slide") else {
                continue;
            };
            let Some(num) = rest.strip_suffix(".xml").and_then(|n| n.parse::<u32>().ok()) else {
                continue;
            };
            let mut data = String::new();
            if file.by_ref().take((MAX_EXTRACTED_BYTES + 1) as u64).read_to_string(&mut data).is_ok()
                && data.len() <= MAX_EXTRACTED_BYTES
            {
                slides.push((num, data));
            }
        }
        slides.sort_by_key(|(n, _)| *n);

        let mut blocks = Vec::new();
        for (number, xml) in slides {
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
            let line_count = text.lines().count().max(1) as i64;
            blocks.push(Block {
                text,
                page: number as i64,
                anchor: Some(format!("slide {number}")),
                section: None,
                line_start: 1,
                line_end: line_count,
            });
        }
        Ok(Some(single(name, "pptx", blocks, Vec::new())))
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
            max_pages: 5_000, // spine items
            max_entries: 5_000,
            streaming: false,
            thread_safe: true,
            requires_external: false,
        }
    }

    fn extract(&self, name: &str, bytes: &[u8]) -> Result<Option<Vec<ExtractedDocument>>> {
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

        let mut blocks = Vec::new();
        let mut order: Vec<String> = Vec::new();
        for idref in &spine {
            if let Some(href) = manifest.iter().find(|(id, _)| id == idref).map(|(_, h)| h) {
                order.push(join_path(&base, href));
            }
        }
        if order.is_empty() {
            for i in 0..zip.len() {
                let Ok(f) = zip.by_index(i) else { continue };
                let fname = f.name().map(|cow| cow.into_owned()).unwrap_or_default();
                if fname.ends_with(".xhtml") || fname.ends_with(".html") || fname.ends_with(".htm") {
                    order.push(fname);
                }
            }
            order.sort();
        }

        for (index, name) in order.iter().enumerate() {
            let Some(raw) = read_entry(&mut zip, name) else {
                continue;
            };
            let text = html2text::from_read(raw.as_bytes(), 100).unwrap_or(raw);
            if text.trim().is_empty() {
                continue;
            }
            let line_count = text.lines().count().max(1) as i64;
            blocks.push(Block {
                text,
                page: (index + 1) as i64,
                anchor: Some(format!("spine {}: {name}", index + 1)),
                section: None,
                line_start: 1,
                line_end: line_count,
            });
        }
        Ok(Some(single(name, "epub", blocks, Vec::new())))
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
                    if let (Some(id), Some(href)) =
                        (attr_value(&e, "id"), attr_value(&e, "href"))
                    {
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