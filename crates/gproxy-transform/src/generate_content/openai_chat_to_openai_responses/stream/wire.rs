use gproxy_protocol::openai;

use crate::TransformError;

pub(super) fn coordinates(
    event: &openai::KnownResponseStreamEvent,
    content: bool,
) -> Result<(String, u32), TransformError> {
    let item_id = required(event.item_id.clone(), "item_id")?;
    let output_index = required(event.output_index, "output_index")?;
    if content {
        required(event.content_index, "content_index")?;
    }
    Ok((item_id, output_index))
}

pub(super) fn required<T>(value: Option<T>, field: &str) -> Result<T, TransformError> {
    value.ok_or_else(|| TransformError::shape("Responses stream", format!("{field} missing")))
}

pub(super) fn merge_rest(target: &mut openai::Rest, source: openai::Rest) {
    target.extend(source);
}

pub(super) fn empty_delta() -> openai::ChatDelta {
    openai::ChatDelta {
        role: None,
        content: None,
        reasoning_content: None,
        refusal: None,
        tool_calls: None,
        function_call: None,
        obfuscation: None,
        rest: Default::default(),
    }
}

pub(super) fn response_item_name(item: &openai::TypedResponseItem) -> String {
    serde_json::to_value(item)
        .ok()
        .and_then(|value| {
            value
                .get("type")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "unsupported item".into())
}
