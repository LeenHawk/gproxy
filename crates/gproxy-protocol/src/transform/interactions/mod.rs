//! Direct Interactions ↔ Responses, Chat and Claude pairs. No intermediate
//! vendor dialect is used. Only declared source fields enter these mappings.
mod content;
mod request;
mod response;
pub mod stream;
pub use request::request;
pub use response::response;

use super::{Converted, Report, TransformError};
use crate::{Dialect, Operation, OperationKey, wire::DeclaredFields};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

fn declared<T: DeserializeOwned + Serialize + DeclaredFields>(
    value: Value,
) -> Result<Value, TransformError> {
    let typed: T = serde_json::from_value(value)?;
    Ok(serde_json::to_value(typed.into_declared())?)
}

fn native(key: OperationKey) -> bool {
    matches!(
        key.operation,
        Operation::GenerateContent | Operation::StreamGenerateContent
    ) && key.dialect == Dialect::GeminiInteractions
}
fn array(value: &Value) -> impl Iterator<Item = &Value> {
    value.as_array().into_iter().flatten()
}
fn string(value: &Value) -> &str {
    value.as_str().unwrap_or_default()
}
fn copy(source: &Value, target: &mut Value, from: &str, to: &str) {
    if let Some(value) = source.get(from).filter(|value| !value.is_null()) {
        target[to] = value.clone();
    }
}
fn args(value: &Value) -> Result<Value, TransformError> {
    if let Some(value) = value.as_str() {
        Ok(serde_json::from_str(value)?)
    } else {
        Ok(value.clone())
    }
}
fn arguments(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string())
}
fn text(value: &Value) -> String {
    if let Some(value) = value.as_str() {
        return value.to_owned();
    }
    array(value)
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n")
}
fn omitted(source: &Value, fields: &[&str], report: &mut Report) {
    for field in fields {
        if source.get(*field).is_some_and(|value| !value.is_null()) {
            report.omitted(*field, "no equivalent on the target protocol");
        }
    }
}

const SIGNATURE: &str = crate::transform::generate::signature::INTERACTIONS;
const REASONING_FORMAT: &str = "google-gemini-interactions-v1";

fn claude_call_id(id: &Value) -> String {
    use crate::transform::identity::tool_alias;
    let id = string(id);
    if tool_alias::accepts(Dialect::Claude, id) && tool_alias::decode(id).is_none() {
        id.to_owned()
    } else {
        tool_alias::upstream("toolu_", id, 0)
    }
}

fn native_call_id(id: &Value) -> String {
    use crate::transform::identity::tool_alias::{self, ToolAlias};
    match tool_alias::decode(string(id)) {
        Some(ToolAlias::Upstream { id, .. }) => id,
        _ => string(id).to_owned(),
    }
}
