//! Retrieval micro-benchmarks. Run with `cargo bench` (release profile).
//!
//! These measure the local, model-free path — FTS5 keyword retrieval and
//! chunking — because that is what every install can do. Semantic latency is
//! dominated by the Candle forward pass and is measured by `paper doctor`.

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use paper::core::index::{chunk_text, Connection, Indexer};
use paper::core::search::{tier0_search, QueryEmbedder};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::hint::black_box;

/// Same deterministic bag-of-words embedder the integration tests use.
struct HashEmbedder;

impl QueryEmbedder for HashEmbedder {
    fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        let mut v = vec![0.0f32; paper::core::index::EMBEDDING_DIMS];
        let dims = v.len();
        for token in text
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|t| t.len() > 2)
        {
            let mut h = DefaultHasher::new();
            token.hash(&mut h);
            let idx = (h.finish() as usize) % dims;
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

fn fixture() -> (tempfile::TempDir, Indexer) {
    let dir = tempfile::tempdir().unwrap();
    let docs = dir.path().join("docs");
    std::fs::create_dir(&docs).unwrap();
    for i in 0..500 {
        let body = format!(
            "Document {i}\n\
             The fridge warranty covers 24 months from 2025-01-15 and expires on 2027-01-15.\n\
             Serial number FR-{i:04}. Invoice total $1,{i:03}.99 was paid in full.\n\
             Contact support for the dishwasher manual and the tax return deadline.\n"
        );
        std::fs::write(docs.join(format!("doc{i}.txt")), body).unwrap();
    }
    let conn = Connection::open(&dir.path().join("chunks.db")).unwrap();
    let indexer = Indexer::new(conn, Some(std::sync::Arc::new(HashEmbedder))).unwrap();
    indexer.index_roots(&[docs]).unwrap();
    (dir, indexer)
}

fn bench_keyword(c: &mut Criterion) {
    let (_dir, indexer) = fixture();
    let conn = indexer.connection();
    c.bench_function("tier0/exact serial number", |b| {
        b.iter(|| tier0_search(conn, black_box("FR-0042"), &[], 10).unwrap())
    });
    c.bench_function("tier0/natural language", |b| {
        b.iter(|| tier0_search(conn, black_box("when does the fridge warranty expire"), &[], 10).unwrap())
    });
}

fn bench_chunking(c: &mut Criterion) {
    let text = std::fs::read_to_string("README.md").unwrap_or_default();
    let text = if text.is_empty() {
        "warranty line\n".repeat(2_000)
    } else {
        text
    };
    c.bench_function("chunk_text/400 words", |b| {
        b.iter_batched(
            || text.clone(),
            |text| black_box(chunk_text(&text, 400, 40)),
            BatchSize::SmallInput,
        )
    });
}

criterion_group!(benches, bench_keyword, bench_chunking);
criterion_main!(benches);
