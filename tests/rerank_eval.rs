//! Reranker evaluation gate (spec section 9). Downloads real models into
//! `$PAPER_HOME/models`, so it is ignored by default:
//!
//! `PAPER_HOME=<scratch> cargo test --release --test rerank_eval -- --ignored --nocapture`
//!
//! Every query is reranked (the language-coverage skip is bypassed) so the
//! comparison between rerankers is fair. `EVAL_RERANKERS` (comma-separated ids)
//! limits which rerankers run.

use paper::core::config::SearchConfig;
use paper::core::eval::{build_golden_index, load_golden_set, GoldenSet};
use paper::core::models::{
    LanguageCoverage, ModelManager, Reranker, LARGE_RERANKER, MMARCO_RERANKER,
    MULTILINGUAL_EMBEDDING, OPTIONAL_RERANKER,
};
use paper::core::search::{SearchPipeline, SharedEmbedder};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

struct Score {
    mrr: f64,
    hit1: f64,
    /// Per-query search time in seconds, in query order.
    secs: Vec<f64>,
}

fn run(set: &GoldenSet, embedder: &SharedEmbedder, reranker: Option<Arc<Reranker>>) -> Score {
    let (conn, doc_to_chunks) = build_golden_index(set, embedder.clone()).unwrap();
    let pipeline = SearchPipeline::new(
        conn,
        Some(embedder.clone()),
        reranker,
        SearchConfig::default(),
    );
    let (mut mrr, mut hit1, mut n, mut secs) = (0.0, 0.0, 0.0, vec![]);
    for q in set.queries.iter().filter(|q| q.answerable) {
        let start = Instant::now();
        let hits = pipeline.search(&q.text, &[]).unwrap();
        secs.push(start.elapsed().as_secs_f64());
        let relevant: Vec<i64> = q
            .qrels
            .iter()
            .filter(|(_, &r)| r > 0)
            .flat_map(|(&d, _)| {
                doc_to_chunks
                    .get(&(d as usize))
                    .cloned()
                    .unwrap_or_default()
            })
            .collect();
        let rank = hits
            .iter()
            .take(10)
            .position(|h| relevant.contains(&h.source_id));
        mrr += rank.map_or(0.0, |r| 1.0 / (r + 1) as f64);
        hit1 += (rank == Some(0)) as u8 as f64;
        n += 1.0;
    }
    Score {
        mrr: mrr / n,
        hit1: hit1 / n,
        secs,
    }
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

fn golden(name: &str) -> GoldenSet {
    load_golden_set(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/eval")
            .join(name),
    )
    .unwrap()
}

#[test]
#[ignore = "downloads up to ~4.4 GB of models; run with --release"]
fn rerank_gate() {
    let home = PathBuf::from(std::env::var("PAPER_HOME").expect("set PAPER_HOME to a scratch dir"));
    let manager = ModelManager::with_cache_dir(home.join("models"));
    let wanted: Vec<String> = std::env::var("EVAL_RERANKERS")
        .unwrap_or_else(|_| format!("{OPTIONAL_RERANKER},{MMARCO_RERANKER},{LARGE_RERANKER}"))
        .split(',')
        .map(String::from)
        .collect();
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(manager.ensure(MULTILINGUAL_EMBEDDING)).unwrap();
    let embedder: SharedEmbedder = Arc::new(manager.load_embedder(MULTILINGUAL_EMBEDDING).unwrap());

    let (en, multi) = (golden("golden.json"), golden("golden-multilingual.json"));
    let base = (run(&en, &embedder, None), run(&multi, &embedder, None));
    println!("| reranker | en MRR@10 | en hit@1 | non-en MRR@10 | non-en hit@1 | median rerank ms/query (en, non-en) |");
    println!("|---|---|---|---|---|---|");
    println!(
        "| none | {:.3} | {:.3} | {:.3} | {:.3} | - |",
        base.0.mrr, base.0.hit1, base.1.mrr, base.1.hit1
    );
    for id in wanted {
        rt.block_on(manager.ensure(&id)).unwrap();
        // Force Multilingual coverage so no query is skipped.
        let mut spec = manager.spec(&id).unwrap().clone();
        spec.coverage = LanguageCoverage::Multilingual;
        let reranker = Arc::new(Reranker::new(&manager.model_dir(&id), &spec).unwrap());
        let e = run(&en, &embedder, Some(reranker.clone()));
        let m = run(&multi, &embedder, Some(reranker));
        let lat = |with: &Score, without: &Score| {
            median(
                with.secs
                    .iter()
                    .zip(&without.secs)
                    .map(|(a, b)| (a - b) * 1000.0)
                    .collect(),
            )
        };
        println!(
            "| {id} | {:.3} | {:.3} | {:.3} | {:.3} | {:.0}, {:.0} |",
            e.mrr,
            e.hit1,
            m.mrr,
            m.hit1,
            lat(&e, &base.0),
            lat(&m, &base.1)
        );
    }
}
