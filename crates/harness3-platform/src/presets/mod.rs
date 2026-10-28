//! Immutable single-parent publication, capability locks and instance parameters.
use crate::{
    domain::*,
    store::{Store, event},
};
use harness3_plugin_protocol::{check_schema, validate_value};
use rusqlite::{OptionalExtension, Transaction, params};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Parent {
    name: String,
    version: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Draft {
    name: String,
    version: String,
    extends: Option<Parent>,
    purpose: Option<String>,
    objective: Option<String>,
    instructions: Option<String>,
    protocol: Option<Protocol>,
    plugin: Option<PluginRef>,
    plugins: Option<Vec<PluginRef>>,
    parameters_schema: Option<Value>,
    parameter_defaults: Option<Map<String, Value>>,
}

/// Read legacy single bindings and current ordered multi-bindings uniformly.
pub fn bindings(snapshot: &Value) -> Vec<Value> {
    snapshot
        .get("plugin_snapshots")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_else(|| {
            snapshot
                .get("plugin_snapshot")
                .cloned()
                .into_iter()
                .collect()
        })
}
fn core(snapshot: &Value) -> PResult<Preset> {
    let mut value = snapshot.clone();
    value
        .as_object_mut()
        .ok_or_else(|| fault(500, "invalid_preset_snapshot"))?
        .retain(|key, _| {
            [
                "name",
                "version",
                "purpose",
                "objective",
                "instructions",
                "protocol",
                "plugin",
            ]
            .contains(&key.as_str())
        });
    Ok(serde_json::from_value(value)?)
}
fn lock_plugin(tx: &Transaction<'_>, reference: &PluginRef) -> PResult<Value> {
    if let Some(package) = crate::organization::package(&reference.id) {
        return if package["version"] == reference.version {
            Ok(package)
        } else {
            Err(fault(404, "plugin_not_found"))
        };
    }
    if !valid_name(&reference.id) || !valid_name(&reference.version) {
        return Err(fault(400, "invalid_plugin_reference"));
    }
    if let Some((body, digest)) = tx
        .query_row(
            "SELECT body,digest FROM plugins WHERE id=? AND version=?",
            params![reference.id, reference.version],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )
        .optional()?
    {
        let mut value: Value = serde_json::from_str(&body)?;
        value["digest"] = digest.into();
        return Ok(value);
    }
    let (body, state): (String, String) = tx
        .query_row(
            "SELECT body,state FROM process_plugins WHERE id=? AND version=?",
            params![reference.id, reference.version],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| fault(404, "plugin_not_found"))?;
    if state != "enabled" {
        return Err(fault(403, "plugin_disabled"));
    }
    let value: Value = serde_json::from_str(&body)?;
    let process: crate::plugins::Snapshot = serde_json::from_value(value.clone())?;
    process
        .manifest
        .validate()
        .map_err(|_| fault(500, "invalid_plugin_snapshot"))?;
    Ok(value)
}
fn template(text: &str, mut replace: impl FnMut(&str) -> PResult<String>) -> PResult<String> {
    let mut remaining = text;
    let mut output = String::new();
    while let Some(start) = remaining.find("{{") {
        let prefix = &remaining[..start];
        if prefix.contains("}}") {
            return Err(fault(400, "invalid_parameter_template"));
        }
        output.push_str(prefix);
        let tail = &remaining[start + 2..];
        let end = tail
            .find("}}")
            .ok_or_else(|| fault(400, "invalid_parameter_template"))?;
        let key = tail[..end].trim();
        if !valid_name(key) {
            return Err(fault(400, "invalid_parameter_template"));
        }
        output.push_str(&replace(key)?);
        remaining = &tail[end + 2..];
    }
    if remaining.contains("}}") {
        return Err(fault(400, "invalid_parameter_template"));
    }
    output.push_str(remaining);
    Ok(output)
}
fn parameter_contract(snapshot: &Value) -> PResult<()> {
    let defaults = snapshot
        .get("parameter_defaults")
        .and_then(Value::as_object);
    let Some(schema) = snapshot.get("parameters_schema") else {
        if defaults.is_some_and(|d| !d.is_empty()) {
            return Err(fault(400, "parameters_schema_required"));
        }
        for field in ["purpose", "objective", "instructions"] {
            template(snapshot[field].as_str().unwrap_or_default(), |_| {
                Err(fault(400, "unknown_template_parameter"))
            })?;
        }
        return Ok(());
    };
    check_schema(schema).map_err(|_| fault(400, "invalid_parameters_schema"))?;
    if schema["type"] != "object" || schema["additionalProperties"] != false {
        return Err(fault(400, "parameters_schema_must_be_closed_object"));
    }
    let properties = schema["properties"]
        .as_object()
        .ok_or_else(|| fault(400, "invalid_parameters_schema"))?;
    if properties.len() > 32 || properties.keys().any(|k| !valid_name(k)) {
        return Err(fault(400, "invalid_parameter_names"));
    }
    if let Some(defaults) = defaults {
        if serde_json::to_vec(defaults)?.len() > 16384 {
            return Err(fault(400, "parameters_too_large"));
        }
        for (key, value) in defaults {
            let schema = properties
                .get(key)
                .ok_or_else(|| fault(400, "unknown_default_parameter"))?;
            validate_value(schema, value).map_err(|_| fault(400, "invalid_default_parameter"))?;
        }
    }
    for field in ["purpose", "objective", "instructions"] {
        template(snapshot[field].as_str().unwrap_or_default(), |key| {
            if !properties.contains_key(key) {
                return Err(fault(400, "unknown_template_parameter"));
            }
            // A referenced parameter must always have a value at instantiation.
            if !defaults.is_some_and(|d| d.contains_key(key))
                && !schema["required"]
                    .as_array()
                    .is_some_and(|a| a.iter().any(|v| v == key))
            {
                return Err(fault(
                    400,
                    "template_parameter_must_be_required_or_defaulted",
                ));
            }
            Ok(String::new())
        })?;
    }
    Ok(())
}

/// Resolve parameters once before fleet admission. Replacements are literal and never re-expanded.
pub fn instantiate(mut snapshot: Value, supplied: &Map<String, Value>) -> PResult<Value> {
    let mut parameters = snapshot
        .get("parameter_defaults")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    parameters.extend(supplied.clone());
    if serde_json::to_vec(&parameters)?.len() > 16384 {
        return Err(fault(400, "parameters_too_large"));
    }
    if let Some(schema) = snapshot.get("parameters_schema") {
        validate_value(schema, &Value::Object(parameters.clone()))
            .map_err(|_| fault(400, "invalid_instance_parameters"))?;
    } else if !parameters.is_empty() {
        return Err(fault(400, "parameters_not_declared"));
    }
    for field in ["purpose", "objective", "instructions"] {
        let rendered = template(snapshot[field].as_str().unwrap_or_default(), |key| {
            let value = parameters
                .get(key)
                .ok_or_else(|| fault(400, "missing_template_parameter"))?;
            Ok(value
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| value.to_string()))
        })?;
        snapshot[field] = rendered.into();
    }
    core(&snapshot)?.validate()?;
    if snapshot.get("parameters_schema").is_some() {
        snapshot["parameters"] = parameters.into();
    }
    Ok(snapshot)
}

