use super::{single, Block, ExtractedDocument, Extractor};
use anyhow::Result;

/// `.eml` and `.mbox`.
///
/// Headers are indexed too because "the invoice from Acme" is usually a subject
/// line, not body prose. Header rendering is verbatim: no address parsing, no
/// date normalisation.
pub struct Email;

const HEADER_KEYS: &[&str] = &["subject", "from", "to", "date", "message-id"];

impl Extractor for Email {
    fn name(&self) -> &'static str {
        "email"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["eml", "mbox", "mbx"]
    }

    fn sniff(&self, bytes: &[u8]) -> bool {
        let head = &bytes[..bytes.len().min(512)];
        let Ok(text) = std::str::from_utf8(head) else {
            return false;
        };
        let lower = text.to_ascii_lowercase();
        text.starts_with("From ")
            || [
                "return-path:",
                "received:",
                "message-id:",
                "from:",
                "mime-version:",
            ]
            .iter()
            .any(|header| lower.contains(header))
    }

    fn extract(&self, name: &str, bytes: &[u8]) -> Result<Option<Vec<ExtractedDocument>>> {
        let is_mbox = name.to_ascii_lowercase().ends_with(".mbox")
            || name.to_ascii_lowercase().ends_with(".mbx");
        let blocks = if is_mbox {
            let mut blocks = Vec::new();
            for (index, message) in split_mbox(bytes).into_iter().enumerate() {
                if let Some(block) = parse_message(message, &format!("message {}", index + 1)) {
                    blocks.push(block);
                }
            }
            blocks
        } else {
            parse_message(bytes, "message 1").into_iter().collect()
        };
        Ok(Some(single(name, "email", blocks, Vec::new())))
    }
}

fn parse_message(bytes: &[u8], fallback_anchor: &str) -> Option<Block> {
    let mail = mailparse::parse_mail(bytes).ok()?;
    let mut headers = String::new();
    // Headers implements IntoIterator
    for header in mail.get_headers() {
        let key = header.get_key();
        if HEADER_KEYS.contains(&key.to_ascii_lowercase().as_str()) {
            headers.push_str(&format!("{}: {}\n", key, header.get_value()));
        }
    }
    let body = mail.get_body().unwrap_or_default();
    let text = format!("{headers}\n{body}");
    if text.trim().is_empty() {
        return None;
    }
    let message_id = mail
        .get_headers()
        .into_iter()
        .find(|h| h.get_key().eq_ignore_ascii_case("message-id"))
        .map(|h| h.get_value())
        .filter(|v| !v.trim().is_empty());
    let anchor = message_id
        .map(|id| format!("message {id}"))
        .unwrap_or_else(|| fallback_anchor.to_string());
    Some(Block {
        text,
        page: 0,
        anchor: Some(anchor),
        section: None,
        line_start: 0,
        line_end: 0,
    })
}

/// Split an mbox at lines beginning with `From `; the separator line is not part
/// of any message body.
fn split_mbox(bytes: &[u8]) -> Vec<&[u8]> {
    let mut starts = Vec::new();
    let mut offset = 0usize;
    for line in bytes.split_inclusive(|b| *b == b'\n') {
        if line.starts_with(b"From ") {
            starts.push(offset);
        }
        offset += line.len();
    }
    if starts.is_empty() {
        return vec![bytes];
    }
    let mut out = Vec::new();
    for (i, start) in starts.iter().enumerate() {
        let end = starts.get(i + 1).copied().unwrap_or(bytes.len());
        // Skip the `From ` separator line itself.
        let body_start = bytes[*start..end]
            .iter()
            .position(|b| *b == b'\n')
            .map(|p| start + p + 1)
            .unwrap_or(*start);
        if body_start < end {
            out.push(&bytes[body_start..end]);
        }
    }
    out
}
