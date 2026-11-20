//! Offline evaluation benchmark for retrieval quality.
//!
//! Runs the golden set evaluation with HashEmbedder and outputs a JSON report.
//! CI gate: fails if recall@10 < 0.75 or citation_accuracy < 0.95.
//!
//! Run with: `cargo bench --bench eval`

use criterion::{criterion_group, criterion_main, Criterion};
use paper::core::eval::{
    load_golden_set, make_eval_pipeline, run_eval, build_golden_index, EvalError, EvalReport,
};
use paper::core::search::SharedEmbedder;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::hint::black_box;
use std::path::PathBuf;
use std::sync::Arc;

fn bench_eval(c: &mut Criterion) {
    // Load the golden set
    let golden_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("eval")
        .join("golden.json");

    let golden_set = load_golden_set(&golden_path).expect("Failed to load golden set");

    // Build the index once
    let (conn, doc_to_chunks) = build_golden_index(&golden_set, make_eval_embedder())
        .expect("Failed to build eval index");
    let pipeline = make_eval_pipeline(conn);

    c.bench_function("eval/full_golden_set", |b| {
        b.iter(|| {
            let report = run_eval(&pipeline, black_box(&golden_set), black_box(&doc_to_chunks))
                .expect("Evaluation failed");
            black_box(report)
        })
    });

    // Also run a one-shot to generate the report file
    let report = run_eval(&pipeline, &golden_set, &doc_to_chunks).expect("Evaluation failed");
    save_report(&report).expect("Failed to save report");
    check_ci_gates(&report).expect("CI gates failed");
}

fn make_eval_embedder() -> SharedEmbedder {
    struct HashEmbedder;
    impl paper::core::search::QueryEmbedder for HashEmbedder {
        fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>> {
            let mut v = vec![0.0f32; paper::core::index::EMBEDDING_DIMS];
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
    Arc::new(HashEmbedder)
}

fn save_report(report: &EvalReport) -> Result<(), EvalError> {
    let output_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("criterion");
    std::fs::create_dir_all(&output_dir)?;
    let output_path = output_dir.join("eval-report.json");
    report.save_json(&output_path)?;
    println!("Evaluation report saved to {}", output_path.display());
    print_report_summary(report);
    Ok(())
}

fn print_report_summary(report: &EvalReport) {
    println!("\n=== Evaluation Report Summary ===");
    println!("Total queries: {}", report.total_queries);
    println!("Answerable: {}", report.answerable_queries);
    println!("Unanswerable: {}", report.unanswerable_queries);
    println!();
    println!("Macro Recall@10:     {:.4}", report.macro_recall_at_10);
    println!("Macro Recall@5:      {:.4}", report.macro_recall_at_5);
    println!("Macro MRR:           {:.4}", report.macro_mrr);
    println!("Macro nDCG@10:       {:.4}", report.macro_ndcg_at_10);
    println!("Macro Citation Acc:  {:.4}", report.macro_citation_accuracy);
    println!("Macro Abstention:    {:.4}", report.macro_abstention_correctness);
    println!();

    // Per-query details for failed queries
    for q in &report.per_query {
        if q.recall_at_10 < 1.0 || q.citation_accuracy < 1.0 || !q.abstention_correct {
            println!(
                "  [{}] recall@10={:.2} citation={:.2} abstain_correct={} retrieved={} relevant={}",
                q.query_id,
                q.recall_at_10,
                q.citation_accuracy,
                q.abstention_correct,
                q.retrieved_count,
                q.relevant_count
            );
        }
    }
}

fn check_ci_gates(report: &EvalReport) -> Result<(), String> {
    let (passed, reasons) = report.check_ci_gates();
    if !passed {
        eprintln!("\n❌ CI GATES FAILED:");
        for reason in &reasons {
            eprintln!("  - {}", reason);
        }
        return Err("CI gates failed".to_string());
    }
    println!("\n✅ All CI gates passed!");
    Ok(())
}

criterion_group!(benches, bench_eval);
criterion_main!(benches);