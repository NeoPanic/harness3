use crate::*;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReworkInput {
    pub expected_round: i64,
    pub note: String,
    #[serde(default)]
    pub assignments: Vec<Assignment>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub task_keys: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assignment {
    pub key: String,
    pub instance_id: String,
}
