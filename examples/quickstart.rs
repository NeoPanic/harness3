//! Offline model → tool → model execution and duplicate-delivery handling.
//! Run with `cargo run --locked --example quickstart`.
use async_trait::async_trait;
use harness3::{
    AgentProfile, Call, CancellationToken, Context, Error, Harness, InboundEvent, Limits,
    LiveOutput, Model, ModelInput, ModelReply, Registry, Request, Result, Source, Tool, ToolOutput,
    ToolSpec, Turn, session::MemorySessionStore,
};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};

struct LocalSource;
impl Source for LocalSource {
    fn name(&self) -> &str {
        "local"
    }

    fn decode(&self, event: &InboundEvent) -> Result<Request> {
        let text = event.payload["text"]
            .as_str()
            .ok_or_else(|| Error::new("invalid_input"))?;
        Ok(Request {
            task: "demo".into(),
            text: text.into(),
        })
    }
}

// A deterministic adapter makes the example reproducible without a provider SDK.
struct LocalModel;
#[async_trait]
impl Model for LocalModel {
    async fn complete(
        &self,
        input: ModelInput,
        _live: &LiveOutput,
        _cancel: CancellationToken,
    ) -> Result<ModelReply> {
        let (text, calls) = match input.turns.last() {
            Some(Turn::Tools(observations)) => {
                let count = observations
                    .first()
                    .and_then(|observation| observation.output.content["characters"].as_u64())
                    .ok_or_else(|| Error::new("unexpected_tool_result"))?;
                (format!("Harness³: {count} characters."), vec![])
            }
            _ => (
                String::new(),
                vec![Call {
                    id: "count-1".into(),
                    name: "count_characters".into(),
                    arguments: json!({"text": input.text}),
                }],
            ),
        };
        Ok(ModelReply {
            text,
            calls,
            usage: None,
            continuation: Value::Null,
            reasoning: None,
        })
    }
}

struct CountCharacters;
#[async_trait]
impl Tool for CountCharacters {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "count_characters".into(),
            description: "Count Unicode scalar values in text (not grapheme clusters).".into(),
            parameters: json!({
                "type": "object",
                "properties": {"text": {"type": "string"}},
                "required": ["text"],
                "additionalProperties": false
            }),
            mutates: false,
        }
    }

    async fn execute(
        &self,
        _context: &Context,
        arguments: Value,
        _cancel: CancellationToken,
    ) -> Result<ToolOutput> {
        // Tools validate arguments themselves; schemas alone are not validation.
        let object = arguments
            .as_object()
            .filter(|object| object.len() == 1)
            .ok_or_else(|| Error::new("invalid_arguments"))?;
        let text = object
            .get("text")
            .and_then(Value::as_str)
            .ok_or_else(|| Error::new("invalid_arguments"))?;
        Ok(ToolOutput {
            content: json!({"characters": text.chars().count()}),
            changed: false,
        })
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let mut registry = Registry::default();
    registry.source(Arc::new(LocalSource))?;
    registry.model("local", Arc::new(LocalModel))?;
    registry.tool(Arc::new(CountCharacters))?;
    registry.task(AgentProfile {
        name: "demo".into(),
        instructions: "Count the input's Unicode scalar values using count_characters.".into(),
        tools: vec!["count_characters".into()],
        required_tool: None,
        skills: vec![],
    })?;
    let harness = Harness::new(
        registry,
        Limits {
            rounds: 4,
            tool_calls: 4,
            timeout: Duration::from_secs(10),
        },
    );
    // Reuse this store across runs. Replace it with a durable adapter in production.
    let store = MemorySessionStore::default();
    let event = InboundEvent {
        source: "local".into(),
        id: "message-1".into(),
        payload: json!({"text": "Harness³"}),
    };
    let report = harness
        .run_session(
            &store,
            "demo-session",
            event.clone(),
            "local",
            CancellationToken::new(),
        )
        .await?;
    // A settled report can contain a run failure; admission success is not enough.
    if let Some(error) = report.error {
        return Err(error);
    }
    assert_eq!(report.output.as_deref(), Some("Harness³: 8 characters."));
    assert_eq!(report.observations.len(), 1);
    println!("{}", report.output.as_deref().unwrap_or_default());

    let duplicate = harness
        .run_session(
            &store,
            "demo-session",
            event,
            "local",
            CancellationToken::new(),
        )
        .await?;
    assert!(duplicate.error.is_none());
    assert_eq!(report.run_id, duplicate.run_id);
    println!("Duplicate delivery reused the settled run: true");
    Ok(())
}
