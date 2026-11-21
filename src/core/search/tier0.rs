use super::filters::SearchFilters;
use crate::core::index::Connection;
use crate::core::text::{excerpt, fts_segment, is_searchable_query, query_terms, SNIPPET_CHARS};
use anyhow::Result;
use std::path::{Path, PathBuf};

/// Spotlight is unreliable below 3 characters, and 1-2 char queries match almost
/// everything in FTS5 anyway.
pub const MIN_QUERY_LEN: usize = 3;

#[derive(Debug, Clone)]
pub struct RawHit {
    pub chunk_id: i64,
    pub path: String,
    pub page: i64,
    pub line_start: i64,
    pub line_end: i64,
    /// Format-specific anchor (`page 3`, `Sheet1!A5:H5`, `slide 2`).
    pub anchor: Option<String>,
    pub snippet: String,
    /// Normalised relevance in `0..=1`; higher is better.
    pub score: f32,
    pub source: HitSource,
    /// File mtime in Unix milliseconds, used by date filters.
    pub mtime: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitSource {
    Keyword,
    Semantic,
}

/// Words that carry no retrieval signal in a question.
const STOPWORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "by", "can", "could", "did", "do", "does", "for",
    "from", "had", "has", "have", "how", "i", "in", "is", "it", "its", "me", "my", "of", "on",
    "or", "our", "please", "should", "show", "tell", "that", "the", "their", "there", "these",
    "this", "those", "to", "was", "we", "were", "what", "when", "where", "which", "who", "will",
    "with", "would", "you", "your",
];

