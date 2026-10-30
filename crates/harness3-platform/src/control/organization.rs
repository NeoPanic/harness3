//! Built-in organization capabilities share the same immutable binding contract as packages.
use crate::{domain::*, store::Store};
use harness3::Tool;
use serde_json::Value;
use std::sync::Arc;
pub fn packages() -> Vec<Value> {
    let mut packages = vec![
        crate::messaging::package(),
        crate::collaboration::package(),
        crate::delegation::package(),
        crate::channels::package(),
        crate::autonomy::package(),
        crate::workspaces::package(),
    ];
    packages.extend(harness3_builtin_plugins::packages());
    packages
}
pub fn package(id: &str) -> Option<Value> {
    match id {
        "platform.tasks" => Some(crate::autonomy::package()),
        "platform.channels" => Some(crate::channels::package()),
        "platform.workspace" => Some(crate::workspaces::package()),
        "platform.messaging" => Some(crate::messaging::package()),
        "platform.delegation" => Some(crate::delegation::package()),
        "platform.context" => Some(crate::collaboration::package()),
        _ => harness3_builtin_plugins::package(id),
    }
}
pub fn tools(snapshot: &Value, store: Store, run: &str) -> PResult<Vec<Arc<dyn Tool>>> {
    let id = snapshot["id"].as_str().unwrap_or_default();
    if package(id).as_ref() != Some(snapshot) {
        return Err(fault(409, "organization_plugin_snapshot_unsupported"));
    }
    Ok(match id {
        "platform.tasks" => crate::autonomy::tools(store, run),
        "platform.channels" => crate::channels::tools(store, run),
        "platform.workspace" => crate::workspaces::tools(store, run),
        "platform.messaging" => vec![crate::messaging::tool(store, run)],
        "platform.delegation" => crate::delegation::tools(store, run),
        "platform.context" => crate::collaboration::tools(store, run),
        _ => harness3_builtin_plugins::tools(id)
            .ok_or_else(|| fault(404, "builtin_plugin_not_found"))?,
    })
}
