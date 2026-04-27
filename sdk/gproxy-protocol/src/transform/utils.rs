use std::borrow::Cow;
use std::collections::{BTreeMap, HashSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransformError {
    pub message: Cow<'static, str>,
}

impl TransformError {
    /// Construct a `TransformError` with a static string message.
    ///
    /// Kept for backwards compatibility with `TryFrom` impls that use
    /// compile-time string literals for "not yet supported" cases.
    pub const fn not_implemented(message: &'static str) -> Self {
        Self {
            message: Cow::Borrowed(message),
        }
    }

    /// Construct a `TransformError` with a dynamically-built message.
    ///
    /// Used by the runtime transform dispatcher in `crate::transform::dispatch`
    /// which reports errors like "no stream aggregation for protocol: {protocol}".
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: Cow::Owned(message.into()),
        }
    }
}

impl Display for TransformError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for TransformError {}

pub type TransformResult<T> = Result<T, TransformError>;

// `push_message_block` lives next to the other Claude-side helpers in
// `transform::claude::utils`. Re-exported here so that callers reach it via
// the generic `transform::utils` path without a cross-module dependency on
// the `claude` submodule.
pub use crate::transform::claude::utils::{ORPHAN_TOOL_USE_PLACEHOLDER_NAME, push_message_block};

/// Patch a JSON Schema in place so it satisfies Anthropic's strict-mode
/// requirements: every `object` node gets `additionalProperties: false`, and
/// every key in `properties` is added to `required`.
///
/// Anthropic rejects object schemas that omit `additionalProperties`, set it
/// to `true`, or fail to list every property in `required`. OpenAI/Gemini
/// schemas typically don't satisfy these constraints.
pub fn enforce_anthropic_strict_schema(schema: &mut BTreeMap<String, Value>) {
    let mut tmp: serde_json::Map<String, Value> = std::mem::take(schema).into_iter().collect();
    enforce_anthropic_strict_value_map(&mut tmp);
    *schema = tmp.into_iter().collect();
}

fn enforce_anthropic_strict_value(value: &mut Value) {
    match value {
        Value::Object(map) => enforce_anthropic_strict_value_map(map),
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                enforce_anthropic_strict_value(v);
            }
        }
        _ => {}
    }
}

fn enforce_anthropic_strict_value_map(map: &mut serde_json::Map<String, Value>) {
    if let Some(Value::Object(props)) = map.get_mut("properties") {
        for (_, v) in props.iter_mut() {
            enforce_anthropic_strict_value(v);
        }
    }
    if let Some(items) = map.get_mut("items") {
        enforce_anthropic_strict_value(items);
    }
    for key in ["$defs", "definitions"] {
        if let Some(Value::Object(defs)) = map.get_mut(key) {
            for (_, v) in defs.iter_mut() {
                enforce_anthropic_strict_value(v);
            }
        }
    }
    for key in ["allOf", "anyOf", "oneOf"] {
        if let Some(Value::Array(arr)) = map.get_mut(key) {
            for v in arr.iter_mut() {
                enforce_anthropic_strict_value(v);
            }
        }
    }

    let is_object_schema = map.get("type").and_then(|v| v.as_str()) == Some("object")
        || map.contains_key("properties");
    if !is_object_schema {
        return;
    }

    map.insert("additionalProperties".to_string(), Value::Bool(false));

    let prop_keys: Vec<String> = map
        .get("properties")
        .and_then(|v| v.as_object())
        .map(|props| props.keys().cloned().collect())
        .unwrap_or_default();
    if prop_keys.is_empty() {
        return;
    }

    let required = map
        .entry("required".to_string())
        .or_insert_with(|| Value::Array(Vec::new()));
    if let Value::Array(arr) = required {
        let existing: HashSet<String> = arr
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect();
        for key in prop_keys {
            if !existing.contains(&key) {
                arr.push(Value::String(key));
            }
        }
    }
}

#[cfg(test)]
mod enforce_anthropic_strict_schema_tests {
    use super::*;
    use serde_json::json;

    fn run(input: serde_json::Value) -> serde_json::Value {
        let mut schema: BTreeMap<String, Value> =
            input.as_object().unwrap().clone().into_iter().collect();
        enforce_anthropic_strict_schema(&mut schema);
        Value::Object(schema.into_iter().collect())
    }

    #[test]
    fn top_level_object_gets_additional_properties_and_required() {
        let out = run(json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "age": {"type": "integer"}
            }
        }));
        assert_eq!(out["additionalProperties"], json!(false));
        let required: HashSet<String> = out["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            required,
            ["name", "age"].iter().map(|s| s.to_string()).collect()
        );
    }

    #[test]
    fn nested_objects_in_properties_and_array_items_are_patched() {
        let out = run(json!({
            "type": "object",
            "properties": {
                "user": {
                    "type": "object",
                    "properties": {"name": {"type": "string"}}
                },
                "tags": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {"id": {"type": "string"}}
                    }
                }
            }
        }));
        assert_eq!(
            out["properties"]["user"]["additionalProperties"],
            json!(false)
        );
        assert_eq!(out["properties"]["user"]["required"], json!(["name"]));
        assert_eq!(
            out["properties"]["tags"]["items"]["additionalProperties"],
            json!(false)
        );
        assert_eq!(
            out["properties"]["tags"]["items"]["required"],
            json!(["id"])
        );
    }

    #[test]
    fn defs_and_anyof_branches_are_patched() {
        let out = run(json!({
            "type": "object",
            "properties": {"x": {"$ref": "#/$defs/X"}},
            "$defs": {
                "X": {"type": "object", "properties": {"a": {"type": "string"}}}
            },
            "anyOf": [
                {"type": "object", "properties": {"b": {"type": "integer"}}}
            ]
        }));
        assert_eq!(out["$defs"]["X"]["additionalProperties"], json!(false));
        assert_eq!(out["$defs"]["X"]["required"], json!(["a"]));
        assert_eq!(out["anyOf"][0]["additionalProperties"], json!(false));
        assert_eq!(out["anyOf"][0]["required"], json!(["b"]));
    }

    #[test]
    fn existing_additional_properties_true_is_overwritten() {
        let out = run(json!({
            "type": "object",
            "additionalProperties": true,
            "properties": {"k": {"type": "string"}}
        }));
        assert_eq!(out["additionalProperties"], json!(false));
    }

    #[test]
    fn existing_required_is_extended_not_replaced() {
        let out = run(json!({
            "type": "object",
            "required": ["a"],
            "properties": {"a": {"type": "string"}, "b": {"type": "string"}}
        }));
        let required: HashSet<String> = out["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        assert_eq!(required, ["a", "b"].iter().map(|s| s.to_string()).collect());
    }

    #[test]
    fn non_object_schemas_are_left_alone() {
        let out = run(json!({"type": "string", "format": "uuid"}));
        assert!(out.get("additionalProperties").is_none());
        assert!(out.get("required").is_none());
    }
}
