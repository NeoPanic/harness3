//! Portable conversation checkpoints. Storage implementations must atomically
//! compare revisions; a stale writer must never overwrite another turn.
use crate::{Call, Error, ModelInput, Observation, Report, Result, ToolOutput, ToolSpec, Turn};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Mutex};

/// Shape of a durable checkpoint. A store that meets a newer value must refuse
/// it rather than guess at fields it does not know.
pub const FORMAT_VERSION: u32 = 3;

/// Immutable checkpoint delta. Its index is the stable event cursor. Replay
/// never invokes a model, tool, observer or transport.
#[derive(Clone, Serialize, Deserialize)]
pub struct SessionEvent {
    pub at_ms: u64,
    pub turns: Vec<Turn>,
    pub requests: Vec<RequestRecord>,
    pub dispatches: Vec<ToolDispatch>,
    pub inputs: BTreeMap<String, String>,
    pub completed: BTreeMap<String, Report>,
    pub active_run: Option<String>,
    pub active_delivery: Option<String>,
    pub active_start: usize,
    pub compaction: Option<Compaction>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ToolDispatch {
    pub run_id: String,
    pub original: Call,
    pub effective: Call,
    pub denied: Option<Error>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Compaction {
    /// Exclusive raw-history boundary; original turns remain immutable.
    pub through: usize,
    pub summary: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct RequestRecord {
    /// None for a normal reply; internal requests are also durable and replayable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
    /// Present only when a policy rewrites the projected request.
    #[serde(default)]
    pub override_input: Option<ModelInput>,
    pub run_id: String,
    pub model: String,
    pub model_metadata: Value,
    pub system: String,
    pub tools: Vec<ToolSpec>,
    pub history_start: usize,
    pub history_end: usize,
    #[serde(default)]
    pub compaction: Option<Compaction>,
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Session {
    #[serde(default)]
    pub dispatches: Vec<ToolDispatch>,
    #[serde(default)]
    pub events: Vec<SessionEvent>,
    #[serde(default)]
    pub compaction: Option<Compaction>,
    /// Written on every checkpoint. Zero means a checkpoint stored before this
    /// field existed, which is read as version one.
    #[serde(default)]
    pub format_version: u32,
    pub revision: u64,
    pub active_run: Option<String>,
    pub active_delivery: Option<String>,
    pub active_start: usize,
    pub inputs: BTreeMap<String, String>,
    pub turns: Vec<Turn>,
    pub requests: Vec<RequestRecord>,
    /// Delivery deduplication includes the source in the key, not just event ID.
    pub completed: BTreeMap<String, Report>,
}
#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn load(&self, id: &str) -> Result<Session>;
    /// Create at revision zero or replace exactly expected_revision. The stored
    /// revision must become expected_revision + 1, even for an empty session.
    async fn save(&self, id: &str, expected_revision: u64, session: &Session) -> Result<()>;
}
#[derive(Default)]
pub struct MemorySessionStore(Mutex<BTreeMap<String, Session>>);
#[async_trait]
impl SessionStore for MemorySessionStore {
    async fn load(&self, id: &str) -> Result<Session> {
        Ok(self
            .0
            .lock()
            .map_err(|_| Error::new("session_storage"))?
            .get(id)
            .cloned()
            .unwrap_or_default())
    }
    async fn save(&self, id: &str, revision: u64, session: &Session) -> Result<()> {
        let mut all = self.0.lock().map_err(|_| Error::new("session_storage"))?;
        if all.get(id).map_or(0, |s| s.revision) != revision {
            return Err(Error::new("session_conflict"));
        }
        let mut next = session.clone();
        next.seal()?;
        if let Some(previous) = all.get(id) {
            next.validate_append(previous)?;
        }
        next.revision = revision + 1;
        all.insert(id.into(), next);
        Ok(())
    }
}
impl Session {
    /// Build the projection at an event cursor without replaying side effects.
    pub fn replay(&self, through: usize) -> Result<Self> {
        self.ensure_supported()?;
        let events = self
            .events
            .get(..through)
            .ok_or_else(|| Error::new("event_not_found"))?;
        let mut out = Self {
            format_version: FORMAT_VERSION,
            ..Self::default()
        };
        for event in events {
            out.turns.extend(event.turns.clone());
            out.requests.extend(event.requests.clone());
            out.dispatches.extend(event.dispatches.clone());
            out.inputs.extend(event.inputs.clone());
            out.completed.extend(event.completed.clone());
            out.active_run = event.active_run.clone();
            out.active_delivery = event.active_delivery.clone();
            out.active_start = event.active_start;
            out.compaction = event.compaction.clone();
        }
        out.events = events.to_vec();
        Ok(out)
    }

    /// Fork only settled boundaries; saving uses a new host-chosen session ID.
    /// Delivery receipts remain inherited so historical effects are not retried.
    pub fn fork(&self, through: usize) -> Result<Self> {
        let out = self.replay(through)?;
        if out.active_run.is_some() {
            return Err(Error::new("session_busy"));
        }
        Ok(out)
    }

    /// Compact the next model projection, never the immutable evidence. Only
    /// complete runs can be summarized, leaving a user-led suffix.
    pub fn compact(&mut self, through: usize, summary: String) -> Result<()> {
        if self.active_run.is_some() {
            return Err(Error::new("session_busy"));
        }
        if summary.trim().is_empty()
            || through == 0
            || through > self.turns.len()
            || (through < self.turns.len() && !matches!(self.turns[through], Turn::User(_)))
            || self
                .compaction
                .as_ref()
                .is_some_and(|c| through < c.through)
        {
            return Err(Error::new("invalid_compaction"));
        }
        let mut boundary = 0;
        let settled = self.events.iter().any(|e| {
            boundary += e.turns.len();
            boundary == through && e.active_run.is_none()
        });
        if !settled {
            return Err(Error::new("invalid_compaction"));
        }
        self.compaction = Some(Compaction { through, summary });
        Ok(())
    }

    /// Called by durable stores in the same transaction as revision fencing.
    pub fn validate_append(&self, previous: &Self) -> Result<()> {
        if !json_prefix(&self.turns, &previous.turns)
            || !json_prefix(&self.requests, &previous.requests)
            || !json_prefix(&self.dispatches, &previous.dispatches)
            || !json_prefix(&self.events, &previous.events)
        {
            return Err(Error::new("session_history_rewritten"));
        }
        Ok(())
    }

    /// Seal new projection facts into one immutable event. Legacy checkpoints
    /// are imported as the first event without deleting their history.
    pub fn seal(&mut self) -> Result<()> {
        // ponytail: full-log projection/validation is O(history) per checkpoint;
        // an incremental event store is the upgrade path for long-lived sessions.
        self.ensure_supported()?;
        let old = self.replay(self.events.len())?;
        if !json_prefix(&self.turns, &old.turns)
            || !json_prefix(&self.requests, &old.requests)
            || !json_prefix(&self.dispatches, &old.dispatches)
            || old
                .inputs
                .iter()
                .any(|(k, v)| self.inputs.get(k) != Some(v))
            || old.completed.iter().any(|(k, v)| {
                serde_json::to_value(self.completed.get(k)).ok()
                    != serde_json::to_value(Some(v)).ok()
            })
        {
            return Err(Error::new("session_history_rewritten"));
        }
        let event = SessionEvent {
            at_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            turns: self.turns[old.turns.len()..].to_vec(),
            requests: self.requests[old.requests.len()..].to_vec(),
            dispatches: self.dispatches[old.dispatches.len()..].to_vec(),
            inputs: self
                .inputs
                .iter()
                .filter(|(k, _)| !old.inputs.contains_key(*k))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            completed: self
                .completed
                .iter()
                .filter(|(k, _)| !old.completed.contains_key(*k))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            active_run: self.active_run.clone(),
            active_delivery: self.active_delivery.clone(),
            active_start: self.active_start,
            compaction: self.compaction.clone(),
        };
        if !event.turns.is_empty()
            || !event.requests.is_empty()
            || !event.dispatches.is_empty()
            || !event.inputs.is_empty()
            || !event.completed.is_empty()
            || self.active_run != old.active_run
            || self.active_delivery != old.active_delivery
            || self.active_start != old.active_start
            || serde_json::to_value(&self.compaction).ok()
                != serde_json::to_value(&old.compaction).ok()
        {
            self.events.push(event);
        }
        self.format_version = FORMAT_VERSION;
        Ok(())
    }
    /// Refuse a checkpoint written by a newer harness. Reading an unknown shape
    /// as if it were current would corrupt history, so the caller fails instead.
    pub fn ensure_supported(&self) -> Result<()> {
        if self.format_version > FORMAT_VERSION {
            return Err(Error::new("session_format_unsupported"));
        }
        Ok(())
    }
    /// Reconstruct an exact admitted model request from immutable history plus
    /// its prompt/tool snapshot. No provider request depends on hidden state.
    pub fn request_input(&self, index: usize) -> Result<ModelInput> {
        let r = self
            .requests
            .get(index)
            .ok_or_else(|| Error::new("request_not_found"))?;
        if let Some(input) = &r.override_input {
            return Ok(input.clone());
        }
        let history = self
            .turns
            .get(r.history_start..r.history_end)
            .ok_or_else(|| Error::new("invalid_session_history"))?;
        Self::project(history, r.compaction.as_ref(), &r.system, &r.tools)
    }

    pub(crate) fn projected_input(&self, system: &str, tools: &[ToolSpec]) -> Result<ModelInput> {
        Self::project(&self.turns, self.compaction.as_ref(), system, tools)
    }

    pub(crate) fn project(
        history: &[Turn],
        compaction: Option<&Compaction>,
        system: &str,
        tools: &[ToolSpec],
    ) -> Result<ModelInput> {
        let projected;
        let history = if let Some(c) = compaction {
            let suffix = history
                .get(c.through..)
                .ok_or_else(|| Error::new("invalid_compaction"))?;
            let mut prefix = vec![Turn::User(format!(
                "Conversation summary (historical context): {}",
                c.summary
            ))];
            // A long tool loop can be compacted mid-run. Keep its original user
            // request verbatim, outside the generated summary.
            if !suffix.is_empty()
                && !matches!(suffix.first(), Some(Turn::User(_)))
                && let Some(index) = history[..c.through]
                    .iter()
                    .rposition(|t| matches!(t, Turn::User(_)))
            {
                prefix.push(history[index].clone());
                prefix.extend(
                    history[index + 1..c.through]
                        .iter()
                        .take_while(|t| matches!(t, Turn::Context(_)))
                        .cloned(),
                );
            }
            projected = [prefix, suffix.to_vec()].concat();
            &projected[..]
        } else {
            history
        };
        let Some(Turn::User(text)) = history.first() else {
            return Err(Error::new("invalid_session_history"));
        };
        Ok(ModelInput {
            system: system.to_owned(),
            tools: tools.to_vec(),
            text: text.clone(),
            turns: history[1..].to_vec(),
        })
    }

    pub(crate) async fn checkpoint(&mut self, store: &dyn SessionStore, id: &str) -> Result<()> {
        self.seal()?;
        store.save(id, self.revision, self).await?;
        self.revision += 1;
        Ok(())
    }
    /// Complete protocol pairs without pretending an interrupted tool succeeded.
    /// A persisted tool-start with no result is uncertain, never replayed here.
    pub(crate) fn settle_pending(&mut self, code: &str) {
        let mut pending: BTreeMap<String, Call> = BTreeMap::new();
        for turn in &self.turns {
            match turn {
                Turn::Assistant(reply) => {
                    for call in &reply.calls {
                        pending.insert(call.id.clone(), call.clone());
                    }
                }
                Turn::Tools(results) => {
                    for result in results {
                        pending.remove(&result.call.id);
                    }
                }
                Turn::User(_) | Turn::Context(_) => {}
            }
        }
        if !pending.is_empty() {
            self.turns.push(Turn::Tools(pending.into_values().map(|call| Observation {
                call, output: ToolOutput { changed: false, content: json!({"error":code,"message":"No durable result is available. Inspect the destination before attempting this operation again; it may already have taken effect."}) },
            }).collect()));
        }
    }
}

fn json_prefix<T: Serialize>(values: &[T], prefix: &[T]) -> bool {
    values
        .get(..prefix.len())
        .is_some_and(|v| serde_json::to_value(v).ok() == serde_json::to_value(prefix).ok())
}
/// Call only after the host has established that the named run is no longer
/// live. Revision fencing prevents concurrent recovery from overwriting a run.
/// This does NOT execute tools, discard receipts, or claim exactly-once effects.
pub async fn recover_interrupted(store: &dyn SessionStore, id: &str, run: &str) -> Result<()> {
    let mut session = store.load(id).await?;
    session.ensure_supported()?;
    if session.active_run.as_deref() != Some(run) {
        return Err(Error::new("session_conflict"));
    }
    let observations = session
        .turns
        .get(session.active_start..)
        .ok_or_else(|| Error::new("invalid_session_history"))?
        .iter()
        .filter_map(|t| match t {
            Turn::Tools(results) => Some(results.clone()),
            _ => None,
        })
        .flatten()
        .collect();
    if let Some(delivery) = session.active_delivery.take() {
        session.completed.insert(
            delivery,
            Report {
                run_id: run.into(),
                output: None,
                error: Some(Error::new("interrupted_outcome_unknown")),
                observations,
                usage: None,
                generation_ms: 0,
                execution_ms: None,
            },
        );
    }
    session.settle_pending("interrupted_outcome_unknown");
    session.active_run = None;
    session.checkpoint(store, id).await
}
