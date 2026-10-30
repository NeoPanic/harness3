use crate::*;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublishInput {
    pub title: String,
    pub instructions: String,
    pub acceptance: String,
}
impl PublishInput {
    pub fn validate(&self) -> PResult<()> {
        if self.title.trim().is_empty()
            || self.title.len() > 200
            || self.instructions.trim().is_empty()
            || self.instructions.len() > 8000
            || self.acceptance.trim().is_empty()
            || self.acceptance.len() > 8000
        {
            return Err(fault(400, "invalid_board_task"));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SplitInput {
    pub tasks: Vec<PublishInput>,
}
