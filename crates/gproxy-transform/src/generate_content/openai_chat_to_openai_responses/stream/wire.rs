use gproxy_protocol::openai;

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
