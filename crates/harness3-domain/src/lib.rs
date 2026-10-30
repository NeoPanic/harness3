use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

pub type PResult<T> = Result<T, Fault>;

#[derive(Debug)]
pub struct Fault(pub u16, pub String);
impl std::fmt::Display for Fault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.1)
    }
}
impl std::error::Error for Fault {}
impl From<serde_json::Error> for Fault {
    fn from(_: serde_json::Error) -> Self {
        Self(500, "stored_data_invalid".into())
    }
}
pub fn fault(code: u16, message: &str) -> Fault {
    Fault(code, message.into())
}
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
pub fn id(prefix: &str) -> String {
    format!("{prefix}_{}", uuid::Uuid::new_v4().simple())
}
pub fn valid_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 80
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Protocol {
    Demo,
    OpenaiCompatible,
    OpenaiResponses,
    Anthropic,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginRef {
    pub id: String,
    pub version: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preset {
    pub name: String,
    pub version: String,
    pub purpose: String,
    pub objective: String,
    pub instructions: String,
    pub protocol: Protocol,
    #[serde(default)]
    pub plugin: Option<PluginRef>,
}
impl Preset {
    pub fn validate(&self) -> PResult<()> {
        if !valid_name(&self.name)
            || !valid_name(&self.version)
            || self.purpose.trim().is_empty()
            || self.objective.trim().is_empty()
            || self.instructions.trim().is_empty()
            || self.purpose.len() > 2000
            || self.objective.len() > 8000
            || self.instructions.len() > 32000
        {
            return Err(fault(400, "invalid_preset"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bundle {
    pub id: String,
    pub version: String,
    pub description: String,
    pub instructions: String,
}
impl Bundle {
    pub fn validate(&self) -> PResult<()> {
        if !valid_name(&self.id)
            || !valid_name(&self.version)
            || self.description.trim().is_empty()
            || self.description.len() > 2000
            || self.instructions.trim().is_empty()
            || self.instructions.len() > 32000
        {
            return Err(fault(400, "invalid_prompt_bundle"));
        }
        Ok(())
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spawn {
    pub preset_name: String,
    pub preset_version: String,
    pub count: usize,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunInput {
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoalPlan {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verification: Option<crate::verification::Policy>,
    pub title: String,
    pub objective: String,
    pub acceptance: String,
    pub tasks: Vec<TaskPlan>,
}
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskPlan {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delegation: Option<crate::delegation::Policy>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub manual_claim: bool,
    pub key: String,
    pub instance_id: String,
    pub instructions: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
}
impl GoalPlan {
    pub fn validate(&self) -> PResult<()> {
        if self.title.trim().is_empty()
            || self.title.len() > 200
            || self.objective.trim().is_empty()
            || self.objective.len() > 8000
            || self.acceptance.trim().is_empty()
            || self.acceptance.len() > 8000
            || self.tasks.is_empty()
            || self.tasks.len() > crate::limits::TASKS_PER_GOAL
        {
            return Err(fault(400, "invalid_goal"));
        }
        let mut seen = std::collections::HashSet::new();
        for task in &self.tasks {
            let mut dependencies = std::collections::HashSet::new();
            if !valid_name(&task.key)
                || !valid_name(&task.instance_id)
                || task.candidates.len() > 15
                || task
                    .candidates
                    .iter()
                    .any(|c| !valid_name(c) || c == &task.instance_id)
                || task
                    .candidates
                    .iter()
                    .collect::<std::collections::HashSet<_>>()
                    .len()
                    != task.candidates.len()
                || seen.contains(&task.key)
                || task.instructions.trim().is_empty()
                || task.instructions.len() > 8000
                || task
                    .depends_on
                    .iter()
                    .any(|dep| !seen.contains(dep) || !dependencies.insert(dep))
            {
                return Err(fault(400, "invalid_task_graph"));
            }
            seen.insert(task.key.clone());
        }
        Ok(())
    }
}

pub fn capabilities() -> Value {
    serde_json::from_str(include_str!("../../../docs/implementation-status.json"))
        .expect("embedded implementation status must be valid JSON")
}

pub fn outcome_unknown(value: &Value) -> bool {
    match value {
        Value::Object(map) => map.iter().any(|(key, value)| {
            ((key == "code" || key == "error")
                && value
                    .as_str()
                    .is_some_and(|s| s.contains("outcome_unknown")))
                || (key == "outcome_unknown" && value == &Value::Bool(true))
                || (key != "arguments" && key != "call" && outcome_unknown(value))
        }),
        Value::Array(items) => items.iter().any(outcome_unknown),
        _ => false,
    }
}

pub mod artifacts;
pub mod board;
pub mod claims;
pub mod collaboration;
pub mod delegation;
pub mod fleets;
pub mod limits;
pub mod messaging;
pub mod research;
pub mod verification;
