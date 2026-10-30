use clap::{Parser, Subcommand};
use std::path::PathBuf;
mod commands;
use commands as cli;

#[derive(Parser)]
#[command(name = "harness3", about = "Harness³ local kernel control plane")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// Publish and inspect shared tasks; the control plane selects Presets dynamically.
    Board(cli::BoardArgs),
    /// Manage shared goals through the running daemon's authenticated HTTP API.
    Goals(cli::GoalArgs),
    Artifacts(cli::ArtifactArgs),
    Context(cli::ContextArgs),
    Tasks(cli::TaskArgs),
    Messages(cli::MessageArgs),
    Research(cli::ResearchArgs),
    /// Manage durable fleets and asynchronous operation receipts.
    Fleets(cli::FleetArgs),
    /// Inspect instances, manage lifecycle, and read durable sessions.
    Instances(cli::InstanceArgs),
    /// Submit UTF-8 input, inspect execution evidence, or cancel a Run.
    Runs(cli::RunArgs),
    /// List or publish immutable mother-agent Presets from JSON.
    Presets(cli::PresetArgs),
    /// Manage prompt bundles and opt-in local process Tool packages.
    Plugins(cli::PluginArgs),
    /// Inspect asynchronous operation completion.
    Operations(cli::OperationArgs),
    /// Inspect bounded state through the authenticated HTTP API.
    Snapshot(cli::Connection),
    /// Read up to 200 persisted events after a sequence cursor.
    Events {
        #[command(flatten)]
        connection: cli::Connection,
        #[arg(long, default_value = "0")]
        after: u64,
    },
    /// Serve outward MCP over newline-delimited stdio; read-only by default.
    Mcp {
        #[command(subcommand)]
        command: McpCommand,
    },
    /// Start the local daemon, persistent workers and embedded Web console.
    Up {
        #[arg(long, default_value = "7333")]
        port: u16,
        #[arg(long, default_value = ".harness3")]
        data_dir: PathBuf,
        #[arg(long)]
        codex_config_dir: Option<PathBuf>,
        #[arg(long, default_value = "gpt-6-luna")]
        model: String,
        #[arg(long)]
        demo: bool,
        /// Allow explicitly installed local process plugins to execute as this OS user.
        #[arg(long)]
        allow_process_plugins: bool,
    },
    /// Check the running daemon's public health endpoint.
    Doctor {
        #[arg(long, default_value = "7333")]
        port: u16,
    },
}
#[derive(Subcommand)]
enum McpCommand {
    Serve {
        #[command(flatten)]
        connection: cli::Connection,
        /// Expose state-changing tools to this MCP client using the local operator token.
        #[arg(long)]
        allow_write: bool,
        #[arg(long, default_value = "stdio", value_parser = ["stdio"])]
        transport: String,
    },
}
#[tokio::main]
async fn main() -> harness3_client::ClientResult<()> {
    match Cli::parse().command {
        Command::Board(args) => cli::board(args).await?,
        Command::Research(args) => cli::research(args).await?,
        Command::Messages(args) => cli::messages(args).await?,
        Command::Goals(args) => cli::goals(args).await?,
        Command::Artifacts(args) => cli::artifacts(args).await?,
        Command::Context(args) => cli::context(args).await?,
        Command::Tasks(args) => cli::tasks(args).await?,
        Command::Fleets(args) => cli::fleets(args).await?,
        Command::Instances(args) => cli::instances(args).await?,
        Command::Runs(args) => cli::runs(args).await?,
        Command::Presets(args) => cli::presets(args).await?,
        Command::Plugins(args) => cli::plugins(args).await?,
        Command::Operations(args) => cli::operations(args).await?,
        Command::Snapshot(connection) => cli::print(
            connection
                .client()?
                .execute(harness3_client::Command::Snapshot)
                .await?,
        )?,
        Command::Events { connection, after } => cli::print(
            connection
                .client()?
                .execute(harness3_client::Command::EventsList { after })
                .await?,
        )?,
        Command::Mcp {
            command:
                McpCommand::Serve {
                    connection,
                    allow_write,
                    ..
                },
        } => {
            harness3_server::mcp::McpServer::new(connection.client()?, allow_write)
                .serve(
                    tokio::io::BufReader::new(tokio::io::stdin()),
                    tokio::io::stdout(),
                )
                .await?;
        }
        Command::Doctor { port } => {
            let response = reqwest::get(format!("http://127.0.0.1:{port}/health")).await?;
            println!("{}", response.error_for_status()?.text().await?);
        }
        Command::Up {
            port,
            data_dir,
            codex_config_dir,
            model,
            demo,
            allow_process_plugins,
        } => {
            harness3_server::up(harness3_server::Options {
                port,
                data_dir,
                codex_config_dir,
                model,
                demo,
                allow_process_plugins,
            })
            .await?;
        }
    }
    Ok(())
}
