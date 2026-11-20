//! Structured search filters, applied as SQL WHERE clauses for keyword search
//! and as cheap predicates for vector search.
//!
//! These answer questions keywords cannot: "tax PDFs from 2025", "receipts in
//! ~/Documents/2024", "only spreadsheets".

use rusqlite::types::Value;

#[derive(Debug, Clone, Default)]
pub struct SearchFilters {
    /// Inclusive lower bound on file mtime, Unix milliseconds.
    pub after: Option<i64>,
    /// Inclusive upper bound on file mtime, Unix milliseconds.
    pub before: Option<i64>,
    /// Lower-case extensions without the dot.
    pub extensions: Vec<String>,
    /// Case-insensitive substring of the path.
    pub path_contains: Option<String>,
}

impl SearchFilters {
    /// Build filters from CLI-style parts. Dates are `YYYY-MM-DD`; `before` is
    /// inclusive of the whole day, because users read "before 2025-01-31" as
    /// "up to and including the 31st".
    pub fn from_parts(
        after: Option<&str>,
        before: Option<&str>,
        extensions: &[String],
        path_contains: Option<String>,
    ) -> anyhow::Result<Self> {
        let after = after.map(|d| parse_date(d, false)).transpose()?;
        let before = before.map(|d| parse_date(d, true)).transpose()?;
        Ok(Self {
            after,
            before,
            extensions: extensions
                .iter()
                .map(|e| e.trim_start_matches('.').to_ascii_lowercase())
                .filter(|e| !e.is_empty())
                .collect(),
            path_contains,
        })
    }

    pub fn is_empty(&self) -> bool {
        self.after.is_none()
            && self.before.is_none()
            && self.extensions.is_empty()
            && self.path_contains.is_none()
    }

    /// In-memory predicate, used by vector search (whose KNN clause cannot take
    /// arbitrary extra WHERE terms reliably).
    pub fn matches(&self, path: &str, mtime: i64) -> bool {
        if let Some(after) = self.after {
            if mtime < after {
                return false;
            }
        }
        if let Some(before) = self.before {
            if mtime > before {
                return false;
            }
        }
        if !self.extensions.is_empty() {
            let lower = path.to_ascii_lowercase();
            let matched = self
                .extensions
                .iter()
                .any(|e| lower.ends_with(&format!(".{}", e.to_ascii_lowercase())));
            if !matched {
                return false;
            }
        }
        if let Some(needle) = &self.path_contains {
            if !path
                .to_ascii_lowercase()
                .contains(&needle.to_ascii_lowercase())
            {
                return false;
            }
        }
        true
    }

    /// SQL fragment for `chunks`, including the leading ` AND`.
    pub fn sql(&self) -> (String, Vec<Value>) {
        let mut clauses = Vec::<String>::new();
        let mut params = Vec::new();
        if let Some(after) = self.after {
            clauses.push("chunks.mtime >= ?".to_string());
            params.push(Value::Integer(after));
        }
        if let Some(before) = self.before {
            clauses.push("chunks.mtime <= ?".to_string());
            params.push(Value::Integer(before));
        }
        if !self.extensions.is_empty() {
            let mut ors = Vec::new();
            for ext in &self.extensions {
                ors.push("lower(chunks.path) LIKE ?".to_string());
                params.push(Value::Text(format!("%.{}", ext.to_ascii_lowercase())));
            }
            let ext_clause = format!("({})", ors.join(" OR "));
            clauses.push(ext_clause);
        }
        if let Some(needle) = &self.path_contains {
            clauses.push("lower(chunks.path) LIKE ?".to_string());
            params.push(Value::Text(format!("%{}%", needle.to_ascii_lowercase())));
        }
        if clauses.is_empty() {
            (String::new(), params)
        } else {
            (format!(" AND {}", clauses.join(" AND ")), params)
        }
    }
}

/// Parse `YYYY-MM-DD` into Unix milliseconds. `end_of_day` shifts to the last
/// millisecond of that day, making an upper bound inclusive.
fn parse_date(input: &str, end_of_day: bool) -> anyhow::Result<i64> {
    let mut parts = input.split('-');
    let (Some(y), Some(m), Some(d), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        anyhow::bail!("expected a date as YYYY-MM-DD, got {input:?}");
    };
    let year: i64 = y
        .parse()
        .map_err(|_| anyhow::anyhow!("bad year in {input:?}"))?;
    let month: i64 = m
        .parse()
        .map_err(|_| anyhow::anyhow!("bad month in {input:?}"))?;
    let day: i64 = d
        .parse()
        .map_err(|_| anyhow::anyhow!("bad day in {input:?}"))?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        anyhow::bail!("date out of range: {input:?}");
    }
    let days = days_from_civil(year, month, day);
    let millis = days * 86_400_000;
    Ok(if end_of_day {
        millis + 86_399_999
    } else {
        millis
    })
}

/// Days since 1970-01-01, Howard Hinnant's `days_from_civil` algorithm. Avoids
/// pulling a date-time crate in just to compare file mtimes.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matches_extension_and_path() {
        let f = SearchFilters {
            extensions: vec!["pdf".into()],
            path_contains: Some("tax".into()),
            ..Default::default()
        };
        assert!(f.matches("/docs/tax/2025.pdf", 0));
        assert!(!f.matches("/docs/tax/2025.docx", 0));
        assert!(!f.matches("/docs/other/2025.pdf", 0));
    }

    #[test]
    fn matches_date_window() {
        let f = SearchFilters {
            after: Some(100),
            before: Some(200),
            ..Default::default()
        };
        assert!(f.matches("a.txt", 150));
        assert!(!f.matches("a.txt", 99));
        assert!(!f.matches("a.txt", 201));
    }
}
