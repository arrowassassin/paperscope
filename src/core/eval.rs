//! Offline evaluation harness for retrieval quality.
//!
//! Uses a deterministic `HashEmbedder` (from `tests/common/mod.rs`) so CI never
//! downloads models. Metrics: recall@k, MRR, nDCG@10, citation accuracy,
//! abstention correctness.

use crate::core::index::Connection;
use crate::core::search::{Hit, QueryEmbedder, SearchPipeline, SharedEmbedder};
use crate::core::config::SearchConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Errors that can occur during evaluation.
#[derive(Debug, Error)]
pub enum EvalError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Search error: {0}")]
    Search(#[from] anyhow::Error),
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("Citation resolution failed: {0}")]
    Citation(String),
    #[error("Invalid golden set: {0}")]
    InvalidGoldenSet(String),
}

/// A single query in the golden set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldenQuery {
    /// Unique query identifier.
    pub id: String,
    /// The query text.
    pub text: String,
    /// Relevance judgments: chunk_id -> relevance (0=not relevant, 1=relevant, 2=highly relevant).
    pub qrels: HashMap<i64, u8>,
    /// Whether this query should have an answer (false = unanswerable, tests abstention).
    pub answerable: bool,
    /// Expected answer text snippet (for citation accuracy verification).
    #[serde(default)]
    pub expected_snippet: Option<String>,
}

/// A document in the golden set corpus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldenDocument {
    /// Document identifier (used as chunk_id in the index).
    pub id: i64,
    /// Path to the document file.
    pub path: String,
    /// Full text content of the document.
    pub content: String,
    /// Optional page number (for paged formats).
    #[serde(default)]
    pub page: i64,
}

/// The complete golden evaluation set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldenSet {
    /// All queries to evaluate.
    pub queries: Vec<GoldenQuery>,
    /// All documents in the corpus.
    pub documents: Vec<GoldenDocument>,
}

/// Metrics computed for a single query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryMetrics {
    pub query_id: String,
    pub recall_at_10: f32,
    pub recall_at_5: f32,
    pub mrr: f32,
    pub ndcg_at_10: f32,
    pub citation_accuracy: f32,
    pub abstention_correct: bool,
    pub retrieved_count: usize,
    pub relevant_count: usize,
}

/// Aggregated evaluation report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalReport {
    pub per_query: Vec<QueryMetrics>,
    pub macro_recall_at_10: f32,
    pub macro_recall_at_5: f32,
    pub macro_mrr: f32,
    pub macro_ndcg_at_10: f32,
    pub macro_citation_accuracy: f32,
    pub macro_abstention_correctness: f32,
    pub total_queries: usize,
    pub answerable_queries: usize,
    pub unanswerable_queries: usize,
}

/// Load a golden set from a JSON file.
pub fn load_golden_set<P: AsRef<Path>>(path: P) -> Result<GoldenSet, EvalError> {
    let content = fs::read_to_string(path)?;
    let set: GoldenSet = serde_json::from_str(&content)?;
    set.validate()?;
    Ok(set)
}

/// Build an in-memory index from the golden set documents.
/// Returns the connection and a mapping from golden set document index to database chunk IDs.
pub fn build_golden_index(
    golden_set: &GoldenSet,
    embedder: SharedEmbedder,
) -> Result<(Connection, HashMap<usize, Vec<i64>>), EvalError> {
    use crate::core::index::{init_db, Indexer};
    use std::env;

    // Create a temporary directory for the database and document files
    let temp_dir = env::temp_dir().join(format!("paper_eval_{}", std::process::id()));
    let db_path = temp_dir.join("eval.db");
    let docs_dir = temp_dir.join("docs");
    fs::create_dir_all(&docs_dir)?;

    // Write all documents to the filesystem
    for doc in &golden_set.documents {
        let doc_path = docs_dir.join(&doc.path);
        fs::create_dir_all(doc_path.parent().unwrap())?;
        fs::write(&doc_path, &doc.content)?;
    }

    // Open connection and index
    {
        let conn = Connection::open(&db_path)?;
        init_db(&conn)?;
        let indexer = Indexer::new(conn, Some(embedder))?;
        indexer.index_roots(&[docs_dir])?;
    }
    // Re-open the connection after indexing (Indexer takes ownership)
    let conn = Connection::open(&db_path)?;

    // Build mapping from golden set document index to chunk IDs
    // by querying the database for source_path
    let mut doc_to_chunks: HashMap<usize, Vec<i64>> = HashMap::new();
    
    // Build a map from filename to golden set document index
    let path_to_doc_idx: HashMap<String, usize> = golden_set
        .documents
        .iter()
        .enumerate()
        .map(|(i, d)| (d.path.clone(), i))
        .collect();

    {
        let mut stmt = conn.inner().prepare(
            "SELECT id, source_path FROM chunks"
        )?;
        let rows = stmt.query_map([], |row| {
            let id: i64 = row.get(0)?;
            let source_path: String = row.get(1)?;
            Ok((id, source_path))
        })?;
        
        for row in rows {
            let (chunk_id, source_path) = row?;
            // Extract filename from source_path (which is the full path)
            let filename = std::path::Path::new(&source_path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&source_path)
                .to_string();
            if let Some(&doc_idx) = path_to_doc_idx.get(&filename) {
                doc_to_chunks.entry(doc_idx).or_default().push(chunk_id);
            }
        }
    } // stmt dropped here, releasing borrow on conn

    Ok((conn, doc_to_chunks))
}

