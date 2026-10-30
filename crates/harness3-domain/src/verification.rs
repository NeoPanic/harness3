use crate::*;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub preset_name: String,
    pub preset_version: String,
    #[serde(default)]
    pub parameters: serde_json::Map<String, Value>,
    pub checks: Vec<Check>,
    #[serde(default)]
    pub max_auto_reworks: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_rework_scope: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Check {
    pub key: String,
    pub description: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Verdict {
    pub summary: String,
    pub checks: Vec<Finding>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Finding {
    pub key: String,
    pub status: String,
    pub reason: String,
    pub artifact_ids: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Retry {
    pub expected_round: i64,
    pub expected_verification_id: String,
}
