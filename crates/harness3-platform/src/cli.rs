use clap::{Args, Subcommand, ValueEnum};
use harness3_platform::client::{ClientResult, Command, PlatformClient};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::{io::Read, path::PathBuf};

#[derive(Args)]
pub struct Connection {
    #[arg(long, default_value = "7333", global = true)]
    pub port: u16,
    #[arg(long, default_value = ".harness3", global = true)]
    pub data_dir: PathBuf,
}
impl Connection {
    pub fn client(&self) -> ClientResult<PlatformClient> {
        PlatformClient::local(self.port, &self.data_dir)
    }
}
macro_rules! resource {
    ($args:ident, $action:ident { $($variants:tt)* }) => {
        #[derive(Args)]
        pub struct $args {
            #[command(flatten)]
            connection: Connection,
            #[command(subcommand)]
            action: $action,
        }
        #[derive(Subcommand)]
        enum $action { $($variants)* }
    };
}
resource!(ResearchArgs, ResearchAction {
    List,
    Show {id:String},
    /// Start a bounded planner -> parallel researchers -> synthesis team. May call paid models.
    Create {#[arg(long)] file:PathBuf,#[arg(long)] key:String},
    Cancel {id:String},
});
pub async fn research(args: ResearchArgs) -> ClientResult<()> {
    let command = match args.action {
        ResearchAction::List => Command::ResearchList,
        ResearchAction::Show { id } => Command::ResearchShow { id },
        ResearchAction::Create { file, key } => Command::ResearchCreate {
            input: read_json(file)?,
            key,
        },
        ResearchAction::Cancel { id } => Command::ResearchCancel { id },
    };
    print(args.connection.client()?.execute(command).await?)
}
resource!(MessageArgs, MessageAction {
    List {#[arg(long,default_value="0")] after:i64,#[arg(long)] limit:Option<usize>,#[arg(long)] instance_id:Option<String>,#[arg(long)] direction:Option<String>},
    Show {id:String},
    /// Send JSON task data; dispatch can call a paid model when the recipient is ready.
    Send {id:String,#[arg(long)] file:PathBuf,#[arg(long)] key:String},
    Action {id:String,#[arg(value_enum)] action:MessageOperation,#[arg(long)] note:String,#[arg(long)] key:String},
});
#[derive(Clone, ValueEnum)]
enum MessageOperation {
    Discard,
    Resolve,
}
pub async fn messages(args: MessageArgs) -> ClientResult<()> {
    let command = match args.action {
        MessageAction::List {
            after,
            limit,
            instance_id,
            direction,
        } => Command::MessagesList {
            after,
            limit,
            instance_id,
            direction,
        },
        MessageAction::Show { id } => Command::MessagesShow { id },
        MessageAction::Send { id, file, key } => Command::MessagesSend {
            id,
            input: read_json(file)?,
            key,
        },
        MessageAction::Action {
            id,
            action,
            note,
            key,
        } => Command::MessagesAction {
            id,
            action: match action {
                MessageOperation::Discard => "discard",
                MessageOperation::Resolve => "resolve",
            }
            .into(),
            note,
            key,
        },
    };
    print(args.connection.client()?.execute(command).await?)
}
resource!(GoalArgs, GoalAction {
    List,
    Show { id: String },
    Create { #[arg(long)] file: PathBuf, #[arg(long)] key: String },
    Cancel { id: String },
    Accept { id: String, #[arg(long)] review: String, #[arg(long,default_value="1")] expected_round:i64 },
    /// Repeat the full graph with a reviewer note and fresh managed instances by default.
    Rework {id:String,#[arg(long)] file:PathBuf,#[arg(long)] key:String},
});
resource!(FleetArgs, FleetAction {
    List,
    Show { id: String },
    Create { #[arg(long)] file: PathBuf, #[arg(long)] key: String },
    Scale { id: String, #[arg(long)] count: usize, #[arg(long)] expected_revision: i64, #[arg(long)] key: String },
    Terminate { id: String, #[arg(long)] expected_revision: i64, #[arg(long)] key: String },
});
resource!(InstanceArgs, InstanceCommand {
    List,
    Action { id: String, #[arg(value_enum)] action: InstanceAction },
    Session { id: String },
});
resource!(RunArgs, RunAction {
    Show { id: String },
    Submit { id: String, #[arg(long)] file: PathBuf, #[arg(long)] key: String },
    Cancel { id: String },
});
resource!(PresetArgs, PresetAction {
    List,
    Publish { #[arg(long)] file: PathBuf },
});
resource!(PluginArgs, PluginAction {
    List,
    Install { #[arg(long)] file: PathBuf },
    /// Install and health-check a local package (requires daemon opt-in).
    InstallProcess { #[arg(long)] source_dir: String },
    ShowProcess {id:String,version:String},
    ActionProcess {id:String,version:String,#[arg(value_enum)] action: ProcessAction},
});
resource!(OperationArgs, OperationAction { Show { id: String } });
#[derive(Clone, ValueEnum)]
enum ProcessAction {
    Enable,
    Disable,
    Check,
}
#[derive(Clone, ValueEnum)]
enum InstanceAction {
    Pause,
    Resume,
    Drain,
    Terminate,
}
impl InstanceAction {
    fn name(&self) -> &'static str {
        match self {
            Self::Pause => "pause",
            Self::Resume => "resume",
            Self::Drain => "drain",
            Self::Terminate => "terminate",
        }
    }
}
pub async fn goals(args: GoalArgs) -> ClientResult<()> {
    use GoalAction::*;
    let command = match args.action {
        List => Command::GoalsList,
        Show { id } => Command::GoalsShow { id },
        Create { file, key } => Command::GoalsCreate {
            plan: read_json(file)?,
            key,
        },
        Cancel { id } => Command::GoalsCancel { id },
        Accept {
            id,
            review,
            expected_round,
        } => Command::GoalsAccept {
            id,
            review,
            expected_round,
        },
        Rework { id, file, key } => Command::GoalsRework {
            id,
            input: read_json(file)?,
            key,
        },
    };
    print(args.connection.client()?.execute(command).await?)
}
pub async fn fleets(args: FleetArgs) -> ClientResult<()> {
    use FleetAction::*;
    let command = match args.action {
        List => Command::FleetsList,
        Show { id } => Command::FleetsShow { id },
        Create { file, key } => Command::FleetsCreate {
            plan: read_json(file)?,
            key,
        },
        Scale {
            id,
            count,
            expected_revision,
            key,
        } => Command::FleetsScale {
            id,
            count,
            expected_revision,
            key,
        },
        Terminate {
            id,
            expected_revision,
            key,
        } => Command::FleetsTerminate {
            id,
            expected_revision,
            key,
        },
    };
    print(args.connection.client()?.execute(command).await?)
}
pub async fn instances(args: InstanceArgs) -> ClientResult<()> {
    let command = match args.action {
        InstanceCommand::List => Command::InstancesList,
        InstanceCommand::Action { id, action } => Command::InstancesAction {
            id,
            action: action.name().into(),
        },
        InstanceCommand::Session { id } => Command::InstancesSession { id },
    };
    print(args.connection.client()?.execute(command).await?)
}
pub async fn runs(args: RunArgs) -> ClientResult<()> {
    let command = match args.action {
        RunAction::Show { id } => Command::RunsShow { id },
        RunAction::Submit { id, file, key } => Command::RunsCreate {
            id,
            text: String::from_utf8(read_file(file)?)?,
            key,
        },
        RunAction::Cancel { id } => Command::RunsCancel { id },
    };
    print(args.connection.client()?.execute(command).await?)
}
pub async fn presets(args: PresetArgs) -> ClientResult<()> {
    let command = match args.action {
        PresetAction::List => Command::PresetsList,
        PresetAction::Publish { file } => Command::PresetsPublish {
            preset: read_json(file)?,
        },
    };
    print(args.connection.client()?.execute(command).await?)
}
pub async fn plugins(args: PluginArgs) -> ClientResult<()> {
    let command = match args.action {
        PluginAction::List => Command::PluginsList,
        PluginAction::InstallProcess { source_dir } => {
            Command::ProcessPluginsInstall { source_dir }
        }
        PluginAction::ShowProcess { id, version } => Command::ProcessPluginsShow { id, version },
        PluginAction::ActionProcess {
            id,
            version,
            action,
        } => Command::ProcessPluginsAction {
            id,
            version,
            action: match action {
                ProcessAction::Enable => "enable",
                ProcessAction::Disable => "disable",
                ProcessAction::Check => "check",
            }
            .into(),
        },
        PluginAction::Install { file } => Command::PluginsInstall {
            bundle: read_json(file)?,
        },
    };
    print(args.connection.client()?.execute(command).await?)
}
pub async fn operations(args: OperationArgs) -> ClientResult<()> {
    let OperationAction::Show { id } = args.action;
    print(
        args.connection
            .client()?
            .execute(Command::OperationsShow { id })
            .await?,
    )
}
fn read_file(path: PathBuf) -> ClientResult<Vec<u8>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(128 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 128 * 1024 {
        return Err("input exceeds 128 KiB".into());
    }
    Ok(bytes)
}
fn read_json<T: DeserializeOwned>(path: PathBuf) -> ClientResult<T> {
    Ok(serde_json::from_slice(&read_file(path)?)?)
}
pub fn print(value: Value) -> ClientResult<()> {
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

resource!(ArtifactArgs, ArtifactAction {
    List {#[arg(long)] goal_id:Option<String>,#[arg(long,default_value="0")] after:i64,#[arg(long)] limit:Option<usize>},
    Show {id:String},
});
pub async fn artifacts(args: ArtifactArgs) -> ClientResult<()> {
    let command = match args.action {
        ArtifactAction::List {
            goal_id,
            after,
            limit,
        } => Command::ArtifactsList {
            goal_id,
            after,
            limit,
        },
        ArtifactAction::Show { id } => Command::ArtifactsShow { id },
    };
    print(args.connection.client()?.execute(command).await?)
}
