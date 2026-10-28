//! Built-in organization capabilities share the same immutable binding contract as packages.
use crate::{domain::*, store::Store};
use harness3::Tool;
use serde_json::Value;
use std::sync::Arc;
pub fn packages() -> Vec<Value> {
    vec![
        crate::messaging::package(),
        crate::collaboration::package(),
        crate::delegation::package(),
    ]
}
pub fn package(id: &str) -> Option<Value> {
    match id {
        "platform.messaging" => Some(crate::messaging::package()),
        "platform.delegation" => Some(crate::delegation::package()),
        "platform.context" => Some(crate::collaboration::package()),
        _ => None,
    }
}
pub fn tools(snapshot: &Value, store: Store, run: &str) -> PResult<Vec<Arc<dyn Tool>>> {
    let id = snapshot["id"].as_str().unwrap_or_default();
    if package(id).as_ref() != Some(snapshot) {
        return Err(fault(409, "organization_plugin_snapshot_unsupported"));
    }
    Ok(match id {
        "platform.messaging" => vec![crate::messaging::tool(store, run)],
        "platform.delegation" => crate::delegation::tools(store, run),
        "platform.context" => crate::collaboration::tools(store, run),
        _ => unreachable!(),
    })
}