/// Run the full evaluation against a golden set.
pub fn run_eval(
    pipeline: &SearchPipeline,
    golden_set: &GoldenSet,
    doc_to_chunks: &HashMap<usize, Vec<i64>>,
) -> Result<EvalReport, EvalError> {
    let mut per_query = Vec::with_capacity(golden_set.queries.len());

    for query in &golden_set.queries {
        let metrics = evaluate_query(pipeline, query, &golden_set.documents, doc_to_chunks)?;
        per_query.push(metrics);
    }

    let total = per_query.len() as f32;
    let answerable = per_query.iter().filter(|q| q.relevant_count > 0).count() as f32;
    let unanswerable = total - answerable;

    let macro_recall_at_10 = per_query.iter().map(|q| q.recall_at_10).sum::<f32>() / total;
    let macro_recall_at_5 = per_query.iter().map(|q| q.recall_at_5).sum::<f32>() / total;
    let macro_mrr = per_query.iter().map(|q| q.mrr).sum::<f32>() / total;
    let macro_ndcg_at_10 = per_query.iter().map(|q| q.ndcg_at_10).sum::<f32>() / total;
    let macro_citation_accuracy = per_query.iter().map(|q| q.citation_accuracy).sum::<f32>() / total;
    let macro_abstention_correctness = per_query.iter().map(|q| q.abstention_correct as u8 as f32).sum::<f32>() / total;

    Ok(EvalReport {
        per_query,
        macro_recall_at_10,
        macro_recall_at_5,
        macro_mrr,
        macro_ndcg_at_10,
        macro_citation_accuracy,
        macro_abstention_correctness,
        total_queries: total as usize,
        answerable_queries: answerable as usize,
        unanswerable_queries: unanswerable as usize,
    })
}

/// Evaluate a single query against the golden set.
fn evaluate_query(
    pipeline: &SearchPipeline,
    query: &GoldenQuery,
    documents: &[GoldenDocument],
    doc_to_chunks: &HashMap<usize, Vec<i64>>,
) -> Result<QueryMetrics, EvalError> {
    // Run search
    let roots: Vec<PathBuf> = vec![]; // Search all roots (empty = no filter)
    let hits = pipeline.search(&query.text, &roots)?;

    // Translate qrels from golden set document indices to actual database chunk IDs
    let mut translated_qrels: HashMap<i64, u8> = HashMap::new();
    for (&doc_idx, &rel) in &query.qrels {
        if let Some(chunk_ids) = doc_to_chunks.get(&(doc_idx as usize)) {
            for &chunk_id in chunk_ids {
                translated_qrels.insert(chunk_id, rel);
            }
        }
    }

    // Compute relevance judgments using translated qrels
    let relevant_ids: Vec<i64> = translated_qrels
        .iter()
        .filter(|(_, &rel)| rel > 0)
        .map(|(&id, _)| id)
        .collect();

    let retrieved_ids: Vec<i64> = hits.iter().map(|h| h.source_id).collect();

    // Recall@k
    let recall_at_10 = recall_at_k(&retrieved_ids, &relevant_ids, 10);
    let recall_at_5 = recall_at_k(&retrieved_ids, &relevant_ids, 5);

    // MRR (Mean Reciprocal Rank)
    let mrr = compute_mrr(&retrieved_ids, &relevant_ids);

    // nDCG@10
    let ndcg_at_10 = compute_ndcg_at_k(&retrieved_ids, &translated_qrels, 10);

    // Citation accuracy: for each hit in the answer (confidence >= Medium),
    // verify that the cited line range in the source document actually contains
    // the answer text (or at least relevant content).
    let citation_accuracy = compute_citation_accuracy(&hits, query, documents, doc_to_chunks)?;

    // Abstention correctness: when confidence is Low, answer should be not_found
    let abstention_correct = compute_abstention_correctness(&hits, query.answerable);

    Ok(QueryMetrics {
        query_id: query.id.clone(),
        recall_at_10,
        recall_at_5,
        mrr,
        ndcg_at_10,
        citation_accuracy,
        abstention_correct,
        retrieved_count: hits.len(),
        relevant_count: relevant_ids.len(),
    })
}

