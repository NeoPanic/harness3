pub mod api;
mod bootstrap;
pub use bootstrap::{Options, up};

pub type ServerResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
pub mod mcp;
