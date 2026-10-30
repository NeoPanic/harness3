use crate::*;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextInput {
    pub key: String,
    pub expected_revision: i64,
    pub title: String,
    pub body: String,
    pub kind: String,
    #[serde(default = "active")]
    pub status: String,
    #[serde(default)]
    pub artifact_ids: Vec<String>,
}
fn active() -> String {
    "active".into()
}
