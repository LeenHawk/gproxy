use gproxy_protocol::{claude, openai};

use crate::TransformError;
use crate::common::native::shape;

use super::ClaudeCall;

pub(super) fn web_search(
    id: String,
    action: Option<openai::WebSearchAction>,
) -> Result<ClaudeCall, TransformError> {
    let input = action
        .map(serde_json::to_value)
        .transpose()?
        .map(shape::value_object)
        .unwrap_or_default();
    Ok(ClaudeCall {
        id: id.clone(),
        name: "web_search".into(),
        input,
    })
}

pub(super) fn code_interpreter(
    id: String,
    code: Option<String>,
    container_id: String,
) -> ClaudeCall {
    let mut input = claude::JsonObject::new();
    if let Some(code) = code {
        input.insert("code".into(), code.into());
    }
    input.insert("container_id".into(), container_id.into());
    ClaudeCall {
        id: id.clone(),
        name: "code_execution".into(),
        input,
    }
}

pub(super) fn tool_search(
    arguments: serde_json::Value,
    _item_id: Option<String>,
    call_id: Option<String>,
    execution: Option<openai::ToolSearchExecution>,
) -> Result<ClaudeCall, TransformError> {
    let id = call_id
        .ok_or_else(|| TransformError::shape("OpenAI tool_search call", "call_id is missing"))?;
    let name = match execution {
        Some(openai::ToolSearchExecution::Client) => "tool_search_tool_regex",
        Some(openai::ToolSearchExecution::Server) => "tool_search_tool_bm25",
        Some(openai::ToolSearchExecution::Unknown(value)) => {
            return Err(TransformError::unsupported(
                "OpenAI tool_search execution",
                value,
            ));
        }
        None => {
            return Err(TransformError::shape(
                "OpenAI tool_search call",
                "execution is missing",
            ));
        }
    };
    Ok(ClaudeCall {
        id,
        name: name.into(),
        input: shape::value_object(arguments),
    })
}

pub(super) fn mcp(
    id: String,
    arguments: String,
    name: String,
) -> Result<ClaudeCall, TransformError> {
    Ok(ClaudeCall {
        id: id.clone(),
        name,
        input: shape::arguments_object(&arguments)?,
    })
}

pub(super) fn program(
    _id: String,
    call_id: String,
    code: String,
    fingerprint: String,
) -> ClaudeCall {
    let input = [
        ("code".into(), code.into()),
        ("fingerprint".into(), fingerprint.into()),
    ]
    .into_iter()
    .collect();
    ClaudeCall {
        id: call_id,
        name: "program".into(),
        input,
    }
}
