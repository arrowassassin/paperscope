use super::filters::SearchFilters;
use super::hit::{Confidence, Engine, Hit};
use super::tier0::{tier0_search_filtered, HitSource, RawHit, MIN_QUERY_LEN};
use super::tier1::tier1_search_filtered;
use crate::core::config::SearchConfig;
use crate::core::index::Connection;
use crate::core::models::{Embedder, Reranker};
use crate::core::text::{excerpt, is_searchable_query, query_terms, SNIPPET_CHARS};
use anyhow::{bail, Result};
use std::collections::HashMap;
use std::path::PathBuf;

/// Deterministic, non-generative embedder used in tests and in `--no-model`
/// keyword-only operation.
pub trait QueryEmbedder {
    /// Embed a document chunk.
    fn embed(&self, text: &str) -> Result<Vec<f32>>;

    /// Embed a search query. Models with an asymmetric retrieval instruction
    /// (BGE, E5) override this; symmetric models just use `embed`.
    fn embed_query(&self, text: &str) -> Result<Vec<f32>> {
        self.embed(text)
    }

    /// The model identifier (e.g. "bge-small-en-v1.5"). Used to detect
    /// index/search model mismatches.
    fn model_id(&self) -> &str;
}

impl QueryEmbedder for Embedder {
    fn embed(&self, text: &str) -> Result<Vec<f32>> {
        Embedder::embed_document(self, text)
    }

    fn embed_query(&self, text: &str) -> Result<Vec<f32>> {
        Embedder::embed_query(self, text)
    }

    fn model_id(&self) -> &str {
        Embedder::model_id(self)
    }
}

/// Shared handle so the indexer and the search pipeline can reuse one model.
pub type SharedEmbedder = std::sync::Arc<dyn QueryEmbedder + Send + Sync>;

pub struct SearchPipeline {
    conn: Connection,
    embedder: Option<SharedEmbedder>,
    reranker: Option<std::sync::Arc<Reranker>>,
    config: SearchConfig,
}

