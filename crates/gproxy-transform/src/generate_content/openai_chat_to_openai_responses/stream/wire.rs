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
