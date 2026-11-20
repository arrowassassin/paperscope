use super::{single, Block, Capabilities, ExtractedDocument, Extractor};
use anyhow::Result;
use calamine::{open_workbook_auto_from_rs, Data, Reader};
use std::io::Cursor;

/// Workbooks (XLSX/XLSM/XLS/XLSB/ODS).
pub struct Sheet;

impl Extractor for Sheet {
    fn name(&self) -> &'static str {
        "spreadsheet"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["xlsx", "xlsm", "xls", "xlsb", "ods"]
    }

    fn sniff(&self, bytes: &[u8]) -> bool {
        // OOXML workbooks put their parts under `xl/`; ODS uses `content.xml`.
        if !bytes.starts_with(b"PK\x03\x04") {
            return false;
        }
        let Ok(mut zip) = zip::ZipArchive::new(Cursor::new(bytes)) else {
            return false;
        };
        zip.by_name("xl/workbook.xml").is_ok() || zip.by_name("content.xml").is_ok()
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            max_file_bytes: 128 * 1024 * 1024, // 128 MB for spreadsheets
            max_pages: 1,
            max_entries: 1_000_000, // max rows across all sheets
            streaming: false,
            thread_safe: true,
            requires_external: false,
        }
    }

    /// One block per non-empty worksheet row.
    ///
    /// A row is the natural citation unit for an invoice line or a tax table,
    /// and `Sheet1!B4:H4` is exact. Values are rendered as the file stores them
    /// — no currency guessing, no date parsing.
    fn extract(&self, name: &str, bytes: &[u8]) -> Result<Option<Vec<ExtractedDocument>>> {
        let mut workbook = match open_workbook_auto_from_rs(Cursor::new(bytes)) {
            Ok(w) => w,
            Err(err) => {
                return Ok(Some(single(
                    name,
                    "spreadsheet",
                    Vec::new(),
                    vec![format!("unreadable workbook: {err}")],
                )))
            }
        };
        let sheets = workbook.sheet_names().to_vec();
        let mut blocks = Vec::new();
        let mut warnings = Vec::new();
        for sheet in sheets {
            let range = match workbook.worksheet_range(&sheet) {
                Ok(range) => range,
                Err(err) => {
                    warnings.push(format!("sheet {sheet}: {err}"));
                    continue;
                }
            };
            for (row_index, row) in range.rows().enumerate() {
                let mut cells = Vec::new();
                let mut first_col = None;
                let mut last_col = None;
                for (col, value) in row.iter().enumerate() {
                    let Some(text) = cell_text(value) else {
                        continue;
                    };
                    let reference = format!("{}{}", column_letter(col), row_index + 1);
                    cells.push(format!("{reference}: {text}"));
                    first_col.get_or_insert(col);
                    last_col = Some(col);
                }
                let (Some(first), Some(last)) = (first_col, last_col) else {
                    continue;
                };
                let anchor = format!(
                    "{}!{}{}:{}{}",
                    sheet,
                    column_letter(first),
                    row_index + 1,
                    column_letter(last),
                    row_index + 1
                );
                blocks.push(Block {
                    text: cells.join(" | "),
                    page: 0,
                    anchor: Some(anchor),
                    section: Some(sheet.clone()),
                    line_start: 0,
                    line_end: 0,
                });
            }
        }
        Ok(Some(single(name, "spreadsheet", blocks, warnings)))
    }
}

fn cell_text(value: &Data) -> Option<String> {
    match value {
        Data::Empty => None,
        Data::String(s) => {
            let s = s.trim();
            (!s.is_empty()).then(|| s.to_string())
        }
        Data::Float(f) => {
            if f.fract() == 0.0 {
                Some(format!("{}", *f as i64))
            } else {
                Some(format!("{f}"))
            }
        }
        Data::Int(i) => Some(i.to_string()),
        Data::Bool(b) => Some(b.to_string()),
        Data::DateTime(dt) => Some(dt.to_string()),
        Data::DateTimeIso(s) | Data::DurationIso(s) => {
            let s = s.trim();
            (!s.is_empty()).then(|| s.to_string())
        }
        Data::Error(e) => Some(format!("#{e:?}")),
    }
}

/// 0-based column index → spreadsheet letters (0 → A, 25 → Z, 26 → AA).
pub fn column_letter(mut index: usize) -> String {
    let mut out = String::new();
    loop {
        out.push((b'A' + (index % 26) as u8) as char);
        if index < 26 {
            break;
        }
        index = index / 26 - 1;
    }
    out.chars().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::column_letter;

    #[test]
    fn column_letters() {
        assert_eq!(column_letter(0), "A");
        assert_eq!(column_letter(25), "Z");
        assert_eq!(column_letter(26), "AA");
        assert_eq!(column_letter(27), "AB");
        assert_eq!(column_letter(701), "ZZ");
        assert_eq!(column_letter(702), "AAA");
    }
}
