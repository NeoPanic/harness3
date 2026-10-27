//! Harness³ — a composable runtime for agent collectives.
//!
//! Provider-, transport- and storage-independent execution with durable sessions.
//! The Rust package and import name are `harness3`; the execution API is [`Harness`].
//! Capabilities are trusted Rust components registered explicitly, not downloaded code.
pub mod context;
pub mod driver;
pub mod policy;
pub mod prompt;
pub mod session;
pub mod skills;
pub mod trace;
pub mod transport;
pub mod work;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Mutex;
use std::time::Duration;
mod engine;
mod registry;
pub use engine::Harness;
pub use registry::Registry;
pub(crate) use registry::name;
pub use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error, PartialEq, Eq)]
#[error("{code}")]
pub struct Error {
    pub code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub details: Value,
}
impl Error {
    pub fn new(code: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: None,
            details: Value::Null,
        }
    }
}
impl Error {
    pub fn explain(mut self, message: impl Into<String>, details: Value) -> Self {
        self.message = Some(message.into());
        self.details = details;
        self
    }
}
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Clone, Serialize, Deserialize)]
pub struct InboundEvent {
    pub source: String,
    pub id: String,
    pub payload: Value,
}
#[derive(Clone)]
pub struct Request {
    pub task: String,
    pub text: String,
}
#[derive(Clone, Serialize)]
pub struct Context {
    pub run_id: String,
    pub source: String,
    pub event_id: String,
    pub task: String,
    pub original_text: String,
    pub metadata: Value,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: Value,
    pub mutates: bool,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Call {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input: u64,
    pub output: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_input: Option<u64>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct ModelReply {
    #[serde(default)]
    pub usage: Option<TokenUsage>,
    pub text: String,
    pub calls: Vec<Call>,
    pub continuation: Value,
    /// Reasoning the provider streamed alongside the reply, when the model
    /// exposes one. Owned by the surface that shows it; never replayed as a
    /// message and never sent back to the model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    pub content: Value,
    pub changed: bool,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Observation {
    pub call: Call,
    pub output: ToolOutput,
}
#[derive(Clone, Serialize, Deserialize)]
pub enum Turn {
    User(String),
    /// Host-selected model-visible metadata, logged alongside its user message.
    Context(Value),
    Assistant(ModelReply),
    Tools(Vec<Observation>),
}
#[derive(Clone, Serialize, Deserialize)]
pub struct ModelInput {
    pub system: String,
    pub text: String,
    pub tools: Vec<ToolSpec>,
    pub turns: Vec<Turn>,
}
#[async_trait]
pub trait Model: Send + Sync {
    fn context_budget(&self) -> context::ContextBudget {
        context::ContextBudget::default()
    }
    /// Non-secret adapter identity/settings for request reconstruction and Trace.
    fn metadata(&self) -> Value {
        Value::Null
    }
    /// `live` receives provider deltas as they arrive, so an interrupted call
    /// still has a durable prefix. Adapters that do not stream ignore it.
    async fn complete(
        &self,
        input: ModelInput,
        live: &LiveOutput,
        cancel: CancellationToken,
    ) -> Result<ModelReply>;
}
/// Live output of one model call. Adapters append provider deltas as they
/// arrive and the harness keeps them, so text the user already read is not
/// lost when a run is cancelled or times out. Called once per streamed
/// fragment: implementations must not do durable work here.
#[derive(Default)]
pub struct LiveOutput {
    text: Mutex<String>,
    reasoning: Mutex<String>,
    internal: bool,
    max_output_tokens: Option<u64>,
    metadata: Mutex<Value>,
}
impl LiveOutput {
    /// Internal model work must not publish text or reasoning to a chat surface.
    pub fn internal() -> Self {
        Self {
            internal: true,
            ..Self::default()
        }
    }
    pub fn metadata(&self) -> Value {
        self.metadata
            .lock()
            .map(|value| value.clone())
            .unwrap_or_default()
    }
    pub(crate) fn set_metadata(&self, value: Value) {
        if let Ok(mut metadata) = self.metadata.lock() {
            *metadata = value;
        }
    }
    pub fn is_internal(&self) -> bool {
        self.internal
    }
    pub fn with_max_output_tokens(mut self, tokens: u64) -> Self {
        self.max_output_tokens = Some(tokens);
        self
    }
    pub fn max_output_tokens(&self) -> Option<u64> {
        self.max_output_tokens
    }
    pub fn text(&self, delta: &str) {
        if let Ok(mut out) = self.text.lock() {
            out.push_str(delta);
        }
    }
    pub fn reasoning(&self, delta: &str) {
        if let Ok(mut out) = self.reasoning.lock() {
            out.push_str(delta);
        }
    }
    pub fn text_so_far(&self) -> String {
        self.text.lock().map(|t| t.clone()).unwrap_or_default()
    }
    pub(crate) fn reset(&self) {
        if let Ok(mut text) = self.text.lock() {
            text.clear();
        }
        if let Ok(mut reasoning) = self.reasoning.lock() {
            reasoning.clear();
        }
    }
    /// The prefix no reply reported, or nothing when the provider produced no
    /// text at all. The continuation is absent because the call never finished.
    pub(crate) fn unfinished(&self) -> Option<ModelReply> {
        let text = self.text_so_far();
        if text.is_empty() {
            return None;
        }
        let reasoning = self.reasoning.lock().map(|r| r.clone()).unwrap_or_default();
        Some(ModelReply {
            usage: None,
            text,
            calls: vec![],
            continuation: Value::Null,
            reasoning: (!reasoning.is_empty()).then_some(reasoning),
        })
    }
}
#[async_trait]
pub trait Tool: Send + Sync {
    fn spec(&self) -> ToolSpec;
    /// Opt in only when independent calls can overlap safely, including calls
    /// to this same tool. Shared caches, budgets and other internal state must
    /// synchronize themselves. Mutating tools remain execution barriers.
    fn parallel_safe(&self) -> bool {
        false
    }
    /// Validate untrusted arguments before any effect. Mutating tools must be
    /// idempotent by context.source/event_id and return a committed receipt.
    /// Once execution begins the harness never drops this future on cancellation.
    async fn execute(
        &self,
        context: &Context,
        arguments: Value,
        cancel: CancellationToken,
    ) -> Result<ToolOutput>;
}
pub trait Source: Send + Sync {
    fn name(&self) -> &str;
    fn decode(&self, event: &InboundEvent) -> Result<Request>;
}
#[derive(Clone)]
pub struct AgentProfile {
    pub name: String,
    pub instructions: String,
    pub tools: Vec<String>,
    pub required_tool: Option<String>,
    pub skills: Vec<String>,
}
#[derive(Clone, Serialize)]
pub struct Event {
    pub sequence: u32,
    pub kind: String,
    pub data: Value,
}
#[async_trait]
pub trait Observer: Send + Sync {
    async fn on_event(&self, context: &Context, event: &Event) -> Result<()>;
}
#[derive(Clone)]
pub struct Limits {
    pub rounds: u32,
    pub tool_calls: u32,
    pub timeout: Duration,
}
impl Default for Limits {
    fn default() -> Self {
        // 一轮真实的整理任务（查人、读技能、写人物、再核对）很容易超过十次
        // 工具调用；上限放得宽一些，超时与“停止”按钮负责兜住成本。
        Self {
            rounds: 256,
            tool_calls: 1024,
            timeout: Duration::from_secs(3600),
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Report {
    pub run_id: String,
    pub output: Option<String>,
    pub error: Option<Error>,
    pub observations: Vec<Observation>,
    /// Summed token usage across every model round of the turn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<TokenUsage>,
    /// Wall time spent inside model calls, for the tokens-per-second reading.
    #[serde(default)]
    pub generation_ms: u64,
    /// Whole run, including tools, policies and context compaction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_ms: Option<u64>,
}