/// Escape a user query into an FTS5 MATCH expression.
///
/// Terms are combined with `OR` and ranked by `bm25`, which is what makes
/// natural-language questions work: an `AND` of every word in "when does the
/// fridge warranty expire?" matches nothing. Every term is double-quoted (with
/// internal quotes doubled) so user input can never inject FTS5 operators.
///
/// The query is segmented exactly like the indexed text, so a CJK or Thai query
/// produces the same token boundaries the index contains.
pub fn fts5_match_expr(query: &str) -> String {
    let segmented = fts_segment(query);
    let terms: Vec<String> = segmented
        .split_whitespace()
        .filter(|term| !STOPWORDS.contains(&term.to_lowercase().as_str()))
        .map(|term| format!("\"{}\"", term.replace('"', "\"\"")))
        .collect();
    if terms.is_empty() {
        // A query made only of stopwords still has to mean something.
        return segmented
            .split_whitespace()
            .map(|term| format!("\"{}\"", term.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" OR ");
    }
    terms.join(" OR ")
}

/// SQL fragment restricting hits to the configured roots. `path LIKE` on the
/// canonical path keeps everything inside the allow-list.
fn root_filter(roots: &[PathBuf]) -> (String, Vec<String>) {
    if roots.is_empty() {
        return (String::new(), vec![]);
    }
    let mut clauses = Vec::with_capacity(roots.len());
    let mut params = Vec::with_capacity(roots.len());
    for root in roots {
        let mut prefix = root.to_string_lossy().to_string();
        if !prefix.ends_with(std::path::MAIN_SEPARATOR) {
            prefix.push(std::path::MAIN_SEPARATOR);
        }
        clauses.push("(chunks.path = ? OR chunks.path LIKE ?)");
        params.push(root.to_string_lossy().to_string());
        params.push(format!("{}%", prefix));
    }
    (format!(" AND ({})", clauses.join(" OR ")), params)
}

/// Tier 0: exact/keyword retrieval over the local FTS5 index.
pub fn tier0_search(
    conn: &Connection,
    query: &str,
    roots: &[PathBuf],
    limit: usize,
) -> Result<Vec<RawHit>> {
    tier0_search_filtered(conn, query, roots, &SearchFilters::default(), limit)
}

/// Tier 0 with structured filters pushed into SQL, so a filtered query does not
/// retrieve a page of useless candidates and discard them afterwards.
pub fn tier0_search_filtered(
    conn: &Connection,
    query: &str,
    roots: &[PathBuf],
    filters: &SearchFilters,
    limit: usize,
) -> Result<Vec<RawHit>> {
    if !is_searchable_query(query) {
        return Ok(vec![]);
    }
    let match_expr = fts5_match_expr(query);
    if match_expr.is_empty() {
        return Ok(vec![]);
    }

    let (roots_sql, root_params) = root_filter(roots);
    let (filters_sql, filter_values) = filters.sql();
    let sql = format!(
        "SELECT chunks.id, chunks.path, chunks.page, chunks.line_start, chunks.line_end,
                chunks.anchor, chunks.mtime,
                chunks.content,
                bm25(chunks_fts) AS rank
         FROM chunks_fts
         JOIN chunks ON chunks.id = chunks_fts.rowid
         WHERE chunks_fts MATCH ?{roots_sql}{filters_sql}
         ORDER BY rank
         LIMIT ?"
    );

    let mut params: Vec<rusqlite::types::Value> = vec![rusqlite::types::Value::Text(match_expr)];
    params.extend(root_params.into_iter().map(rusqlite::types::Value::Text));
    params.extend(filter_values);
    params.push(rusqlite::types::Value::Integer(limit as i64));
    let refs: Vec<&dyn rusqlite::ToSql> =
        params.iter().map(|p| p as &dyn rusqlite::ToSql).collect();

    let mut stmt = conn.inner().prepare(&sql)?;
    let rows = stmt.query_map(refs.as_slice(), |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, Option<String>>(5)?,
            row.get::<_, i64>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, f64>(8)?,
        ))
    })?;

    let mut hits = Vec::new();
    let mut best: f32 = 0.0;
    let terms = query_terms(query);
    for row in rows {
        let (id, path, page, ls, le, anchor, mtime, content, bm25) = row?;
        // SQLite's bm25() is <= 0; more negative means more relevant.
        let relevance = (-bm25 as f32).max(0.0);
        best = best.max(relevance);
        hits.push(RawHit {
            chunk_id: id,
            path,
            page,
            line_start: ls,
            line_end: le,
            anchor,
            // Built from the stored `content`, not the segmented index text:
            // the excerpt is a verbatim window around the first matching term,
            // so it never has to be "desegmented" back into readable form.
            snippet: excerpt(&content, &terms, SNIPPET_CHARS),
            score: relevance,
            source: HitSource::Keyword,
            mtime,
        });
    }
    if best > 0.0 {
        for hit in &mut hits {
            hit.score /= best;
        }
    }
    Ok(hits)
}

/// macOS only: ask Spotlight which files match. Used to surface documents that
/// match but have not been indexed yet — it never replaces the local index.
#[cfg(target_os = "macos")]
pub fn spotlight_paths(query: &str, roots: &[PathBuf], limit: usize) -> Result<Vec<PathBuf>> {
    use std::process::Command;

    if !is_searchable_query(query) {
        return Ok(vec![]);
    }
    let escaped = query.replace('"', "\\\"");
    let predicate = format!("kMDItemTextContent == \"*{}*\"cd", escaped);
    let roots: Vec<PathBuf> = if roots.is_empty() {
        vec![std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"))]
    } else {
        roots.to_vec()
    };

    let mut out = Vec::new();
    for root in roots {
        let output = Command::new("mdfind")
            .arg("-onlyin")
            .arg(&root)
            .arg(&predicate)
            .output();
        let Ok(output) = output else { continue };
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let p = PathBuf::from(line);
            if p.is_file() {
                out.push(p);
            }
            if out.len() >= limit {
                return Ok(out);
            }
        }
    }
    Ok(out)
}

#[cfg(not(target_os = "macos"))]
pub fn spotlight_paths(_query: &str, _roots: &[PathBuf], _limit: usize) -> Result<Vec<PathBuf>> {
    Ok(vec![])
}

/// True when `path` is inside one of `roots` (or `roots` is empty).
pub fn within_roots(path: &Path, roots: &[PathBuf]) -> bool {
    roots.is_empty() || roots.iter().any(|r| path.starts_with(r))
}
