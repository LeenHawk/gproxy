//! Sparse backend envelopes precede complete protocol items. Keep their unknown
//! fields intact while repairing lifecycle; native tool payloads are validated
//! with the protocol types in `tools` once their input is complete.
use crate::channel::ChannelError;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Event {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sequence_number: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_index: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delta: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, flatten)]
    pub rest: serde_json::Map<String, Value>,
}

impl Event {
    pub fn new(kind: &str) -> Self {
        Self {
            kind: kind.into(),
            sequence_number: None,
            output_index: None,
            item_id: None,
            item: None,
            response: None,
            delta: None,
            arguments: None,
            input: None,
            name: None,
            rest: Default::default(),
        }
    }
    pub fn item(kind: &str, index: u32, item: Value) -> Self {
        let mut event = Self::new(kind);
        event.output_index = Some(index);
        event.item = Some(item);
        event
    }
    pub fn index(&self) -> Result<u32, ChannelError> {
        self.output_index
            .ok_or_else(|| invalid("output_index missing"))
    }
    pub fn terminal(&self) -> bool {
        matches!(
            self.kind.as_str(),
            "response.completed" | "response.incomplete" | "response.failed" | "error"
        )
    }
    pub fn input_field(&self) -> Option<&'static str> {
        match self.kind.as_str() {
            "response.function_call_arguments.delta" | "response.function_call_arguments.done" => {
                Some("arguments")
            }
            "response.custom_tool_call_input.delta" | "response.custom_tool_call_input.done" => {
                Some("input")
            }
            _ => None,
        }
    }
}

pub(super) fn invalid(error: impl std::fmt::Display) -> ChannelError {
    ChannelError::InvalidResponse(format!("Codex Responses SSE: {error}"))
}

pub(super) fn created(response: &Value) -> Result<Event, ChannelError> {
    let id = response
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("response id missing"))?;
    // Supply the required response fields for cross-dialect stream decoders.
    // Copy actual backend metadata when available, but never publish final
    // output, errors or usage in the synthetic start event.
    let mut body = json!({"id":id,"object":"response","created_at":0,"model":"",
        "error":null,"incomplete_details":null,"instructions":null,"metadata":null,
        "parallel_tool_calls":true,"temperature":null,"top_p":null,"tool_choice":"auto","tools":[]});
    body.as_object_mut().unwrap().extend(
        response
            .as_object()
            .ok_or_else(|| invalid("response must be an object"))?
            .clone(),
    );
    body["status"] = json!("in_progress");
    body["output"] = json!([]);
    body["usage"] = Value::Null;
    body["error"] = Value::Null;
    body["incomplete_details"] = Value::Null;
    body.as_object_mut().unwrap().remove("completed_at");
    let mut event = Event::new("response.created");
    event.response = Some(body);
    Ok(event)
}
