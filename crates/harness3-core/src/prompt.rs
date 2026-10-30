//! Composable instructions. Policies are model-facing guidance, not a business
//! workflow: contributors do not select tools or intercept user decisions.
use crate::{CancellationToken, Context, Result};
use async_trait::async_trait;

#[async_trait]
pub trait PromptSection: Send + Sync {
    fn name(&self) -> &str;
    fn order(&self) -> i32 {
        0
    }
    async fn render(&self, context: &Context, cancel: CancellationToken) -> Result<String>;
}
/// Optional general interaction guidance; a host can replace or omit it.
pub struct InteractionPrinciples;
#[async_trait]
impl PromptSection for InteractionPrinciples {
    fn name(&self) -> &str {
        "interaction-principles"
    }
    async fn render(&self, _: &Context, _: CancellationToken) -> Result<String> {
        Ok(include_str!("interaction.md").into())
    }
}