/// Recall@k: fraction of relevant documents retrieved in top k.
fn recall_at_k(retrieved: &[i64], relevant: &[i64], k: usize) -> f32 {
    if relevant.is_empty() {
        return 1.0; // Vacuously true: no relevant docs to find
    }
    let top_k: std::collections::HashSet<_> = retrieved.iter().take(k).collect();
    let found = relevant.iter().filter(|id| top_k.contains(id)).count();
    found as f32 / relevant.len() as f32
}

/// Mean Reciprocal Rank: 1/rank of first relevant document, 0 if none found.
fn compute_mrr(retrieved: &[i64], relevant: &[i64]) -> f32 {
    let relevant_set: std::collections::HashSet<_> = relevant.iter().collect();
    for (rank, id) in retrieved.iter().enumerate() {
        if relevant_set.contains(id) {
            return 1.0 / (rank + 1) as f32;
        }
    }
    0.0
}

/// nDCG@k: normalized Discounted Cumulative Gain.
/// Gain: 0 for not relevant, 1 for relevant, 3 for highly relevant (2^rel - 1).
fn compute_ndcg_at_k(retrieved: &[i64], qrels: &HashMap<i64, u8>, k: usize) -> f32 {
    let mut dcg = 0.0f32;
    for (i, &id) in retrieved.iter().take(k).enumerate() {
        let rel = qrels.get(&id).copied().unwrap_or(0) as f32;
        let gain = 2.0f32.powf(rel) - 1.0; // 0, 1, 3 for rel 0, 1, 2
        dcg += gain / ((i + 2) as f32).log2();
    }

    // Ideal DCG: sort all relevant by relevance descending
    let mut ideal_gains: Vec<f32> = qrels
        .values()
        .filter(|&&r| r > 0)
        .map(|&r| 2.0f32.powf(r as f32) - 1.0)
        .collect();
    ideal_gains.sort_by(|a, b| b.partial_cmp(a).unwrap());

    let mut idcg = 0.0f32;
    for (i, gain) in ideal_gains.iter().take(k).enumerate() {
        idcg += gain / ((i + 2) as f32).log2();
    }

    if idcg == 0.0 {
        1.0 // No relevant docs, perfect score vacuously
    } else {
        dcg / idcg
    }
}

/// Citation accuracy: verify that the top supported hit (used as the answer text
/// per Answer::from_hits) has a cited line range that actually contains the
/// answer text (or at least relevant content).
fn compute_citation_accuracy(
    hits: &[Hit],
    query: &GoldenQuery,
    documents: &[GoldenDocument],
    doc_to_chunks: &HashMap<usize, Vec<i64>>,
) -> Result<f32, EvalError> {
    // Filter to hits with confidence >= Medium (these are the ones used in answers)
    let supported_hits: Vec<&Hit> = hits
        .iter()
        .filter(|h| !matches!(h.confidence, crate::core::search::Confidence::Low))
        .collect();

    if supported_hits.is_empty() {
        // No answer produced - this is correct for unanswerable queries
        return Ok(if query.answerable { 0.0 } else { 1.0 });
    }

    // Answer::from_hits uses the FIRST supported hit as the answer text.
    // We evaluate citation accuracy on this top hit only, since that's what
    // the user sees as the answer. The additional citations are supplementary.
    let top_hit = supported_hits[0];

    // Build reverse mapping: chunk_id -> document index
    let mut chunk_to_doc: HashMap<i64, usize> = HashMap::new();
    for (doc_idx, chunk_ids) in doc_to_chunks {
        for &chunk_id in chunk_ids {
            chunk_to_doc.insert(chunk_id, *doc_idx);
        }
    }

    if let Some(&doc_idx) = chunk_to_doc.get(&top_hit.source_id) {
        if let Some(doc) = documents.get(doc_idx) {
            // Extract the cited line range from the document
            let lines: Vec<&str> = doc.content.lines().collect();
            let start = (top_hit.line_start - 1).max(0) as usize;
            let end = (top_hit.line_end as usize).min(lines.len());

            if start < lines.len() && start <= end {
                let cited_text: String = lines[start..end].join("\n");

                // Check if cited text contains the expected snippet or query terms
                let is_correct = if let Some(expected) = &query.expected_snippet {
                    cited_text.to_lowercase().contains(&expected.to_lowercase())
                } else {
                    // Fallback: check if cited text shares significant terms with query
                    let query_lower = query.text.to_lowercase();
                    let query_terms: Vec<String> = query_lower
                        .split_whitespace()
                        .filter(|t| t.len() >= 3)
                        .map(|s| s.to_string())
                        .collect();
                    let cited_lower = cited_text.to_lowercase();
                    if query_terms.is_empty() {
                        false
                    } else {
                        let matches = query_terms.iter().filter(|t| cited_lower.contains(*t)).count();
                        matches as f32 / query_terms.len() as f32 > 0.3
                    }
                };

                return Ok(if is_correct { 1.0 } else { 0.0 });
            }
        }
    }

    Ok(0.0)
}

