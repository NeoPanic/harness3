use harness3::{session::SessionStore, *};
use harness3_models::Models;
use harness3_platform::{domain::*, store::Store};
use serde_json::{Value, json};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::Mutex;

pub type Cancellations = Arc<Mutex<HashMap<String, CancellationToken>>>;

#[cfg(feature = "test-support")]
#[path = "../tests/support/model.rs"]
mod fixture;
mod recovery;
mod runner;
mod scheduler;
#[cfg(feature = "test-support")]
use fixture::DemoModel;
pub use recovery::recover;
pub use scheduler::{worker_loop, worker_loop_with_plugins, worker_loop_with_router};

/// Whether the offline regression model is compiled in (development tests only).
pub const fn test_model_enabled() -> bool {
    cfg!(feature = "test-support")
}
