//! Local control-plane preview around the independent Harness³ runtime.
pub mod collaboration;
pub mod control;
pub mod domain;
pub mod entrypoints;
pub mod limits;
pub mod models;
pub mod plugins;
pub mod scheduling;
pub mod storage;
pub mod worker;
pub mod workspaces;

// Preserve existing module paths while implementations move behind their boundaries.
pub use collaboration::{
    autonomy, channels, claims, coordination, delegation, messaging, research, verification,
};
pub use control::{fleets, lifecycle, organization, presets};
pub use entrypoints::{client, mcp, server::api};
pub use scheduling::{board, decision, routing};
pub use storage::sqlite as store;
pub use worker::runner as runtime;
pub use workspaces::artifacts;
