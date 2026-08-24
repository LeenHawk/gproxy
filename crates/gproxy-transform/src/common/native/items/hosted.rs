use gproxy_protocol::{claude, openai};

use crate::TransformError;
use crate::common::native::shape;

use super::ClaudeCall;

pub(super) fn web_search(
    id: String,
    action: openai::WebSearchAction,
    mut rest: openai::Rest,
) -> Result<ClaudeCall, TransformError> {
    rest.insert("openai_native_tool".into(), "web_search_call".into());
    Ok(ClaudeCall {
        id: id.clone(),
        name: "web_search".into(),
        input: shape::value_object(serde_json::to_value(action)?),
        item_id: Some(id),
        rest,
    })
}

pub(super) fn code_interpreter(
    id: String,
    code: Option<String>,
    container_id: String,
    mut rest: openai::Rest,
) -> ClaudeCall {
    let mut input = claude::JsonObject::new();
    if let Some(code) = code {
        input.insert("code".into(), code.into());
    }
    input.insert("container_id".into(), container_id.into());
    rest.insert("openai_native_tool".into(), "code_interpreter_call".into());
    ClaudeCall {
        id: id.clone(),
        name: "code_execution".into(),
        input,
        item_id: Some(id),
        rest,
    }
}

pub(super) fn tool_search(
    arguments: serde_json::Value,
    item_id: Option<String>,
    call_id: Option<String>,
    execution: Option<openai::ToolSearchExecution>,
    mut rest: openai::Rest,
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
    rest.insert("openai_native_tool".into(), "tool_search_call".into());
    Ok(ClaudeCall {
        id,
        name: name.into(),
        input: shape::value_object(arguments),
        item_id,
        rest,
    })
}

pub(super) fn mcp(
    id: String,
    arguments: String,
    name: String,
    mut rest: openai::Rest,
) -> Result<ClaudeCall, TransformError> {
    rest.insert("openai_native_tool".into(), "mcp_call".into());
    Ok(ClaudeCall {
        id: id.clone(),
        name,
        input: shape::arguments_object(&arguments)?,
        item_id: Some(id),
        rest,
    })
}

pub(super) fn program(
    id: String,
    call_id: String,
    code: String,
    fingerprint: String,
    mut rest: openai::Rest,
) -> ClaudeCall {
    let input = [
        ("code".into(), code.into()),
        ("fingerprint".into(), fingerprint.into()),
    ]
    .into_iter()
    .collect();
    rest.insert("openai_native_tool".into(), "program".into());
    ClaudeCall {
        id: call_id,
        name: "program".into(),
        input,
        item_id: Some(id),
        rest,
    }
}
