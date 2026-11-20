use super::{ModelsArgs, ModelsCommand, EXIT_ERROR, EXIT_FOUND};
use crate::core::models::ModelManager;
use anyhow::Result;

pub async fn run(args: ModelsArgs) -> Result<i32> {
    let manager = ModelManager::new();
    match args.command {
        ModelsCommand::List => {
            println!("{:<22} {:>8}  {:<9} {}", "MODEL", "SIZE", "INSTALLED", "DESCRIPTION");
            for m in manager.list() {
                println!(
                    "{:<22} {:>6}MB  {:<9} {}",
                    m.id,
                    m.size_mb,
                    if m.installed { "yes" } else { "no" },
                    m.name
                );
            }
        }
        ModelsCommand::Install { model } => {
            let path = manager.ensure(&model).await?;
            println!("installed {model} at {}", path.display());
        }
        ModelsCommand::Remove { model } => {
            if manager.spec(&model).is_err() {
                eprintln!("unknown model: {model}");
                return Ok(EXIT_ERROR);
            }
            manager.remove(&model).await?;
            println!("removed {model}");
        }
    }
    Ok(EXIT_FOUND)
}
