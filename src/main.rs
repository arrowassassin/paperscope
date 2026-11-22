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

impl Commands {
    /// Subcommand name, used by first-run automation to decide whether it should
    /// run (it must not interfere with daemon management or diagnostics).
    #[cfg_attr(not(feature = "daemon"), allow(dead_code))]
    fn name(&self) -> &'static str {
        match self {
            Commands::Search(_) => "search",
            Commands::Answer(_) => "answer",
            Commands::Index(_) => "index",
            Commands::Aggregate(_) => "aggregate",
            Commands::Serve(_) => "serve",
            Commands::Models(_) => "models",
            Commands::Doctor => "doctor",
            #[cfg(feature = "daemon")]
            Commands::Daemon(_) => "daemon",
            #[cfg(feature = "daemon")]
            Commands::DaemonControl(_) => "daemon-control",
        }
    }
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

    // Install-once, never-configure: the first ordinary command registers the
    // background indexer. It never fails the command and never waits on the
    // network (the daemon downloads models and indexes).
    #[cfg(feature = "daemon")]
    paper::bootstrap::maybe_autostart(cli.command.name()).await;

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
