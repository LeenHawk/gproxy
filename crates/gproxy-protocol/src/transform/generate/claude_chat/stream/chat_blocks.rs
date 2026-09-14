use super::*;
#[derive(Default)]
pub(super) struct Tool {
    pub name: String,
    pub arguments: String,
    pub id: Option<String>,
}
impl Tool {
    pub fn append(
        &mut self,
        id: Option<String>,
        function: Option<q::DeltaFunctionCall>,
    ) -> Result<(), TransformError> {
        if let Some(id) = id {
            if id.is_empty() || self.id.as_ref().is_some_and(|v| v != &id) {
                return Err(invalid("tool ID changed"));
            }
            self.id = Some(id);
        }
        if let Some(function) = function {
            if let Some(v) = function.name.flatten() {
                self.name.push_str(&v);
            }
            if let Some(v) = function.arguments.flatten() {
                self.arguments.push_str(&v);
            }
        }
        Ok(())
    }
    pub fn object(&self) -> Result<serde_json::Map<String, serde_json::Value>, TransformError> {
        if self.name.is_empty() {
            return Err(TransformError::missing_metadata("tool.name"));
        }
        serde_json::from_str(&self.arguments).map_err(|e| {
            TransformError::invalid_result(
                "tool.arguments",
                format!("Claude requires a complete JSON object: {e}"),
            )
        })
    }
}
pub(super) fn text_start(index: i64) -> s::StreamEvent {
    s::StreamEvent::ContentBlockStart(
        s::ContentBlockStartEvent::builder(
            index,
            c::ResponseContentBlock::Text(
                c::ResponseTextBlock::builder(c::ResponseTextBlockType::Tag, String::new()).build(),
            ),
        )
        .build(),
    )
}
pub(super) fn text_delta(index: i64, text: String) -> s::StreamEvent {
    s::StreamEvent::ContentBlockDelta(
        s::ContentBlockDeltaEvent::builder(
            index,
            s::ContentBlockDelta::Text(s::TextDelta::builder(text).build()),
        )
        .build(),
    )
}
pub(super) fn block_stop(index: i64) -> s::StreamEvent {
    s::StreamEvent::ContentBlockStop(s::ContentBlockStopEvent::builder(index).build())
}
pub(super) fn usage_delta(usage: c::Usage) -> s::MessageDeltaUsage {
    s::MessageDeltaUsage {
        output_tokens: usage.output_tokens,
        input_tokens: Some(Some(usage.input_tokens)),
        cache_creation_input_tokens: usage.cache_creation_input_tokens,
        cache_read_input_tokens: usage.cache_read_input_tokens,
        output_tokens_details: usage.output_tokens_details,
        server_tool_use: usage.server_tool_use,
        iterations: usage.iterations,
        fallback_credit: usage.fallback_credit,
        rest: Default::default(),
    }
}
