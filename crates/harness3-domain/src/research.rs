use crate::*;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResearchInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verification: Option<crate::verification::Policy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delegation: Option<crate::delegation::Policy>,
    pub title: String,
    pub objective: String,
    pub acceptance: String,
    pub preset_name: String,
    pub preset_version: String,
    pub researchers: usize,
    #[serde(default)]
    pub parameters: serde_json::Map<String, Value>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub tasks: Vec<ResearchTask>,
    pub synthesis: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResearchTask {
    pub title: String,
    pub instructions: String,
}
