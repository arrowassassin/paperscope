use super::{single, Block, ExtractedDocument, Extractor};
use anyhow::Result;

/// Plain text and source-like formats.
///
/// Only valid UTF-8 is accepted and NUL bytes are rejected: everything Paper
/// cites must be line-accurate, and lossy decoding would make the quoted bytes
/// differ from the file. Binary files are never "read as text".
pub struct Plain;

/// Text formats that are read verbatim. Every entry is an explicit promise; an
/// unlisted extension is unsupported rather than guessed.
const TEXT_EXTENSIONS: &[&str] = &[
    "txt", "text", "md", "markdown", "mdx", "rst", "org", "adoc", "tex", "log", "csv", "tsv",
    "json", "jsonl", "ndjson", "yaml", "yml", "toml", "ini", "cfg", "conf", "env", "xml", "svg",
    "sql", "sh", "bash", "zsh", "fish", "ps1", "bat", "py", "rb", "rs", "go", "java", "kt", "c",
    "h", "cc", "cpp", "hpp", "cs", "js", "mjs", "cjs", "ts", "tsx", "jsx", "php", "swift", "scala",
    "lua", "pl", "r", "jl", "ex", "exs", "erl", "hs", "clj", "vim", "el", "diff", "patch",
];

impl Extractor for Plain {
    fn name(&self) -> &'static str {
        "text"
    }

    fn extensions(&self) -> &'static [&'static str] {
        TEXT_EXTENSIONS
    }

    fn extract(&self, name: &str, bytes: &[u8]) -> Result<Option<Vec<ExtractedDocument>>> {
        // A NUL byte in the first block is the standard binary tell, and it is a
        // fact about the file, not a heuristic about its contents.
        if bytes.iter().take(8192).any(|b| *b == 0) {
            return Ok(None);
        }
        let Ok(text) = std::str::from_utf8(bytes) else {
            return Ok(None);
        };
        if text.trim().is_empty() {
            return Ok(None);
        }
        let blocks = if is_markdown(name) {
            markdown_blocks(text)
        } else {
            vec![Block::text(text)]
        };
        Ok(Some(single(name, "text", blocks, Vec::new())))
    }
}

fn is_markdown(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [".md", ".markdown", ".mdx"]
        .iter()
        .any(|ext| lower.ends_with(ext))
}

/// Split Markdown at headings so a block never straddles two sections.
///
/// `#` headings are structure the document declares, so using them is reading,
/// not inference. Line numbers stay exact: each block records the real line
/// range of the source it quotes.
fn markdown_blocks(text: &str) -> Vec<Block> {
    let lines: Vec<&str> = text.lines().collect();
    let mut blocks = Vec::new();
    let mut path: Vec<(usize, String)> = Vec::new();
    let mut buffer: Vec<&str> = Vec::new();
    let mut start = 1usize;

    let flush = |blocks: &mut Vec<Block>,
                 buffer: &mut Vec<&str>,
                 start: usize,
                 end: usize,
                 path: &[(usize, String)]| {
        if buffer.iter().all(|line| line.trim().is_empty()) {
            buffer.clear();
            return;
        }
        blocks.push(Block {
            text: buffer.join("\n"),
            page: 0,
            anchor: None,
            section: section_of(path),
            line_start: start as i64,
            line_end: end.max(start) as i64,
        });
        buffer.clear();
    };

    for (index, line) in lines.iter().enumerate() {
        let number = index + 1;
        if let Some((level, title)) = markdown_heading(line) {
            flush(&mut blocks, &mut buffer, start, number - 1, &path);
            path.retain(|(existing, _)| *existing < level);
            path.push((level, title));
            start = number;
        }
        buffer.push(line);
    }
    flush(&mut blocks, &mut buffer, start, lines.len().max(1), &path);
    blocks
}

fn markdown_heading(line: &str) -> Option<(usize, String)> {
    let trimmed = line.trim_start();
    let hashes = trimmed.chars().take_while(|c| *c == '#').count();
    if !(1..=6).contains(&hashes) || trimmed.chars().nth(hashes) != Some(' ') {
        return None;
    }
    let title = trimmed[hashes..].trim();
    if title.is_empty() {
        return None;
    }
    Some((hashes, title.trim_end_matches('#').trim().to_string()))
}

