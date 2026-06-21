use std::collections::BTreeMap;

use crate::protocol::claude;
use serde_json::Value;

pub(super) fn json_value<T: serde::Serialize>(value: T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

pub(super) fn json_object(value: Value) -> BTreeMap<String, Value> {
    match value {
        Value::Object(map) => map.into_iter().collect(),
        _ => BTreeMap::new(),
    }
}

pub(super) fn claude_json_schema(schema: BTreeMap<String, Value>) -> claude::JsonSchema {
    let mut properties = BTreeMap::new();
    let mut required = Vec::new();
    let mut extra = schema;

    if let Some(Value::Object(values)) = extra.remove("properties") {
        properties = values.into_iter().collect();
    }
    if let Some(Value::Array(values)) = extra.remove("required") {
        required = values
            .into_iter()
            .filter_map(|value| value.as_str().map(str::to_owned))
            .collect();
    }

    claude::JsonSchema {
        type_: claude::JsonSchemaObjectType::Known(claude::JsonSchemaObjectTypeKnown::Object),
        properties,
        required,
        extra,
    }
}

pub(super) fn empty_string_to_none(value: String) -> Option<String> {
    if value.is_empty() { None } else { Some(value) }
}

pub(super) fn non_empty_vec<T>(value: Vec<T>) -> Option<Vec<T>> {
    if value.is_empty() { None } else { Some(value) }
}