impl Store {
    pub fn publish_definition(&self, input: Value) -> PResult<Value> {
        self.publish_checked(input, |_| Ok(()))
    }
    pub fn publish_checked(
        &self,
        input: Value,
        validate_protocol: impl FnOnce(&Protocol) -> PResult<()>,
    ) -> PResult<Value> {
        if input.as_object().is_some_and(|fields| {
            fields
                .iter()
                .any(|(key, value)| key != "plugin" && value.is_null())
        }) {
            return Err(fault(400, "preset_field_cannot_be_null"));
        }
        let draft: Draft = serde_json::from_value(input.clone())
            .map_err(|_| fault(400, "invalid_preset_definition"))?;
        if !valid_name(&draft.name) || !valid_name(&draft.version) {
            return Err(fault(400, "invalid_preset"));
        }
        self.transaction(|tx| {
            if let Some(body)=tx.query_row("SELECT body FROM presets WHERE name=? AND version=?",params![draft.name,draft.version],|r|r.get::<_,String>(0)).optional()? {
                let stored:Value=serde_json::from_str(&body)?;
                let legacy=serde_json::to_value(core(&stored)?)?;
                if stored.get("publication").unwrap_or(&legacy)!=&input {return Err(fault(409,"preset_version_immutable"));}
                return Ok(stored);
            }
            let mut snapshot=if let Some(parent)=&draft.extends {
                if !valid_name(&parent.name)||!valid_name(&parent.version)||(parent.name==draft.name&&parent.version==draft.version) {return Err(fault(400,"invalid_preset_parent"));}
                let body:String=tx.query_row("SELECT body FROM presets WHERE name=? AND version=?",params![parent.name,parent.version],|r|r.get(0)).optional()?.ok_or_else(||fault(404,"parent_preset_not_found"))?;
                serde_json::from_str(&body)?
            } else {json!({"plugin":null})};
            let mut lineage=snapshot.get("lineage").and_then(Value::as_array).cloned().unwrap_or_default();
            if let Some(parent)=&draft.extends {
                lineage.push(json!({"name":parent.name,"version":parent.version}));
                if lineage.len()>16 || lineage.iter().any(|p|p["name"]==draft.name&&p["version"]==draft.version) {return Err(fault(400,"preset_inheritance_depth_or_cycle"));}
            }
            snapshot["name"]=draft.name.clone().into();snapshot["version"]=draft.version.clone().into();
            for (field,value) in [("purpose",draft.purpose.as_ref()),("objective",draft.objective.as_ref()),("instructions",draft.instructions.as_ref())] {
                if let Some(value)=value {snapshot[field]=value.clone().into();}
                if !snapshot[field].is_string() {return Err(fault(400,"preset_field_required"));}
            }
            if let Some(protocol)=&draft.protocol {snapshot["protocol"]=serde_json::to_value(protocol)?;}
            if snapshot.get("protocol").is_none() {return Err(fault(400,"preset_protocol_required"));}
            if input.get("plugin").is_some() && draft.plugins.is_some() {return Err(fault(400,"ambiguous_plugin_bindings"));}
            let replacing=input.get("plugin").is_some() || draft.plugins.is_some();
            let references:Vec<PluginRef>=if let Some(plugins)=draft.plugins {plugins} else if replacing {draft.plugin.into_iter().collect()} else {
                bindings(&snapshot).into_iter().map(|p|serde_json::from_value(json!({"id":p["id"],"version":p["version"]}))).collect::<Result<_,_>>()?
            };
            if references.len()>16 {return Err(fault(400,"too_many_plugin_bindings"));}
            let mut ids=BTreeSet::new();let mut locks=Vec::new();let mut tool_names=BTreeSet::from(["text.inspect".to_owned()]);let mut instruction_bytes=0;
            for reference in &references {
                if !ids.insert(&reference.id) {return Err(fault(400,"duplicate_plugin_binding"));}
                let lock=lock_plugin(tx,reference)?;
                instruction_bytes+=lock["instructions"].as_str().or_else(||lock["manifest"]["instructions"].as_str()).unwrap_or_default().len();
                if lock["kind"]=="organization" {for tool in lock["tools"].as_array().unwrap(){if !tool_names.insert(tool["name"].as_str().unwrap().to_owned()){return Err(fault(400,"plugin_tool_name_collision"));}}}
                if lock["kind"]=="process" {
                    let manifest:&Value=&lock["manifest"];
                    if let Some(tools)=manifest["tools"].as_array() {for tool in tools {
                        let name=format!("{}.{}",reference.id,tool["name"].as_str().unwrap_or_default());
                        if !tool_names.insert(name) {return Err(fault(400,"plugin_tool_name_collision"));}
                    }}
                }
                locks.push(lock);
            }
            if instruction_bytes>64000 || tool_names.len()>64 {return Err(fault(400,"preset_capability_limit"));}
            let object=snapshot.as_object_mut().ok_or_else(||fault(500,"invalid_preset_snapshot"))?;
            for key in ["plugin_snapshot","plugin_snapshots","plugins","extends","lineage","publication"] {object.remove(key);}
            snapshot["plugin"]=if references.len()==1 {serde_json::to_value(&references[0])?} else {Value::Null};
            // Preserve the legacy single-binding representation for existing consumers.
            if locks.len()==1 {snapshot["plugin_snapshot"]=locks.remove(0);} else if !locks.is_empty() {snapshot["plugins"]=serde_json::to_value(&references)?;snapshot["plugin_snapshots"]=locks.into();}
            if let Some(schema)=draft.parameters_schema {snapshot["parameters_schema"]=schema;}
            if let Some(defaults)=draft.parameter_defaults {snapshot["parameter_defaults"]=defaults.into();}
            let preset=core(&snapshot)?;preset.validate()?;parameter_contract(&snapshot)?;validate_protocol(&preset.protocol)?;
            if let Some(parent)=draft.extends {snapshot["extends"]=json!({"name":parent.name,"version":parent.version});snapshot["lineage"]=lineage.into();}
            snapshot["publication"]=input;
            let body=serde_json::to_string(&snapshot)?;
            if body.len()>1024*1024 {return Err(fault(400,"preset_snapshot_too_large"));}
            tx.execute("INSERT INTO presets(name,version,body,created_at) VALUES(?,?,?,?)",params![draft.name,draft.version,body,now()])?;
            event(tx,"preset.published",&draft.name,json!({"version":draft.version,"extends":snapshot.get("extends"),"bindings":references.len()}))?;
            Ok(snapshot)
        })
    }
}

pub fn runtime_preset(snapshot: &Value) -> PResult<Preset> {
    core(snapshot)
}
