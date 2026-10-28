use clap::{Parser, Subcommand};
use fs2::FileExt;
use harness3::CancellationToken;
use harness3_platform::{
    api::{App, router},
    domain::{Preset, Protocol},
    models::Models,
    runtime,
    store::Store,
};
use std::{collections::HashMap, fs::OpenOptions, path::PathBuf, sync::Arc};
use tokio::sync::Mutex;
mod cli;

#[derive(Parser)]
#[command(name = "harness3", about = "Harness³ local kernel control plane")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
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
async fn main() -> harness3_platform::client::ClientResult<()> {
    match Cli::parse().command {
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
                .execute(harness3_platform::client::Command::Snapshot)
                .await?,
        )?,
        Command::Events { connection, after } => cli::print(
            connection
                .client()?
                .execute(harness3_platform::client::Command::EventsList { after })
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
            harness3_platform::mcp::McpServer::new(connection.client()?, allow_write)
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
            std::fs::create_dir_all(&data_dir)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&data_dir, std::fs::Permissions::from_mode(0o700))?;
            }
            let lock = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(data_dir.join("daemon.lock"))?;
            lock.try_lock_exclusive()
                .map_err(|_| "another daemon already owns this data directory")?;
            let token_path = data_dir.join("token");
            let token = match std::fs::read_to_string(&token_path) {
                Ok(token) => token.trim().to_owned(),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    use std::io::Write;
                    let token = format!(
                        "{}{}",
                        uuid::Uuid::new_v4().simple(),
                        uuid::Uuid::new_v4().simple()
                    );
                    let mut options = OpenOptions::new();
                    options.write(true).create_new(true);
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::OpenOptionsExt;
                        options.mode(0o600);
                    }
                    let mut file = options.open(&token_path)?;
                    file.write_all(token.as_bytes())?;
                    token
                }
                Err(error) => return Err(error.into()),
            };
            if token.len() != 64 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err("invalid local token file".into());
            }
            let store = Store::open(&data_dir.join("platform.sqlite"))?;
            let mut models = Models::from_env();
            if let Some(root) = codex_config_dir {
                models.load_codex_responses(&root, &model)?;
            }
            store.publish(Preset {
                name: "demo-inspector".into(),
                version: "1.0.0".into(),
                purpose: "无需模型账号的内核执行验证".into(),
                objective: "通过真实工具回执验证输入统计与会话恢复边界".into(),
                instructions: "Inspect the input with text.inspect and summarize its evidence."
                    .into(),
                protocol: Protocol::Demo,
                plugin: None,
            })?;
            store.publish_definition(serde_json::json!({
                "name":"demo-collaborator","version":"1.0.0",
                "extends":{"name":"demo-inspector","version":"1.0.0"},
                "purpose":"无需模型账号的多实例共享上下文验证",
                "plugins":[{"id":"platform.context","version":"1.0.0"}]
            }))?;
            store.publish_definition(serde_json::json!({
                "name":"demo-delegator","version":"1.0.0",
                "extends":{"name":"demo-inspector","version":"1.0.0"},
                "purpose":"确定性演示：递归分工与原 Agent 续作",
                "plugins":[{"id":"platform.delegation","version":"1.0.0"}]
            }))?;
            if !demo && models.responses.is_some() {
                store.publish(Preset{name:"researcher".into(),version:"1.0.0".into(),purpose:"通用研究与分析".into(),objective:"依据可用证据回答用户问题；需要文本统计时使用 text.inspect".into(),instructions:"You are a careful research agent. Use tools when needed. Distinguish evidence from inference. Reply in the user's language. Never claim tool results without receipts.".into(),protocol:Protocol::OpenaiResponses,plugin:None})?;
                store.publish_definition(serde_json::json!({
                    "name":"research-collaborator","version":"1.0.0",
                    "extends":{"name":"researcher","version":"1.0.0"},
                    "purpose":"共享证据与对照结论的协作研究",
                    "plugins":[{"id":"platform.context","version":"1.0.0"}]
                }))?;
            }
            if !demo && models.responses.is_some() {
                store.publish_definition(serde_json::json!({
                    "name":"research-delegator","version":"1.0.0",
                    "extends":{"name":"researcher","version":"1.0.0"},
                    "purpose":"按批准角色分工、核对贡献并继续完成原任务",
                    "plugins":[{"id":"platform.delegation","version":"1.0.0"},{"id":"platform.context","version":"1.0.0"}]
                }))?;
            }
            let plugins = if allow_process_plugins {
                harness3_platform::plugins::ProcessHost::enabled(
                    store.clone(),
                    data_dir.join("plugins"),
                )?
            } else {
                harness3_platform::plugins::ProcessHost::disabled(store.clone())
            };
            runtime::recover(&store).await?;
            let cancellations = Arc::new(Mutex::new(HashMap::new()));
            let shutdown = CancellationToken::new();
            let listener =
                tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await?;
            let actual = listener.local_addr()?;
            store.record(
                "platform.started",
                "daemon",
                serde_json::json!({"version":env!("CARGO_PKG_VERSION")}),
            )?;
            println!(
                "Harness³ kernel preview: http://{actual}\nData: {}\nLogin / API token file: {}\nWorker slots: 4 · instance quota: 64\nOpen the console and paste the local token to log in. Ctrl-C drains active runs.",
                data_dir.display(),
                token_path.display()
            );
            let worker = tokio::spawn(runtime::worker_loop_with_plugins(
                store.clone(),
                models.clone(),
                cancellations.clone(),
                shutdown.clone(),
                plugins.clone(),
            ));
            let app = router(App {
                store,
                token,
                cancellations,
                models,
                plugins,
            });
            let stop = shutdown.clone();
            let signal = async move {
                let _ = tokio::signal::ctrl_c().await;
                stop.cancel();
            };
            let result = axum::serve(listener, app)
                .with_graceful_shutdown(signal)
                .await;
            shutdown.cancel();
            worker.await?;
            result?;
            drop(lock);
        }
    }
    Ok(())
}