/// Abstention correctness: when all hits have Low confidence (or no hits),
/// the system should abstain (not_found = true).
/// For unanswerable queries, this is correct behavior.
/// For answerable queries, this is a false negative.
fn compute_abstention_correctness(hits: &[Hit], answerable: bool) -> bool {
    let has_high_or_medium = hits.iter().any(|h| {
        !matches!(h.confidence, crate::core::search::Confidence::Low)
    });

    if answerable {
        // Should find an answer (at least Medium confidence)
        has_high_or_medium
    } else {
        // Should abstain (no High/Medium confidence hits)
        !has_high_or_medium
    }
}

impl GoldenSet {
    /// Validate the golden set for consistency.
    fn validate(&self) -> Result<(), EvalError> {
        if self.queries.is_empty() {
            return Err(EvalError::InvalidGoldenSet("No queries in golden set".into()));
        }
        if self.documents.is_empty() {
            return Err(EvalError::InvalidGoldenSet("No documents in golden set".into()));
        }

        // Check that all qrels reference existing document IDs
        let doc_ids: std::collections::HashSet<i64> =
            self.documents.iter().enumerate().map(|(i, _)| i as i64).collect();

        for query in &self.queries {
            for &chunk_id in query.qrels.keys() {
                if !doc_ids.contains(&chunk_id) {
                    return Err(EvalError::InvalidGoldenSet(format!(
                        "Query {} references unknown chunk_id {}",
                        query.id, chunk_id
                    )));
                }
            }
        }

        Ok(())
    }
}

