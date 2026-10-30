use crate::*;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FleetPlan {
    pub preset_name: String,
    pub preset_version: String,
    pub count: usize,
    #[serde(default = "finite")]
    pub mode: String,
    #[serde(default)]
    pub ttl_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub parameters: serde_json::Map<String, Value>,
}
fn finite() -> String {
    "finite".into()
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FleetAction {
    pub action: String,
    pub expected_revision: i64,
    #[serde(default)]
    pub count: Option<usize>,
}