fn section_of(path: &[(usize, String)]) -> Option<String> {
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

/// HTML, converted to readable text while keeping source line numbers.
///
/// Reflowing with a converter would make line citations point at text that is
/// not in the file, so the text nodes are read directly and each block records
/// the real line range it spans. Headings the document declares become the
/// section path.
pub struct Html;

impl Extractor for Html {
    fn name(&self) -> &'static str {
        "html"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["html", "htm", "xhtml"]
    }

    fn sniff(&self, bytes: &[u8]) -> bool {
        let head = String::from_utf8_lossy(&bytes[..bytes.len().min(512)]).to_ascii_lowercase();
        let head = head.trim_start();
        head.starts_with("<!doctype html") || head.starts_with("<html")
    }

    fn extract(&self, name: &str, bytes: &[u8]) -> Result<Option<Vec<ExtractedDocument>>> {
        let Ok(source) = std::str::from_utf8(bytes) else {
            return Ok(None);
        };
        Ok(Some(single(name, "html", html_blocks(source), Vec::new())))
    }
}

fn html_blocks(source: &str) -> Vec<Block> {
    let line_of = line_index(source);
    let mut reader = quick_xml::Reader::from_str(source);
    reader.config_mut().trim_text(false);

    let mut blocks = Vec::new();
    let mut path: Vec<(usize, String)> = Vec::new();
    let mut buffer = String::new();
    let mut start_line = 1usize;
    let mut in_skip = 0usize;
    let mut heading: Option<usize> = None;

    let flush = |blocks: &mut Vec<Block>,
                     buffer: &mut String,
                     start_line: usize,
                     end_line: usize,
                     path: &[(usize, String)]| {
        let text = buffer.trim();
        if text.is_empty() {
            buffer.clear();
            return;
        }
        blocks.push(Block {
            text: text.to_string(),
            page: 0,
            anchor: None,
            section: section_of(path),
            line_start: start_line as i64,
            line_end: end_line.max(start_line) as i64,
        });
        buffer.clear();
    };

    loop {
        let position = reader.buffer_position() as usize;
        match reader.read_event() {
            Ok(quick_xml::events::Event::Start(e)) => {
                let tag = e.name().as_ref().as_bytes().to_vec();
                if matches!(tag.as_slice(), b"script" | b"style" | b"noscript") {
                    in_skip += 1;
                }
                if let Some(level) = heading_level(&tag) {
                    heading = Some(level);
                }
                if buffer.trim().is_empty() {
                    start_line = line_of(position);
                }
            }
            Ok(quick_xml::events::Event::Text(e)) => {
                if in_skip == 0 {
                    let text = e.xml10_content();
                    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
                    if !text.is_empty() {
                        if buffer.trim().is_empty() {
                            start_line = line_of(position);
                        }
                        if !buffer.is_empty() {
                            buffer.push(' ');
                        }
                        buffer.push_str(&text);
                    }
                }
            }
            Ok(quick_xml::events::Event::End(e)) => {
                let tag = e.name().as_ref().as_bytes().to_vec();
                if matches!(tag.as_slice(), b"script" | b"style" | b"noscript") {
                    in_skip = in_skip.saturating_sub(1);
                }
                let end_line = line_of(reader.buffer_position() as usize);
                if is_block_level(&tag) {
                    flush(&mut blocks, &mut buffer, start_line, end_line, &path);
                }
                if let Some(level) = heading_level(&tag) {
                    if heading.is_some() && level == heading.unwrap() {
                        let title = blocks
                            .last()
                            .map(|block| block.text.clone())
                            .unwrap_or_default();
                        path.retain(|(existing, _)| *existing < level);
                        if !title.is_empty() {
                            path.push((level, title));
                        }
                        heading = None;
                    }
                }
            }
            Ok(quick_xml::events::Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    let end_line = line_of(source.len());
    flush(&mut blocks, &mut buffer, start_line, end_line, &path);
    blocks
}

fn heading_level(tag: &[u8]) -> Option<usize> {
    if tag.len() != 2 || tag[0] != b'h' {
        return None;
    }
    match tag[1] {
        b'1'..=b'6' => Some((tag[1] - b'0') as usize),
        _ => None,
    }
}

fn is_block_level(tag: &[u8]) -> bool {
    matches!(
        tag,
        b"p" | b"div"
            | b"li"
            | b"tr"
            | b"section"
            | b"article"
            | b"header"
            | b"footer"
            | b"blockquote"
            | b"pre"
            | b"td"
            | b"th"
            | b"h1"
            | b"h2"
            | b"h3"
            | b"h4"
            | b"h5"
            | b"h6"
            | b"br"
    )
}

/// Byte offset → 1-based line number, by binary search over newline offsets.
fn line_index(source: &str) -> impl Fn(usize) -> usize + 'static {
    let newlines: Vec<usize> = source
        .bytes()
        .enumerate()
        .filter(|(_, b)| *b == b'\n')
        .map(|(offset, _)| offset)
        .collect();
    move |offset: usize| 1 + newlines.partition_point(|newline| *newline < offset)
}