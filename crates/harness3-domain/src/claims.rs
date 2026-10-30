use crate::*;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimInput {
    pub expected_round: i64,
    pub expected_generation: i64,
    pub instance_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseInput {
    pub expected_round: i64,
    pub expected_generation: i64,
    pub note: String,
}
