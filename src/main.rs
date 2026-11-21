use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "paper",
    version,
    about = "Local document finder with citations"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Search(paper::cli::SearchArgs),
    Answer(paper::cli::AnswerArgs),
    Index(paper::cli::IndexArgs),
    Aggregate(paper::cli::aggregate::AggregateArgs),
    Serve(paper::mcp::ServeArgs),
    Models(paper::cli::ModelsArgs),
    Doctor,
    #[cfg(feature = "daemon")]
    Daemon(paper::daemon::DaemonArgs),
    #[cfg(feature = "daemon")]
    DaemonControl(paper::cli::DaemonControlArgs),
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    let result = match cli.command {
        Commands::Search(args) => paper::cli::run_search(args).await,
        Commands::Answer(args) => paper::cli::run_answer(args).await,
        Commands::Index(args) => paper::cli::run_index(args).await,
        Commands::Aggregate(args) => paper::cli::run_aggregate(args).await,
        Commands::Serve(args) => paper::mcp::run_serve(args).await,
        Commands::Models(args) => paper::cli::run_models(args).await,
        Commands::Doctor => paper::cli::run_doctor().await,
        #[cfg(feature = "daemon")]
        Commands::Daemon(args) => paper::daemon::run_daemon(args).await,
        #[cfg(feature = "daemon")]
        Commands::DaemonControl(args) => paper::cli::run_daemon_control(args).await,
    };

    match result {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("paper: {error:#}");
            std::process::exit(paper::cli::EXIT_ERROR);
        }
    }
}
