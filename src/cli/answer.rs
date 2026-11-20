use super::{AnswerArgs, Workspace, EXIT_FOUND, EXIT_NOT_FOUND};
use crate::core::answer::Answer;
use anyhow::Result;

pub async fn run(args: AnswerArgs) -> Result<i32> {
    if args.polish {
        eprintln!("note: --polish needs a generative engine; none is configured, returning the verbatim extract");
    }
    let ws = Workspace::new();
    let roots = ws.search_roots(&args.root);
    let hits = ws.pipeline()?.search(&args.question, &roots)?;
    let answer = Answer::from_hits(&args.question, &hits);

    if args.json {
        println!("{}", serde_json::to_string_pretty(&answer)?);
    } else {
        println!("{}", answer.render());
    }
    Ok(if answer.not_found { EXIT_NOT_FOUND } else { EXIT_FOUND })
}
