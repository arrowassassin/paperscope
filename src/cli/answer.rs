use super::{terminal_safe, AnswerArgs, Workspace, EXIT_FOUND, EXIT_NOT_FOUND};
use crate::core::answer::Answer;
use crate::core::config::Config;
use crate::core::models::chat::ChatClient;
use crate::core::models::polish::{self, Extract, PolishEngine};
use anyhow::Result;
use std::time::Duration;

/// How many local passages are offered to the polishing engine.
const MAX_POLISH_EXTRACTS: usize = 5;

pub async fn run(args: AnswerArgs) -> Result<i32> {
    let ws = Workspace::new();
    let roots = ws.search_roots(&args.root);
    let filters = args.filters()?;
    let hits = ws
        .pipeline()?
        .search_with(&args.question, &roots, &filters)?;
    let mut answer = Answer::from_hits(&args.question, &hits);
    ws.note_setup_in_progress();
    crate::core::effective::note_deprecations(ws.remote.as_ref(), &ws.effective);

    if args.polish && !answer.not_found {
        match polish_answer(&args, &hits).await {
            Ok((text, engine)) => {
                answer.text = text;
                answer.engine = engine;
            }
            Err(err) => {
                // Any failure keeps the verbatim extract: a polished answer is a
                // convenience, never a requirement, and the citations stay exact.
                eprintln!("note: polish failed ({err:#}); returning the verbatim extract");
            }
        }
    } else if args.polish {
        eprintln!("note: nothing to polish; returning the verbatim extract");
    }

    if args.json {
        println!("{}", serde_json::to_string_pretty(&answer)?);
    } else {
        println!("{}", terminal_safe(&answer.render()));
    }
    Ok(if answer.not_found {
        EXIT_NOT_FOUND
    } else {
        EXIT_FOUND
    })
}

/// Ask the configured engine to rewrite the extract.
///
/// Returns the polished text and a label naming the engine, so the answer
/// always says which engine produced it.
async fn polish_answer(
    args: &AnswerArgs,
    hits: &[crate::core::search::Hit],
) -> Result<(String, String)> {
    if offline() {
        anyhow::bail!("PAPER_OFFLINE is set, so no engine may be contacted");
    }
    let engine = PolishEngine::parse(&args.engine)?;
    let config = Config::load()?;

    let (base_url, api_key, model, timeout) = match engine {
        PolishEngine::Local => (
            config.local_engine.base_url.clone(),
            None,
            config.local_engine.model.clone(),
            config.local_engine.timeout_secs,
        ),
        PolishEngine::OpenRouter => {
            let key = if config.openrouter.api_key.is_empty() {
                std::env::var("OPENROUTER_API_KEY").unwrap_or_default()
            } else {
                config.openrouter.api_key.clone()
            };
            if key.is_empty() {
                anyhow::bail!(
                    "no OpenRouter API key: set OPENROUTER_API_KEY or [openrouter] api_key in the config"
                );
            }
            (
                "https://openrouter.ai".to_string(),
                Some(key),
                config.openrouter.model.clone(),
                config.openrouter.timeout_secs,
            )
        }
    };

    if model.trim().is_empty() {
        anyhow::bail!("no model configured for the {} engine", engine.as_str());
    }

    // The extracts are the passages the answer may draw on: the same hits the
    // extractive answer cites, with their exact references.
    let extracts: Vec<Extract> = hits
        .iter()
        .filter(|hit| !matches!(hit.confidence, crate::core::search::Confidence::Low))
        .take(MAX_POLISH_EXTRACTS)
        .map(|hit| Extract {
            reference: citation_reference(hit),
            text: hit.content.clone(),
        })
        .collect();
    if extracts.is_empty() {
        anyhow::bail!("no extract with enough confidence to polish");
    }

    let client = ChatClient::new(
        &base_url,
        api_key,
        &model,
        Duration::from_secs(timeout.max(1)),
    )?;
    let text = polish::polish(&client, engine, &args.question, &extracts).await?;
    let label = format!("{}/{}", engine.as_str(), client.model());
    Ok((text, label))
}

/// `path:line_start-line_end`, matching what the extractive answer prints.
fn citation_reference(hit: &crate::core::search::Hit) -> String {
    let mut reference = hit.path.clone();
    if let Some(anchor) = &hit.anchor {
        reference.push_str(&format!(" [{anchor}]"));
    }
    if hit.line_start > 0 {
        reference.push_str(&format!(":{}-{}", hit.line_start, hit.line_end));
    } else if hit.page > 0 {
        reference.push_str(&format!(":p.{}", hit.page));
    }
    reference
}

fn offline() -> bool {
    std::env::var("PAPER_OFFLINE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}
