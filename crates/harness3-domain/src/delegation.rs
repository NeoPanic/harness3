use crate::*;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub max_depth: usize,
    pub max_children: usize,
    pub max_descendants: usize,
    pub roles: Vec<Role>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Role {
    pub name: String,
    pub preset_name: String,
    pub preset_version: String,
    #[serde(default)]
    pub parameters: serde_json::Map<String, Value>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub tasks: Vec<Child>,
    pub continuation: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Child {
    pub role: String,
    pub title: String,
    pub instructions: String,
}
