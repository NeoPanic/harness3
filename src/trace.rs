//! Developer diagnostics. The harness always collects trace in process; a host
//! that wants it persisted or read attaches a `TraceSink` and renders the
//! events however it likes. Trace never changes conversation state or execution.
use crate::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::VecDeque,
    sync::{Mutex, MutexGuard},
};

/// Trace is a debugging window, not a log: older events are dropped.
pub const CAPACITY: usize = 10_000;

#[derive(Clone, Serialize, Deserialize)]
pub struct TraceEvent {
    pub run_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    pub source: String,
    pub event_id: String,
    pub at_ms: u64,
    pub kind: String,
    pub data: Value,
}
impl TraceEvent {
    pub(crate) fn new(ctx: &Context, kind: &str, data: Value) -> Self {
        Self {
            run_id: ctx.run_id.clone(),
            correlation_id: ctx.metadata["correlation_id"].as_str().map(str::to_owned),
            source: ctx.source.clone(),
            event_id: ctx.event_id.clone(),
            at_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            kind: kind.into(),
            data,
        }
    }
}
/// Ordered in-process trace, newest last.
#[derive(Default)]
pub struct Trace(Mutex<VecDeque<TraceEvent>>);
impl Trace {
    pub fn events(&self) -> Vec<TraceEvent> {
        self.lock().iter().cloned().collect()
    }
    pub(crate) fn push(&self, event: TraceEvent) {
        let mut events = self.lock();
        if events.len() >= CAPACITY {
            events.pop_front();
        }
        events.push_back(event);
    }
    /// Diagnostics must not fail on another thread's panic.
    fn lock(&self) -> MutexGuard<'_, VecDeque<TraceEvent>> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}
#[async_trait]
pub trait TraceSink: Send + Sync {
    /// Diagnostics are best-effort: errors must not invalidate a committed tool.
    async fn record(&self, event: TraceEvent) -> Result<()>;
}
