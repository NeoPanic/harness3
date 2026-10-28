use clap::{Args, Subcommand};
use harness3_platform::domain::{GoalPlan, valid_name};
use serde_json::{Value, json};
use std::{path::PathBuf, time::Duration};

#[derive(Args)]
pub struct GoalArgs {
    #[arg(long, default_value = "7333", global = true)]
    port: u16,
    #[arg(long, default_value = ".harness3", global = true)]
    data_dir: PathBuf,
    #[command(subcommand)]
    action: GoalCommand,
}
#[derive(Subcommand)]
enum GoalCommand {
    /// List the latest 100 shared goals.
    List,
    /// Submit an explicit task graph; reuse the key to safely retry.
    Create {
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        key: String,
    },
    /// Inspect tasks, dependency contributions, runs and review state.
    Show { id: String },
    /// Cancel pending work and cooperatively drain started runs.
    Cancel { id: String },
    /// Record human acceptance after all tasks have completed.
    Accept {
        id: String,
        #[arg(long)]
        review: String,
    },
}
pub async fn goals(args: GoalArgs) -> Result<(), Box<dyn std::error::Error>> {
    let token = std::fs::read_to_string(args.data_dir.join("token"))?;
    let token = token.trim();
    if token.len() != 64 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("invalid local token file".into());
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;
    let base = format!("http://127.0.0.1:{}/api/v1/goals", args.port);
    let request = match args.action {
        GoalCommand::List => client.get(&base),
        GoalCommand::Create { file, key } => {
            let mut file = std::fs::File::open(file)?;
            // Bound the actual read as files can grow after a metadata check.
            let mut bytes = Vec::new();
            std::io::Read::read_to_end(
                &mut std::io::Read::take(&mut file, 128 * 1024 + 1),
                &mut bytes,
            )?;
            if bytes.len() > 128 * 1024 {
                return Err("goal plan exceeds 128 KiB".into());
            }
            let plan: GoalPlan = serde_json::from_slice(&bytes)?;
            plan.validate()?;
            client
                .post(&base)
                .header("Idempotency-Key", key)
                .json(&plan)
        }
        GoalCommand::Show { id } => {
            validate_id(&id)?;
            client.get(format!("{base}/{id}"))
        }
        GoalCommand::Cancel { id } => {
            validate_id(&id)?;
            client.post(format!("{base}/{id}/cancel")).json(&json!({}))
        }
        GoalCommand::Accept { id, review } => {
            validate_id(&id)?;
            client
                .post(format!("{base}/{id}/accept"))
                .json(&json!({"review":review}))
        }
    };
    let response = request.bearer_auth(token).send().await?;
    let status = response.status();
    let body: Value = response.json().await?;
    if !status.is_success() {
        return Err(format!(
            "HTTP {status}: {}",
            body["error"]["code"].as_str().unwrap_or("request_failed")
        )
        .into());
    }
    println!("{}", serde_json::to_string_pretty(&body)?);
    Ok(())
}
fn validate_id(id: &str) -> Result<(), Box<dyn std::error::Error>> {
    if !valid_name(id) {
        return Err("invalid goal ID".into());
    }
    Ok(())
}
