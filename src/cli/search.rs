use super::{SearchArgs, Workspace, EXIT_FOUND, EXIT_MODEL_MISSING, EXIT_NOT_FOUND};
use crate::core::models::DEFAULT_EMBEDDING;
use anyhow::Result;

pub async fn run(args: SearchArgs) -> Result<i32> {
    let ws = Workspace::new();
    if args.semantic && !ws.embedding_model_installed() {
        eprintln!(
            "embedding model is not installed; run `paper models install {DEFAULT_EMBEDDING}` \
             or drop --semantic for keyword-only search"
        );
        return Ok(EXIT_MODEL_MISSING);
    }

    let roots = ws.search_roots(&args.root);
    let filters = args.filters()?;
    let pipeline = ws.pipeline()?;
    let mut hits = pipeline.search_with(&args.query, &roots, &filters)?;
    hits.truncate(args.limit);

    if args.json {
        println!("{}", serde_json::to_string_pretty(&hits)?);
    } else if hits.is_empty() {
        println!("No matches for {:?}.", args.query);
        if !ws.embedding_model_installed() {
            eprintln!("hint: install the embedding model for semantic search (paper models install {DEFAULT_EMBEDDING})");
        }
    } else {
        for hit in &hits {
            let location = match &hit.anchor {
                Some(anchor) => format!("{} [{anchor}]", hit.path),
                None => format!("{}:{}-{}", hit.path, hit.line_start, hit.line_end),
            };
            println!("{location}");
            if let Some(section) = &hit.section {
                println!("  § {section}");
            }
            println!("  {}", hit.snippet);
            println!("  score {:.2}  {}", hit.score, hit.why_matched);
        }
    }
    Ok(if hits.is_empty() {
        EXIT_NOT_FOUND
    } else {
        EXIT_FOUND
    })
}
