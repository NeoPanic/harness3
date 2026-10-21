//! Explicit outbound delivery. Registration is not authorization to send.
use crate::{CancellationToken, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct Outbound {
    /// Host ledger identity, stable across recovery; no automatic retry.
    pub id: String,
    pub recipient: String,
    pub text: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Receipt {
    pub external_id: String,
}

/// Bind credentials, account and authorization context in the host adapter,
/// never in a serializable message. An error may mean an uncertain send.
#[async_trait]
pub trait Transport: Send + Sync {
    async fn send(&self, message: &Outbound, cancel: CancellationToken) -> Result<Receipt>;
}
