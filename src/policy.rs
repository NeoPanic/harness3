//! Ordered interception. Policies may narrow a request or deny execution;
//! they are trusted host code, not a replacement for tool-side validation.
use crate::{Call, Context, ModelInput, Observation, Result};
use async_trait::async_trait;

#[async_trait]
pub trait Policy: Send + Sync {
    fn name(&self) -> &str;
    async fn before_step(&self, _: &Context, _: &mut ModelInput) -> Result<()> {
        Ok(())
    }
    /// ID must remain unchanged. Rewritten names/arguments are checked against
    /// the current task's authorization before dispatch.
    async fn before_tool(&self, _: &Context, _: &mut Call) -> Result<()> {
        Ok(())
    }
    /// The receipt is already durable and immutable. Failure stops the run,
    /// but cannot undo an effect or turn a committed receipt into a failure.
    async fn after_tool(&self, _: &Context, _: &Observation) -> Result<()> {
        Ok(())
    }
}
