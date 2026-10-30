//! Generic scheduled work contracts; no domain vocabulary or storage dependency.
use crate::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkLease {
    pub id: String,
    pub subject: String,
    pub generation: i64,
    pub token: String,
    pub payload: Value,
}
#[async_trait]
pub trait WorkStore: Send + Sync {
    async fn claim(&self, now: i64, lease_seconds: u32) -> Result<Option<WorkLease>>;
    async fn settle(&self, lease: &WorkLease, error: Option<&str>) -> Result<()>;
}
