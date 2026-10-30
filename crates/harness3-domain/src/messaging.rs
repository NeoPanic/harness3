use crate::*;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MessageInput {
    pub text: String,
    #[serde(default)]
    pub goal_id: Option<String>,
    #[serde(default)]
    pub correlation_id: Option<String>,
    #[serde(default)]
    pub causation_id: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MessageAction {
    pub action: String,
    pub note: String,
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MessageQuery {
    #[serde(default)]
    pub after: i64,
    pub limit: Option<usize>,
    pub instance_id: Option<String>,
    pub direction: Option<String>,
}
