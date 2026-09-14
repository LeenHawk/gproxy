use super::*;
use crate::{
    transform::generate::stream::responses::{ResponsesStreamCollector, ResponsesStreamLimits},
    wire::openai::responses::{self as r, stream as s},
};
impl sealed::Event for s::StreamEvent {}
impl NativeEvent for s::StreamEvent {
    type Full = r::GenerateContentResponseBody;
    type Collector = ResponsesStreamCollector;
    const DIALECT: Dialect = Dialect::OpenAi;
    fn collector(_: IdentityFlow, _: TargetIdPolicy, limits: EventLimits) -> Self::Collector {
        ResponsesStreamCollector::new(ResponsesStreamLimits {
            max_events: limits.max_events,
            max_bytes: limits.max_bytes,
            max_items: limits.max_items,
            max_text_bytes: limits.max_bytes,
            max_json_bytes: limits.max_bytes,
        })
    }
    fn collect(collector: &mut Self::Collector, event: Self) -> Result<(), TransformError> {
        collector.push(event)
    }
    fn collected(
        collector: Self::Collector,
    ) -> Result<Converted<Collected<Self::Full>>, TransformError> {
        let value = collector.finish()?;
        Ok(Converted {
            value: Collected::native(value.value),
            report: value.report,
        })
    }
    fn event_name(&self) -> Option<&'static str> {
        Some(match self {
            Self::Created(_) => "response.created",
            Self::Queued(_) => "response.queued",
            Self::InProgress(_) => "response.in_progress",
            Self::Completed(_) => "response.completed",
            Self::Failed(_) => "response.failed",
            Self::Incomplete(_) => "response.incomplete",
            Self::OutputItemAdded(_) => "response.output_item.added",
            Self::OutputItemDone(_) => "response.output_item.done",
            Self::ContentPartAdded(_) => "response.content_part.added",
            Self::ContentPartDone(_) => "response.content_part.done",
            Self::OutputTextDelta(_) => "response.output_text.delta",
            Self::OutputTextDone(_) => "response.output_text.done",
            Self::OutputTextAnnotationAdded(_) => "response.output_text.annotation.added",
            Self::RefusalDelta(_) => "response.refusal.delta",
            Self::RefusalDone(_) => "response.refusal.done",
            Self::ReasoningTextDelta(_) => "response.reasoning_text.delta",
            Self::ReasoningTextDone(_) => "response.reasoning_text.done",
            Self::ReasoningSummaryPartAdded(_) => "response.reasoning_summary_part.added",
            Self::ReasoningSummaryPartDone(_) => "response.reasoning_summary_part.done",
            Self::ReasoningSummaryTextDelta(_) => "response.reasoning_summary_text.delta",
            Self::ReasoningSummaryTextDone(_) => "response.reasoning_summary_text.done",
            Self::FunctionCallArgumentsDelta(_) => "response.function_call_arguments.delta",
            Self::FunctionCallArgumentsDone(_) => "response.function_call_arguments.done",
            Self::CustomToolInputDelta(_) => "response.custom_tool_call_input.delta",
            Self::CustomToolInputDone(_) => "response.custom_tool_call_input.done",
            Self::CodeInterpreterCodeDelta(_) => "response.code_interpreter_call_code.delta",
            Self::CodeInterpreterCodeDone(_) => "response.code_interpreter_call_code.done",
            Self::AudioDelta(_) => "response.audio.delta",
            Self::AudioDone(_) => "response.audio.done",
            Self::AudioTranscriptDelta(_) => "response.audio.transcript.delta",
            Self::AudioTranscriptDone(_) => "response.audio.transcript.done",
            Self::ImagePartial(_) => "response.image_generation_call.partial_image",
            Self::ImageCall(_) => "response.image_generation_call.in_progress",
            Self::ImageGenerating(_) => "response.image_generation_call.generating",
            Self::ImageCompleted(_) => "response.image_generation_call.completed",
            Self::CodeInterpreterInProgress(_) => "response.code_interpreter_call.in_progress",
            Self::CodeInterpreterInterpreting(_) => "response.code_interpreter_call.interpreting",
            Self::CodeInterpreterCompleted(_) => "response.code_interpreter_call.completed",
            Self::FileSearchInProgress(_) => "response.file_search_call.in_progress",
            Self::FileSearchSearching(_) => "response.file_search_call.searching",
            Self::FileSearchCompleted(_) => "response.file_search_call.completed",
            Self::WebSearchInProgress(_) => "response.web_search_call.in_progress",
            Self::WebSearchSearching(_) => "response.web_search_call.searching",
            Self::WebSearchCompleted(_) => "response.web_search_call.completed",
            Self::McpArgumentsDelta(_) => "response.mcp_call_arguments.delta",
            Self::McpArgumentsDone(_) => "response.mcp_call_arguments.done",
            Self::McpInProgress(_) => "response.mcp_call.in_progress",
            Self::McpCompleted(_) => "response.mcp_call.completed",
            Self::McpFailed(_) => "response.mcp_call.failed",
            Self::McpListToolsInProgress(_) => "response.mcp_list_tools.in_progress",
            Self::McpListToolsCompleted(_) => "response.mcp_list_tools.completed",
            Self::McpListToolsFailed(_) => "response.mcp_list_tools.failed",
            Self::Error(_) => "error",
        })
    }
    fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed(_) | Self::Incomplete(_) | Self::Failed(_) | Self::Error(_)
        )
    }
    fn tool_declarations(
        &self,
        _complete_tool_names: bool,
    ) -> Vec<(String, ToolCallKind, Option<String>)> {
        let items: Vec<&r::ResponseOutputItem> = match self {
            Self::Created(v) => v.response.output.iter().collect(),
            Self::Queued(v) => v.response.output.iter().collect(),
            Self::InProgress(v) => v.response.output.iter().collect(),
            Self::Completed(v) => v.response.output.iter().collect(),
            Self::Incomplete(v) => v.response.output.iter().collect(),
            Self::OutputItemAdded(v) | Self::OutputItemDone(v) => vec![&v.item],
            _ => Vec::new(),
        };
        items
            .into_iter()
            .filter_map(|item| match item {
                r::ResponseOutputItem::FunctionCall(v) => Some((
                    v.call_id.clone(),
                    ToolCallKind::Function,
                    (!v.name.is_empty()).then(|| v.name.clone()),
                )),
                r::ResponseOutputItem::CustomToolCall(v) => Some((
                    v.call_id.clone(),
                    ToolCallKind::Custom,
                    (!v.name.is_empty()).then(|| v.name.clone()),
                )),
                _ => None,
            })
            .collect()
    }
}
