use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "paper",
    version,
    about = "Local document finder with citations",
    long_about = "Paper searches your own files and answers with the file, page and line \n\
                  each fact came from. No cloud, no API keys, no telemetry.\n\n\
                  Install once and it keeps itself ready: a background service indexes \n\
                  your folders, and the search models (including OCR) download on first run.",
    after_help = "Examples:\n\
                    paper search \"notice period\"        find matching passages\n\
                    paper answer \"when can I renew?\"     answer with quoted evidence\n\
                    paper index --root ~/Documents       add a folder to the index\n\
                    paper serve --mcp                    let Claude or Cursor search your files\n\
                    paper update                         install the latest release"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Find files and passages that match a query, with exact citations.
    ///
    /// Prints every hit with its file, page or cell anchor and a relevance
    /// score. Use it when you want to see what exists.
    ///
    /// Example: paper search "notice period" --limit 5
    Search(paper::cli::SearchArgs),

    /// Ask a question and get an answer made only of quoted sentences.
    ///
    /// Each sentence in the answer carries the file and page it came from, so
    /// you can check it in one step. Use it instead of reading documents.
    ///
    /// Example: paper answer "what is the notice period for ending the lease?"
    Answer(paper::cli::AnswerArgs),

    /// Add files or folders to the search index.
    ///
    /// Recursive, incremental and safe to re-run: unchanged files are skipped.
    /// A background service already does this for your common folders.
    ///
    /// Example: paper index --root ~/Documents/contracts
    Index(paper::cli::IndexArgs),

    /// Summarise every source that touches a topic.
    ///
    /// Use it when one question spans many files and you want the whole picture
    /// with one citation per claim.
    ///
    /// Example: paper aggregate "data retention policy"
    Aggregate(paper::cli::aggregate::AggregateArgs),

    /// Run the MCP server so Claude, Cursor or any agent can search your files.
    ///
    /// Speaks MCP over stdio. Exposes search, read, answer and index-control
    /// tools. Point the client's MCP config at this command.
    ///
    /// Example: paper serve --mcp
    Serve(paper::mcp::ServeArgs),

    /// List, install or remove the local models.
    ///
    /// The defaults download on their own; this is for choosing a different
    /// embedding model or freeing disk space.
    ///
    /// Example: paper models install multilingual-e5-small
    Models(paper::cli::ModelsArgs),

    /// Show where your data lives and what is healthy.
    ///
    /// Reports the index size, installed models, OCR state and whether the
    /// background service is running. Good first stop when a search looks odd.
    ///
    /// Example: paper doctor
    Doctor,

    /// Install the newest release, or check whether one exists.
    ///
    /// Runs the same signed installer a fresh install uses, so updates and first
    /// installs never drift apart.
    ///
    /// Example: paper update --check
    Update(paper::cli::UpdateArgs),

    #[cfg(feature = "daemon")]
    /// Run the background indexer in the foreground (for debugging).
    ///
    /// Normally the OS service starts this for you; run it directly to watch
    /// what it does.
    ///
    /// Example: paper daemon --verbose
    Daemon(paper::daemon::DaemonArgs),

    #[cfg(feature = "daemon")]
    /// Install, start, stop or inspect the background indexing service.
    ///
    /// Manages a launchd agent on macOS or a systemd user unit on Linux.
    ///
    /// Example: paper daemon-control status
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
            Commands::Update(_) => "update",
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
        Commands::Update(args) => paper::cli::update::run(args).await,
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