impl EvalReport {
    /// Save the report as JSON.
    pub fn save_json<P: AsRef<Path>>(&self, path: P) -> Result<(), EvalError> {
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    /// Check if the report passes CI gates.
    /// Returns (passed, failure_reasons).
    pub fn check_ci_gates(&self) -> (bool, Vec<String>) {
        let mut reasons = Vec::new();

        if self.macro_recall_at_10 < 0.75 {
            reasons.push(format!(
                "recall@10 {:.3} < 0.75",
                self.macro_recall_at_10
            ));
        }
        if self.macro_citation_accuracy < 0.95 {
            reasons.push(format!(
                "citation_accuracy {:.3} < 0.95",
                self.macro_citation_accuracy
            ));
        }

        (reasons.is_empty(), reasons)
    }
}

/// Create a SearchPipeline configured for evaluation with HashEmbedder.
pub fn make_eval_pipeline(conn: Connection) -> SearchPipeline {
    use crate::core::search::SharedEmbedder;
    use std::sync::Arc;

    struct HashEmbedder;
    impl QueryEmbedder for HashEmbedder {
        fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>> {
            let mut v = vec![0.0f32; crate::core::index::EMBEDDING_DIMS];
            for token in text
                .to_lowercase()
                .split(|c: char| !c.is_alphanumeric())
                .filter(|t| t.len() > 2)
            {
                let mut h = DefaultHasher::new();
                token.hash(&mut h);
                let idx = (h.finish() as usize) % v.len();
                v[idx] += 1.0;
            }
            let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
            if norm > 0.0 {
                for x in &mut v {
                    *x /= norm;
                }
            }
            Ok(v)
        }
    }

    let embedder: SharedEmbedder = Arc::new(HashEmbedder);
    SearchPipeline::new(conn, Some(embedder), None, SearchConfig::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::search::Confidence;

    #[test]
    fn test_recall_at_k() {
        let retrieved = vec![1, 2, 3, 4, 5];
        let relevant = vec![2, 4, 6];
        assert!((recall_at_k(&retrieved, &relevant, 5) - 2.0 / 3.0).abs() < 0.001);
        assert!((recall_at_k(&retrieved, &relevant, 3) - 1.0 / 3.0).abs() < 0.001);
        assert_eq!(recall_at_k(&retrieved, &[], 5), 1.0);
    }

    #[test]
    fn test_mrr() {
        let retrieved = vec![1, 2, 3, 4, 5];
        let relevant = vec![3, 5];
        assert!((compute_mrr(&retrieved, &relevant) - 1.0 / 3.0).abs() < 0.001);

        let relevant = vec![6, 7];
        assert_eq!(compute_mrr(&retrieved, &relevant), 0.0);
    }

    #[test]
    fn test_ndcg_at_k() {
        let retrieved = vec![1, 2, 3];
        let mut qrels = HashMap::new();
        qrels.insert(1, 2); // highly relevant
        qrels.insert(2, 1); // relevant
        qrels.insert(3, 0); // not relevant

        let ndcg = compute_ndcg_at_k(&retrieved, &qrels, 3);
        // DCG = 3/log2(2) + 1/log2(3) + 0/log2(4) = 3 + 0.63 = 3.63
        // IDCG = 3/log2(2) + 1/log2(3) = 3.63
        // nDCG = 1.0
        assert!((ndcg - 1.0).abs() < 0.01);

        // Swap order: relevant first, then highly relevant
        let retrieved = vec![2, 1, 3];
        let ndcg = compute_ndcg_at_k(&retrieved, &qrels, 3);
        // DCG = 1/log2(2) + 3/log2(3) = 1 + 1.89 = 2.89
        // nDCG = 2.89/3.63 = 0.796
        assert!((ndcg - 0.796).abs() < 0.01);
    }

    #[test]
    fn test_abstention_correctness() {
        // Answerable query with High confidence hit -> correct
        let hits = vec![Hit {
            source_id: 1,
            path: "test.txt".into(),
            source_path: "test.txt".into(),
            page: 0,
            line_start: 1,
            line_end: 1,
            anchor: None,
            snippet: "test".into(),
            content: "test".into(),
            score: 1.0,
            why_matched: "test".into(),
            confidence: Confidence::High,
            engine: crate::core::search::Engine::Local,
        }];
        assert!(compute_abstention_correctness(&hits, true));

        // Answerable query with only Low confidence -> incorrect (should have answered)
        let hits = vec![Hit {
            source_id: 1,
            path: "test.txt".into(),
            source_path: "test.txt".into(),
            page: 0,
            line_start: 1,
            line_end: 1,
            anchor: None,
            snippet: "test".into(),
            content: "test".into(),
            score: 0.1,
            why_matched: "test".into(),
            confidence: Confidence::Low,
            engine: crate::core::search::Engine::Local,
        }];
        assert!(!compute_abstention_correctness(&hits, true));

        // Unanswerable query with no hits -> correct
        let hits = vec![];
        assert!(compute_abstention_correctness(&hits, false));

        // Unanswerable query with Low confidence only -> correct
        let hits = vec![Hit {
            source_id: 1,
            path: "test.txt".into(),
            source_path: "test.txt".into(),
            page: 0,
            line_start: 1,
            line_end: 1,
            anchor: None,
            snippet: "test".into(),
            content: "test".into(),
            score: 0.1,
            why_matched: "test".into(),
            confidence: Confidence::Low,
            engine: crate::core::search::Engine::Local,
        }];
        assert!(compute_abstention_correctness(&hits, false));

        // Unanswerable query with Medium confidence -> incorrect (should have abstained)
        let hits = vec![Hit {
            source_id: 1,
            path: "test.txt".into(),
            source_path: "test.txt".into(),
            page: 0,
            line_start: 1,
            line_end: 1,
            anchor: None,
            snippet: "test".into(),
            content: "test".into(),
            score: 0.5,
            why_matched: "test".into(),
            confidence: Confidence::Medium,
            engine: crate::core::search::Engine::Local,
        }];
        assert!(!compute_abstention_correctness(&hits, false));
    }
}