pub mod server;

use anyhow::Result;
use clap::Args;

#[derive(Args)]
pub struct ServeArgs {
    /// Run as an MCP server over stdio (the only supported transport).
    #[arg(long)]
    pub mcp: bool,
    /// Directories agents are allowed to read. Empty means the configured roots.
    #[arg(long, short)]
    pub root: Vec<String>,
}

pub async fn run_serve(args: ServeArgs) -> Result<i32> {
    if args.mcp {
        server::run_mcp(args.root).await
    } else {
        anyhow::bail!("only --mcp (stdio) is supported; usage: paper serve --mcp --root <dir>")
    }
}
