//! Authenticated loopback HTTP client shared by CLI and outward MCP adapters.
use crate::domain::valid_name;
use reqwest::{Method, redirect::Policy};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{path::Path, time::Duration};

pub type ClientResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Clone)]
pub struct PlatformClient {
    http: reqwest::Client,
    base: String,
    token: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "tool", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Snapshot,
    PresetsList,
    PresetsPublish {
        preset: crate::domain::Preset,
    },
    PluginsList,
    ProcessPluginsInstall {
        source_dir: String,
    },
    ProcessPluginsShow {
        id: String,
        version: String,
    },
    ProcessPluginsAction {
        id: String,
        version: String,
        action: String,
    },
    PluginsInstall {
        bundle: crate::domain::Bundle,
    },
    FleetsList,
    FleetsCreate {
        plan: crate::fleets::FleetPlan,
        key: String,
    },
    FleetsShow {
        id: String,
    },
    FleetsScale {
        id: String,
        expected_revision: i64,
        count: usize,
        key: String,
    },
    FleetsTerminate {
        id: String,
        expected_revision: i64,
        key: String,
    },
    OperationsShow {
        id: String,
    },
    InstancesList,
    InstancesAction {
        id: String,
        action: String,
    },
    InstancesSession {
        id: String,
    },
    RunsCreate {
        id: String,
        text: String,
        key: String,
    },
    RunsShow {
        id: String,
    },
    RunsCancel {
        id: String,
    },
    GoalsList,
    GoalsCreate {
        plan: crate::domain::GoalPlan,
        key: String,
    },
    GoalsShow {
        id: String,
    },
    GoalsCancel {
        id: String,
    },
    GoalsAccept {
        id: String,
        review: String,
    },
    EventsList {
        #[serde(default)]
        after: u64,
    },
}
impl Command {
    pub fn read_only(&self) -> bool {
        matches!(
            self,
            Self::Snapshot
                | Self::PresetsList
                | Self::PluginsList
                | Self::ProcessPluginsShow { .. }
                | Self::FleetsList
                | Self::FleetsShow { .. }
                | Self::OperationsShow { .. }
                | Self::InstancesList
                | Self::InstancesSession { .. }
                | Self::RunsShow { .. }
                | Self::GoalsList
                | Self::GoalsShow { .. }
                | Self::EventsList { .. }
        )
    }
}
impl PlatformClient {
    pub fn local(port: u16, data_dir: &Path) -> ClientResult<Self> {
        let token = std::fs::read_to_string(data_dir.join("token"))?;
        let token = token.trim().to_owned();
        if token.len() != 64 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("invalid local token file".into());
        }
        Ok(Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .redirect(Policy::none())
                .no_proxy()
                .build()?,
            base: format!("http://127.0.0.1:{port}/api/v1"),
            token,
        })
    }
    async fn request(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        key: Option<&str>,
    ) -> ClientResult<Value> {
        let mut request = self
            .http
            .request(method, format!("{}{path}", self.base))
            .bearer_auth(&self.token);
        if let Some(key) = key {
            if key.is_empty() || key.len() > 160 || !key.bytes().all(|b| (0x20..=0x7e).contains(&b))
            {
                return Err(
                    "invalid idempotency key (1–160 printable ASCII characters required)".into(),
                );
            }
            request = request.header("Idempotency-Key", key);
        }
        if let Some(body) = body {
            let bytes = serde_json::to_vec(&body)?;
            if bytes.len() > 128 * 1024 {
                return Err("request exceeds 128 KiB".into());
            }
            request = request
                .header("Content-Type", "application/json")
                .body(bytes);
        }
        // A timeout can follow a committed command. Never retry a write with a new key.
        let response = request.send().await.map_err(|_| "daemon connection failed or timed out; inspect state or retry the same content with the same key")?;
        let status = response.status();
        if !status.is_success() {
            let body = response.json::<Value>().await.unwrap_or(Value::Null);
            return Err(format!(
                "HTTP {}: {}",
                status.as_u16(),
                body["error"]["code"].as_str().unwrap_or("request_failed")
            )
            .into());
        }
        Ok(response.json().await?)
    }
    pub async fn execute(&self, command: Command) -> ClientResult<Value> {
        use Command::*;
        let (method, path, body, key) = match command {
            Snapshot => (Method::GET, "/snapshot".into(), None, None),
            PresetsList | PluginsList | InstancesList => {
                let collection = match command {
                    PresetsList => "presets",
                    PluginsList => "plugins",
                    _ => "instances",
                };
                let snapshot = self.request(Method::GET, "/snapshot", None, None).await?;
                if collection == "plugins" {
                    return Ok(
                        json!({"plugins":snapshot["plugins"],"process_plugins":snapshot["process_plugins"],"execution_enabled":snapshot["process_plugin_execution"]}),
                    );
                }
                return Ok(json!({collection: snapshot[collection]}));
            }
            PresetsPublish { preset } => (
                Method::POST,
                "/presets".into(),
                Some(serde_json::to_value(preset)?),
                None,
            ),
            ProcessPluginsInstall { source_dir } => (
                Method::POST,
                "/process-plugins".into(),
                Some(json!({"source_dir":source_dir})),
                None,
            ),
            ProcessPluginsShow { id, version } => (
                Method::GET,
                object("process-plugins", &id, &format!("/{}", segment(&version)?))?,
                None,
                None,
            ),
            ProcessPluginsAction {
                id,
                version,
                action,
            } => (
                Method::POST,
                object(
                    "process-plugins",
                    &id,
                    &format!("/{}/actions", segment(&version)?),
                )?,
                Some(json!({"action":action})),
                None,
            ),
            PluginsInstall { bundle } => (
                Method::POST,
                "/plugins".into(),
                Some(serde_json::to_value(bundle)?),
                None,
            ),
            FleetsList => (Method::GET, "/agent-fleets".into(), None, None),
            FleetsCreate { plan, key } => (
                Method::POST,
                "/agent-fleets".into(),
                Some(serde_json::to_value(plan)?),
                Some(key),
            ),
            FleetsShow { id } => (Method::GET, object("agent-fleets", &id, "")?, None, None),
            FleetsScale {
                id,
                expected_revision,
                count,
                key,
            } => (
                Method::POST,
                object("agent-fleets", &id, "/actions")?,
                Some(json!({"action":"scale","expected_revision":expected_revision,"count":count})),
                Some(key),
            ),
            FleetsTerminate {
                id,
                expected_revision,
                key,
            } => (
                Method::POST,
                object("agent-fleets", &id, "/actions")?,
                Some(json!({"action":"terminate","expected_revision":expected_revision})),
                Some(key),
            ),
            OperationsShow { id } => (Method::GET, object("operations", &id, "")?, None, None),
            InstancesAction { id, action } => (
                Method::POST,
                object("agent-instances", &id, "/actions")?,
                Some(json!({"action":action})),
                None,
            ),
            InstancesSession { id } => (
                Method::GET,
                object("agent-instances", &id, "/session")?,
                None,
                None,
            ),
            RunsCreate { id, text, key } => (
                Method::POST,
                object("agent-instances", &id, "/runs")?,
                Some(json!({"text":text})),
                Some(key),
            ),
            RunsShow { id } => (Method::GET, object("runs", &id, "")?, None, None),
            RunsCancel { id } => (
                Method::POST,
                object("runs", &id, "/cancel")?,
                Some(json!({})),
                None,
            ),
            GoalsList => (Method::GET, "/goals".into(), None, None),
            GoalsCreate { plan, key } => (
                Method::POST,
                "/goals".into(),
                Some(serde_json::to_value(plan)?),
                Some(key),
            ),
            GoalsShow { id } => (Method::GET, object("goals", &id, "")?, None, None),
            GoalsCancel { id } => (
                Method::POST,
                object("goals", &id, "/cancel")?,
                Some(json!({})),
                None,
            ),
            GoalsAccept { id, review } => (
                Method::POST,
                object("goals", &id, "/accept")?,
                Some(json!({"review":review})),
                None,
            ),
            EventsList { after } => (Method::GET, format!("/events?after={after}"), None, None),
        };
        self.request(method, &path, body, key.as_deref()).await
    }
}
fn object(collection: &str, id: &str, suffix: &str) -> ClientResult<String> {
    if !valid_name(id) || matches!(id, "." | "..") {
        return Err("invalid object ID".into());
    }
    Ok(format!("/{collection}/{id}{suffix}"))
}

fn segment(value: &str) -> ClientResult<&str> {
    if !valid_name(value) || matches!(value, "." | "..") {
        Err("invalid object version".into())
    } else {
        Ok(value)
    }
}
