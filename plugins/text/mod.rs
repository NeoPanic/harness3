use harness3::{CancellationToken, Context, Error, Result, Tool, ToolOutput, ToolSpec};
use serde_json::{Value, json};
pub struct InspectText;
#[async_trait::async_trait]
impl Tool for InspectText {
    fn spec(&self) -> ToolSpec {
        ToolSpec{name:"text.inspect".into(),description:"Inspect Unicode scalar count, lines and UTF-8 bytes in text. Read-only; does not read files.".into(),parameters:json!({"type":"object","properties":{"text":{"type":"string","maxLength":64000}},"required":["text"],"additionalProperties":false}),mutates:false}
    }
    fn parallel_safe(&self) -> bool {
        true
    }
    async fn execute(
        &self,
        _: &Context,
        arguments: Value,
        _: CancellationToken,
    ) -> Result<ToolOutput> {
        let object = arguments
            .as_object()
            .filter(|v| v.len() == 1)
            .ok_or_else(|| Error::new("invalid_arguments"))?;
        let text = object
            .get("text")
            .and_then(Value::as_str)
            .filter(|s| s.len() <= 64000)
            .ok_or_else(|| Error::new("invalid_arguments"))?;
        Ok(ToolOutput {
            content: json!({"characters":text.chars().count(),"lines":text.lines().count(),"bytes":text.len()}),
            changed: false,
        })
    }
}
