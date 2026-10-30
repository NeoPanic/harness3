//! A host-facing driver seam. Hosts can replace the loop without changing
//! channels, stores or callers. A driver owns its turn until it has settled.
use crate::{CancellationToken, Harness, InboundEvent, Report, Result, session::SessionStore};
use async_trait::async_trait;
#[async_trait]
pub trait AgentDriver: Send + Sync {
    async fn run_turn(
        &self,
        store: &dyn SessionStore,
        session: &str,
        event: InboundEvent,
        model: &str,
        cancel: CancellationToken,
    ) -> Result<Report>;
}
#[async_trait]
impl AgentDriver for Harness {
    async fn run_turn(
        &self,
        store: &dyn SessionStore,
        session: &str,
        event: InboundEvent,
        model: &str,
        cancel: CancellationToken,
    ) -> Result<Report> {
        self.run_session(store, session, event, model, cancel).await
    }
}
