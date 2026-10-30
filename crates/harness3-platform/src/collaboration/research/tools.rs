use super::Plan;
use crate::store::Store;
use harness3::{CancellationToken, Context, Error, Result, Tool, ToolOutput, ToolSpec};
use serde_json::{Value, json};
use std::sync::Arc;
pub fn tool(store: Store, run: &str, count: usize) -> Arc<dyn Tool> {
    Arc::new(Planner {
        store,
        run: run.into(),
        count,
    })
}
struct Planner {
    store: Store,
    run: String,
    count: usize,
}
#[async_trait::async_trait]
impl Tool for Planner {
    fn spec(&self) -> ToolSpec {
        ToolSpec{name:"platform.research.plan".into(),description:"Submit the bounded research division once. Each task goes to a different fresh Agent; all tasks run independently before a separate synthesizer receives their evidence. Do not assume tools beyond the selected Preset.".into(),parameters:json!({"type":"object","properties":{"tasks":{"type":"array","minItems":self.count,"maxItems":self.count,"items":{"type":"object","properties":{"title":{"type":"string","maxLength":200},"instructions":{"type":"string","maxLength":4000}},"required":["title","instructions"],"additionalProperties":false}},"synthesis":{"type":"string","maxLength":4000}},"required":["tasks","synthesis"],"additionalProperties":false}),mutates:true}
    }
    async fn execute(
        &self,
        _: &Context,
        arguments: Value,
        cancel: CancellationToken,
    ) -> Result<ToolOutput> {
        if cancel.is_cancelled() {
            return Err(Error::new("cancelled"));
        }
        let plan: Plan =
            serde_json::from_value(arguments).map_err(|_| Error::new("invalid_arguments"))?;
        let receipt = self
            .store
            .submit_research_plan(&self.run, &plan)
            .map_err(|e| Error::new(e.1))?;
        Ok(ToolOutput {
            content: receipt,
            changed: true,
        })
    }
}