impl SearchPipeline {
    pub fn new(
        conn: Connection,
        embedder: Option<SharedEmbedder>,
        reranker: Option<std::sync::Arc<Reranker>>,
        config: SearchConfig,
    ) -> Self {
        Self {
            conn,
            embedder,
            reranker,
            config,
        }
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    pub fn search(&self, query: &str, roots: &[PathBuf]) -> Result<Vec<Hit>> {
        self.search_with(query, roots, &SearchFilters::default())
    }

    /// Search with structured filters (`--after`, `--before`, `--ext`, path).
    pub fn search_with(
        &self,
        query: &str,
        roots: &[PathBuf],
        filters: &SearchFilters,
    ) -> Result<Vec<Hit>> {
        if !is_searchable_query(query) {
            bail!(
                "query must be at least {MIN_QUERY_LEN} characters \
                 (one is enough for CJK and other scripts without spaces)"
            );
        }

        // If we have an embedder and the index has a stored embedding model,
        // they must match. This prevents silently searching with the wrong model.
        if let Some(embedder) = &self.embedder {
            if let Some(stored) = self.conn.stored_embedding_model()? {
                if stored != embedder.model_id() {
                    bail!(
                        "embedding model mismatch: index was built with '{stored}' \
                         but search is using '{current}'. Re-index or install the \
                         correct model.",
                        current = embedder.model_id()
                    );
                }
            }
        }

        let kw_hits =
            tier0_search_filtered(&self.conn, query, roots, filters, self.config.keyword_limit)?;

        // Strong keyword evidence: skip the embedding model entirely. This keeps
        // exact lookups (invoice numbers, names) fast and model-free.
        //
        // "Strong" means the top keyword hit carries every content word of the
        // query, not merely "there are ten hits": ten partial `OR` matches of a
        // vague question used to suppress semantic search exactly when it was
        // most needed.
        let mut sem_hits: Vec<RawHit> = Vec::new();
        if !self.keyword_is_decisive(query, &kw_hits) {
            if let Some(embedder) = &self.embedder {
                let query_vec = embedder.embed_query(query)?;
                sem_hits = tier1_search_filtered(
                    &self.conn,
                    &query_vec,
                    roots,
                    filters,
                    self.config.semantic_limit,
                )?
                .into_iter()
                .filter(|hit| hit.score >= self.config.semantic_min_score)
                .collect();
            }
        }

        let fused = rrf_fuse(
            &kw_hits,
            &sem_hits,
            self.config.rrf_k,
            self.config.fused_limit,
        );
        let relevance = relevance_map(&kw_hits, &sem_hits);
        let terms = query_terms(query);
        let mut hits = self.hydrate(&fused, &relevance, &terms)?;

        if let Some(reranker) = &self.reranker {
            // Only the top candidates go through the cross-encoder: it is one
            // forward pass per pair, so reranking everything would be far slower.
            let rest = hits.split_off(self.config.rerank_top.min(hits.len()));
            let mut reranked = reranker.rerank(query, hits)?;
            reranked.extend(rest);
            hits = reranked;
        }

        for hit in &mut hits {
            hit.confidence = self.confidence_for(hit, query, &kw_hits, &sem_hits);
            hit.engine = Engine::Local;
            hit.why_matched = why_matched(hit, &kw_hits, &sem_hits);
        }
        Ok(hits)
    }

    /// True when keyword search alone has already answered the query.
    ///
    /// The bar is lexical coverage of the *whole* query by the best hit, plus
    /// enough corroborating results that this is not a fluke. A query with ten
    /// weak partial matches still goes to the embedding model.
    fn keyword_is_decisive(&self, query: &str, hits: &[RawHit]) -> bool {
        if hits.len() < self.config.keyword_sufficiency_threshold {
            return false;
        }
        let Some(top) = hits.first() else {
            return false;
        };
        // Normalised relevance: 1.0 is the best match in this result set.
        top.score >= 0.9 && full_term_coverage(query, &top.snippet)
    }

    /// Confidence describes the *evidence*, not the rank position.
    ///
    /// A lone keyword hit is not automatically strong evidence: normalising bm25
    /// within a result set of size one would otherwise always yield 1.0.
    fn confidence_for(
        &self,
        hit: &Hit,
        query: &str,
        keyword: &[RawHit],
        semantic: &[RawHit],
    ) -> Confidence {
        let kw = keyword.iter().any(|h| h.chunk_id == hit.source_id);
        let sem = semantic.iter().find(|h| h.chunk_id == hit.source_id);
        match (kw, sem) {
            // Corroborated by two independent retrievers.
            (true, Some(_)) => Confidence::High,
            // Every content word of the query literally appears in the chunk.
            (true, None) if full_term_coverage(query, &hit.content) => Confidence::High,
            (true, None) => Confidence::Medium,
            (false, Some(sem)) if sem.score >= 0.75 => Confidence::High,
            (false, Some(sem)) if sem.score >= 0.5 => Confidence::Medium,
            _ => Confidence::Low,
        }
    }

    /// Turn `(chunk_id, rrf_score)` pairs into full citations.
    ///
    /// RRF scores only define the *order* — they are tiny fractions like 0.016
    /// and meaningless to a user. The displayed `score` is the underlying
    /// normalised relevance, which is also what confidence is derived from.
    fn hydrate(
        &self,
        fused: &[(i64, f32)],
        relevance: &HashMap<i64, f32>,
        terms: &[String],
    ) -> Result<Vec<Hit>> {
        let mut hits = Vec::with_capacity(fused.len());
        let mut stmt = self.conn.inner().prepare(
            "SELECT id, path, source_path, page, line_start, line_end, anchor, section, content
             FROM chunks WHERE id = ?",
        )?;
        for &(id, score) in fused {
            let row = stmt.query_row([id], |row| {
                Ok(Hit {
                    source_id: row.get(0)?,
                    path: row.get(1)?,
                    source_path: row.get(2)?,
                    page: row.get(3)?,
                    line_start: row.get(4)?,
                    line_end: row.get(5)?,
                    anchor: row.get(6)?,
                    section: row.get(7)?,
                    snippet: String::new(),
                    content: row.get::<_, String>(8)?,
                    score: relevance.get(&id).copied().unwrap_or(score),
                    why_matched: String::new(),
                    confidence: Confidence::Low,
                    engine: Engine::Local,
                })
            });
            match row {
                Ok(mut hit) => {
                    hit.snippet = excerpt(&hit.content, terms, SNIPPET_CHARS);
                    hits.push(hit);
                }
                // A chunk can disappear between the scan and hydration.
                Err(rusqlite::Error::QueryReturnedNoRows) => continue,
                Err(e) => return Err(e.into()),
            }
        }
        Ok(hits)
    }
}

/// Reciprocal Rank Fusion. Results present in *both* lists accumulate two
/// contributions and therefore outrank single-source results.
pub fn rrf_fuse(keyword: &[RawHit], semantic: &[RawHit], k: i32, limit: usize) -> Vec<(i64, f32)> {
    let k = k.max(1) as f32;
    let mut scores: HashMap<i64, f32> = HashMap::new();
    for list in [keyword, semantic] {
        for (rank, hit) in list.iter().enumerate() {
            *scores.entry(hit.chunk_id).or_insert(0.0) += 1.0 / (k + rank as f32 + 1.0);
        }
    }
    let mut fused: Vec<(i64, f32)> = scores.into_iter().collect();
    fused.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.0.cmp(&b.0))
    });
    fused.truncate(limit);
    fused
}

/// Best available relevance per chunk: the semantic cosine similarity when we
/// have one, otherwise the normalised keyword score.
fn relevance_map(keyword: &[RawHit], semantic: &[RawHit]) -> HashMap<i64, f32> {
    let mut map = HashMap::new();
    for hit in keyword {
        map.insert(hit.chunk_id, hit.score.clamp(0.0, 1.0));
    }
    for hit in semantic {
        let entry = map.entry(hit.chunk_id).or_insert(0.0);
        *entry = entry.max(hit.score.clamp(0.0, 1.0));
    }
    map
}

/// True when every content word of the query occurs in the text, i.e. an exact
/// lexical match rather than a partial `OR` match.
fn full_term_coverage(query: &str, text: &str) -> bool {
    let text = text.to_lowercase();
    let mut terms = query
        .split_whitespace()
        .map(|t| {
            t.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .filter(|t| t.len() >= 3);
    let Some(first) = terms.next() else {
        return false;
    };
    text.contains(&first) && terms.all(|t| text.contains(&t))
}

/// Snippets are drawn from the stored `content`, not from the segmented index
/// text: FTS5's `snippet()` would emit bigram-separated CJK and would need an
/// unreliable desegmentation pass to look right.
fn why_matched(hit: &Hit, keyword: &[RawHit], semantic: &[RawHit]) -> String {
    let kw = keyword.iter().any(|h| h.chunk_id == hit.source_id);
    let sem = semantic.iter().any(|h| h.chunk_id == hit.source_id);
    match (kw, sem) {
        (true, true) => "matched by keyword and semantic search".to_string(),
        (true, false) => "matched by keyword search".to_string(),
        (false, true) => "matched by semantic search".to_string(),
        (false, false) => "matched".to_string(),
    }
}

/// Used by `Hit::source` serialisation; kept here so `HitSource` stays imported.
pub fn source_of(hit: &RawHit) -> HitSource {
    hit.source
}
