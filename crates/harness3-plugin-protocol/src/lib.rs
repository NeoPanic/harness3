//! Portable v1 manifest and strict, bounded JSON Schema subset.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub const PROTOCOL: &str = "harness3-plugin-rpc/v1";
pub const API: &str = "harness3.plugins/v1";
pub const MAX_FRAME: u64 = 128 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub api_version: String,
    pub id: String,
    pub version: String,
    pub description: String,
    pub instructions: String,
    pub runtime: Runtime,
    /// Relative package path -> lowercase SHA-256 hex. All executable dependencies must be listed.
    pub files: BTreeMap<String, String>,
    pub tools: Vec<Tool>,
    pub timeout_ms: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Runtime {
    pub protocol: String,
    pub entry: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub output_schema: Value,
    pub mutates: bool,
    #[serde(default)]
    pub parallel_safe: bool,
}
fn name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 80
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}
pub fn package_path(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 200
        && s.split('/')
            .all(|part| name(part) && part != "." && part != "..")
}
impl Manifest {
    pub fn validate(&self) -> Result<(), String> {
        if self.api_version != API
            || self.runtime.protocol != PROTOCOL
            || !name(&self.id)
            || !self.id.contains('.')
            || !name(&self.version)
            || self.description.trim().is_empty()
            || self.description.len() > 2000
            || self.instructions.len() > 32000
            || !(100..=30000).contains(&self.timeout_ms)
            || self.files.is_empty()
            || self.files.len() > 32
            || !self.files.contains_key(&self.runtime.entry)
            || !package_path(&self.runtime.entry)
            || self
                .files
                .keys()
                .any(|p| !package_path(p) || p == "manifest.json")
            || self.files.values().any(|h| {
                h.len() != 64
                    || !h
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            })
            || self.tools.is_empty()
            || self.tools.len() > 16
        {
            return Err("invalid_manifest".into());
        }
        let mut names = BTreeSet::new();
        for tool in &self.tools {
            if !name(&tool.name)
                || tool.name.contains('.')
                || !names.insert(&tool.name)
                || format!("{}.{}", self.id, tool.name).len() > 100
                || tool.description.trim().is_empty()
                || tool.description.len() > 2000
                || (tool.mutates && tool.parallel_safe)
            {
                return Err("invalid_tool_declaration".into());
            }
            check_schema(&tool.input_schema)?;
            check_schema(&tool.output_schema)?;
            if tool.input_schema["type"] != "object" {
                return Err("tool_input_must_be_object".into());
            }
        }
        Ok(())
    }
}
/// Reject unsupported keywords instead of silently accepting a weaker contract.
pub fn check_schema(schema: &Value) -> Result<(), String> {
    fn walk(s: &Value, depth: usize, nodes: &mut usize) -> Result<(), String> {
        *nodes += 1;
        let o = s.as_object().ok_or("schema_must_be_object")?;
        if depth > 16
            || *nodes > 512
            || o.keys().any(|k| {
                ![
                    "type",
                    "description",
                    "title",
                    "properties",
                    "required",
                    "additionalProperties",
                    "items",
                    "enum",
                    "minimum",
                    "maximum",
                    "minLength",
                    "maxLength",
                    "minItems",
                    "maxItems",
                ]
                .contains(&k.as_str())
            })
        {
            return Err("unsupported_or_oversized_schema".into());
        }
        let kind = s["type"].as_str().ok_or("schema_type_required")?;
        if ![
            "object", "array", "string", "integer", "number", "boolean", "null",
        ]
        .contains(&kind)
        {
            return Err("unsupported_schema_type".into());
        }
        if let Some(v) = o.get("enum")
            && (v.as_array().is_none_or(|a| a.is_empty() || a.len() > 100))
        {
            return Err("invalid_schema_enum".into());
        }
        for key in ["description", "title"] {
            if let Some(v) = o.get(key)
                && !v.is_string()
            {
                return Err("invalid_schema_annotation".into());
            }
        }
        for (min, max, types, integer) in [
            ("minimum", "maximum", &["integer", "number"][..], false),
            ("minLength", "maxLength", &["string"][..], true),
            ("minItems", "maxItems", &["array"][..], true),
        ] {
            for key in [min, max] {
                if let Some(v) = o.get(key)
                    && (!types.contains(&kind)
                        || if integer {
                            v.as_u64().is_none()
                        } else {
                            v.as_f64().is_none()
                        })
                {
                    return Err("invalid_schema_bound".into());
                }
            }
            if let (Some(min), Some(max)) = (s[min].as_f64(), s[max].as_f64())
                && min > max
            {
                return Err("invalid_schema_bound".into());
            }
        }
        match kind {
            "object" => {
                if !s["additionalProperties"].is_boolean() {
                    return Err("additional_properties_must_be_explicit".into());
                }
                let props = s["properties"]
                    .as_object()
                    .ok_or("schema_properties_required")?;
                if props.len() > 64 {
                    return Err("too_many_schema_properties".into());
                }
                for nested in props.values() {
                    walk(nested, depth + 1, nodes)?;
                }
                if let Some(required) = o.get("required") {
                    let required = required.as_array().ok_or("invalid_required")?;
                    let mut seen = BTreeSet::new();
                    for key in required {
                        let key = key.as_str().ok_or("invalid_required")?;
                        if !props.contains_key(key) || !seen.insert(key) {
                            return Err("invalid_required".into());
                        }
                    }
                }
            }
            "array" => walk(&s["items"], depth + 1, nodes)?,
            _ => {}
        }
        for key in ["properties", "required", "additionalProperties"] {
            if o.contains_key(key) && kind != "object" {
                return Err("schema_keyword_type_mismatch".into());
            }
        }
        if o.contains_key("items") && kind != "array" {
            return Err("schema_keyword_type_mismatch".into());
        }
        Ok(())
    }
    walk(schema, 0, &mut 0)
}
/// Validate a value against an already checked schema.
pub fn validate_value(schema: &Value, value: &Value) -> Result<(), String> {
    let kind = schema["type"].as_str().ok_or("invalid_schema")?;
    let matches = match kind {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "integer" => value.is_i64() || value.is_u64(),
        "number" => value.is_number(),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        _ => false,
    };
    if !matches
        || schema
            .get("enum")
            .is_some_and(|e| !e.as_array().unwrap().contains(value))
    {
        return Err("schema_value_mismatch".into());
    }
    if let Some(object) = value.as_object() {
        let props = schema["properties"].as_object().ok_or("invalid_schema")?;
        if schema["additionalProperties"] == false && object.keys().any(|k| !props.contains_key(k))
        {
            return Err("unexpected_property".into());
        }
        if let Some(required) = schema["required"].as_array()
            && required
                .iter()
                .any(|k| !object.contains_key(k.as_str().unwrap()))
        {
            return Err("missing_property".into());
        }
        for (key, nested) in props {
            if let Some(v) = object.get(key) {
                validate_value(nested, v)?;
            }
        }
    }
    if let Some(array) = value.as_array() {
        bounds(schema, array.len() as f64, "minItems", "maxItems")?;
        for v in array {
            validate_value(&schema["items"], v)?;
        }
    }
    if let Some(text) = value.as_str() {
        bounds(
            schema,
            text.chars().count() as f64,
            "minLength",
            "maxLength",
        )?;
    }
    if let Some(number) = value.as_f64() {
        let integer = |v: &Value| {
            v.as_i64()
                .map(i128::from)
                .or_else(|| v.as_u64().map(i128::from))
        };
        for key in ["minimum", "maximum"] {
            if let Some(bound) = schema.get(key) {
                let violates = if let (Some(a), Some(b)) = (integer(value), integer(bound)) {
                    if key == "minimum" { a < b } else { a > b }
                } else if key == "minimum" {
                    number < bound.as_f64().unwrap()
                } else {
                    number > bound.as_f64().unwrap()
                };
                if violates {
                    return Err("schema_bound_exceeded".into());
                }
            }
        }
    }
    Ok(())
}
fn bounds(schema: &Value, value: f64, min: &str, max: &str) -> Result<(), String> {
    if schema[min].as_f64().is_some_and(|v| value < v)
        || schema[max].as_f64().is_some_and(|v| value > v)
    {
        Err("schema_bound_exceeded".into())
    } else {
        Ok(())
    }
}
