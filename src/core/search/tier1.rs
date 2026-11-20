use super::filters::SearchFilters;
use super::tier0::{HitSource, RawHit};
use crate::core::index::Connection;
use anyhow::Result;
use std::path::PathBuf;

/// Serialise an embedding the way sqlite-vec expects: packed little-endian f32.
pub fn serialize_f32_vec(v: &[f32]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(v.len() * 4);
    for &x in v {
        buf.extend_from_slice(&x.to_le_bytes());
    }
    buf
}

pub fn deserialize_f32_vec(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

/// Tier 1: exact nearest-neighbour search over `chunks_vec`.
///
/// `MATCH ... AND k = ?` is sqlite-vec's KNN syntax; `distance` is the cosine
/// distance, so similarity is `1 - distance`.
pub fn tier1_search(
    conn: &Connection,
    query_vec: &[f32],
    roots: &[PathBuf],
    limit: usize,
) -> Result<Vec<RawHit>> {
    tier1_search_filtered(conn, query_vec, roots, &SearchFilters::default(), limit)
}

/// Tier 1 with structured filters.
///
/// sqlite-vec's KNN clause may not accept arbitrary extra `WHERE` terms, so the
/// filters are applied in memory. When filters are present the KNN fetch is
/// widened, otherwise a filter could drop every neighbour before the limit is
/// filled.
pub fn tier1_search_filtered(
    conn: &Connection,
    query_vec: &[f32],
    roots: &[PathBuf],
    filters: &SearchFilters,
    limit: usize,
) -> Result<Vec<RawHit>> {
    let blob = serialize_f32_vec(query_vec);
    // When filters or roots are present we cannot push them into the sqlite-vec
    // KNN query, so we must fetch neighbours in distance order and filter in
    // memory. Fetching only `limit * 8` risks silently missing every matching
    // chunk when the nearest neighbours all fall outside the filter/roots (C2).
    // Instead, fetch a large superset and keep iterating until we have `limit`
    // passing results or exhaust the table.
    let fetch = if filters.is_empty() && roots.is_empty() {
        limit
    } else {
        // Large superset: fetch enough neighbours that in-memory filtering almost
        // always finds `limit` matches. sqlite-vec will return at most the table
        // row count, so this is bounded by index size.
        usize::max(limit * 64, 4096)
    };
    let sql = "SELECT chunks.id, chunks.path, chunks.page, chunks.line_start, chunks.line_end,
                      chunks.anchor, chunks.mtime,
                      substr(chunks.content, 1, 240) AS snip, v.distance
               FROM chunks_vec AS v
               JOIN chunks ON chunks.id = v.rowid
               WHERE v.embedding MATCH ? AND k = ?
               ORDER BY v.distance";

    let mut stmt = conn.inner().prepare(sql)?;
    let rows = stmt.query_map(rusqlite::params![blob, fetch as i64], |row| {
        Ok(RawHit {
            chunk_id: row.get(0)?,
            path: row.get(1)?,
            page: row.get(2)?,
            line_start: row.get(3)?,
            line_end: row.get(4)?,
            anchor: row.get(5)?,
            mtime: row.get(6)?,
            snippet: row.get(7)?,
            score: 1.0 - row.get::<_, f64>(8)? as f32,
            source: HitSource::Semantic,
        })
    })?;

    let mut hits = Vec::new();
    for row in rows {
        let hit = row?;
        if !roots.is_empty()
            && !roots
                .iter()
                .any(|r| std::path::Path::new(&hit.path).starts_with(r))
        {
            continue;
        }
        if !filters.matches(&hit.path, hit.mtime) {
            continue;
        }
        hits.push(hit);
        if hits.len() >= limit {
            break;
        }
    }
    Ok(hits)
}
