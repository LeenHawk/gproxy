//! Restore the Guardian client's Responses SSE contract after validation.

use super::{GuardianClassifyInvocation, GuardianNativeResponse, GuardianReviewInvocation};
use crate::{
    HttpBody, WireResponse,
    codec::{self, CodecLimits, SseEncoder, SseEvent},
    transform::{
        Converted, Report, TransformError, TransformErrorKind,
        generate::{
            chat_responses::{self, ResponsesResponseContext},
            claude_responses::{self, ClaudeResponseContext},
            gemini_responses::{self, GeminiResponseContext},
            stream::responses::{self, ResponsesStreamLimits},
        },
        guardian::{self, GuardianExtraction, GuardianOperation},
        identity::{IdentityFlow, TargetIdPolicy},
    },
    wire::{
        DeclaredFields,
        openai::responses::{input as i, response as r, stream as s},
    },
};

/// Concrete invocation facts used by the existing direct response pair. Claude
/// and Gemini need a factual timestamp, and missing usage details need actual
/// host facts. No timestamp or zero counter is supplied by this adapter.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum GuardianStreamContext {
    Responses,
    Chat(ResponsesResponseContext),
    Claude(ClaudeResponseContext),
    Gemini(GeminiResponseContext),
}

#[derive(Debug)]
pub enum GuardianStreamInvocation {
    Success {
        result: GuardianExtraction,
        response: WireResponse<HttpBody>,
        report: Report,
    },
    Rejected(WireResponse<HttpBody>),
    Invalid {
        error: TransformError,
        native: Box<GuardianNativeResponse>,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct GuardianStreamLimits {
    pub codec: CodecLimits,
    pub stream: ResponsesStreamLimits,
}

impl GuardianReviewInvocation {
    pub fn into_stream(
        self,
        context: GuardianStreamContext,
        flow: &mut IdentityFlow,
        limits: GuardianStreamLimits,
    ) -> Result<GuardianStreamInvocation, TransformError> {
        match self {
            Self::Rejected(response) => Ok(GuardianStreamInvocation::Rejected(response)),
            Self::Invalid { error, native } => Ok(GuardianStreamInvocation::Invalid {
                error,
                native: Box::new(native),
            }),
            Self::Success { result, native } => build(
                native,
                context,
                GuardianExtraction::Review(result),
                flow,
                limits,
            ),
        }
    }
}

impl GuardianClassifyInvocation {
    pub fn into_stream(
        self,
        context: GuardianStreamContext,
        flow: &mut IdentityFlow,
        limits: GuardianStreamLimits,
    ) -> Result<GuardianStreamInvocation, TransformError> {
        match self {
            Self::Rejected(response) => Ok(GuardianStreamInvocation::Rejected(response)),
            Self::Invalid { error, native } => Ok(GuardianStreamInvocation::Invalid {
                error,
                native: Box::new(native),
            }),
            Self::Success { result, native } => build(
                native,
                context,
                GuardianExtraction::Classify(result),
                flow,
                limits,
            ),
        }
    }
}

fn clean<T: DeclaredFields + serde::Serialize>(
    body: T,
    limits: CodecLimits,
) -> Result<T, TransformError> {
    let body = body.into_declared();
    codec::encode_json(&body, limits).map_err(encoding)?;
    Ok(body)
}

fn build(
    native: GuardianNativeResponse,
    context: GuardianStreamContext,
    expected: GuardianExtraction,
    flow: &mut IdentityFlow,
    limits: GuardianStreamLimits,
) -> Result<GuardianStreamInvocation, TransformError> {
    let mut ids = flow.clone();
    let policy = TargetIdPolicy::new(crate::Dialect::OpenAi);
    let (status, mut headers, original, converted) = match (native, context) {
        (GuardianNativeResponse::Responses(response), GuardianStreamContext::Responses) => {
            let body = clean(response.body, limits.codec)?;
            let text = crate::transform::memory::responses_text(body.clone())?;
            (
                response.status,
                response.headers,
                text,
                Converted {
                    value: body,
                    report: Report::default(),
                },
            )
        }
        (GuardianNativeResponse::Chat(response), GuardianStreamContext::Chat(context)) => {
            let body = clean(response.body, limits.codec)?;
            let text = crate::transform::memory::chat_text(body.clone())?;
            (
                response.status,
                response.headers,
                text,
                chat_responses::chat_to_responses_response(body, context, &mut ids, &policy)?,
            )
        }
        (GuardianNativeResponse::Claude(response), GuardianStreamContext::Claude(context)) => {
            let body = clean(response.body, limits.codec)?;
            let text = crate::transform::memory::claude_text(body.clone())?;
            (
                response.status,
                response.headers,
                text,
                claude_responses::claude_to_responses_response(body, context, &mut ids, &policy)?,
            )
        }
        (GuardianNativeResponse::Gemini(response), GuardianStreamContext::Gemini(context)) => {
            let body = clean(response.body, limits.codec)?;
            let text = crate::transform::memory::gemini_text(body.clone())?;
            (
                response.status,
                response.headers,
                text,
                gemini_responses::gemini_to_responses_response(body, context, &mut ids, &policy)?,
            )
        }
        _ => {
            return Err(TransformError::shape(
                "guardian.stream.context",
                "response dialect and concrete context must match",
            ));
        }
    };
    if !status.is_success() {
        return Err(TransformError::invalid_result(
            "guardian.status",
            "successful result contains non-success HTTP status",
        ));
    }
    let mut response = clean(converted.value, limits.codec)?;
    let mut report = converted.report;
    let operation = match &expected {
        GuardianExtraction::Review(_) => GuardianOperation::Review,
        GuardianExtraction::Classify(_) => GuardianOperation::Classify,
    };
    let text = match operation {
        GuardianOperation::Review => original,
        GuardianOperation::Classify => original.trim().to_owned(),
    };
    // Generic response pairs may represent separate text blocks with separators.
    // Guardian consumes one formal JSON document or one full label, so retain
    // the actual target text concatenation before applying its client projection.
    collapse_text(&mut response, text, &mut report)?;
    let actual = guardian::extract_responses(response.clone(), operation)?;
    if actual != expected {
        return Err(TransformError::invalid_result(
            "guardian.result",
            "validated label/result contradicts native response text",
        ));
    }
    let synthesized = responses::synthesize_responses_stream(response, &mut ids, limits.stream)?;
    report.diagnostics.extend(synthesized.report.diagnostics);
    let mut encoder = SseEncoder::new(limits.codec);
    let mut chunks = Vec::new();
    for event in synthesized.value {
        let data = codec::encode_json(&event, limits.codec).map_err(encoding)?;
        let frame = SseEvent {
            event: Some(event_name(&event).into()),
            id: None,
            data: String::from_utf8(data.to_vec())
                .map_err(|e| TransformError::invalid_result("guardian.sse", e.to_string()))?,
            retry: None,
        };
        chunks.push(encoder.event(&frame).map_err(encoding)?);
    }
    for name in [
        http::header::CONTENT_LENGTH,
        http::header::CONTENT_ENCODING,
        http::header::TRANSFER_ENCODING,
    ] {
        headers.remove(name);
    }
    headers.insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("text/event-stream"),
    );
    let body = HttpBody::Stream(Box::pin(futures_util::stream::iter(
        chunks.into_iter().map(Ok),
    )));
    *flow = ids;
    Ok(GuardianStreamInvocation::Success {
        result: actual,
        response: WireResponse {
            status,
            headers,
            body,
        },
        report,
    })
}

fn collapse_text(
    response: &mut r::GenerateContentResponseBody,
    text: String,
    report: &mut Report,
) -> Result<(), TransformError> {
    let mut found = false;
    let mut changed = false;
    let mut output = Vec::new();
    for item in std::mem::take(&mut response.output) {
        match item {
            r::ResponseOutputItem::Message(mut message) => {
                if found {
                    changed = true;
                    continue;
                }
                found = true;
                let exact = message.content.len() == 1
                    && matches!(&message.content[0],i::OutputContent::Text(v) if v.text==text);
                if !exact {
                    changed = true;
                    message.content = vec![i::OutputContent::Text(
                        i::ResponseOutputText::builder(
                            i::ResponseOutputTextType::ResponseOutputText,
                            text.clone(),
                            Vec::new(),
                            Vec::new(),
                        )
                        .build(),
                    )];
                }
                output.push(r::ResponseOutputItem::Message(message));
            }
            item => output.push(item),
        }
    }
    if !found {
        return Err(TransformError::invalid_result(
            "guardian.output",
            "missing assistant text message",
        ));
    }
    response.output = output;
    if response.output_text.is_some() {
        response.output_text = Some(Some(text));
    }
    if changed {
        report.changed("guardian.output_text","joined validated text into one complete client text event; displaced per-part annotations/logprobs are omitted");
    }
    Ok(())
}

fn encoding(error: crate::codec::CodecError) -> TransformError {
    TransformError::new(
        if error.kind() == crate::codec::CodecErrorKind::Limit {
            TransformErrorKind::Limit
        } else {
            TransformErrorKind::InvalidResult
        },
        "guardian.sse",
        error.to_string(),
    )
}

fn event_name(event: &s::StreamEvent) -> &'static str {
    match event {
        s::StreamEvent::Created(_) => "response.created",
        s::StreamEvent::Queued(_) => "response.queued",
        s::StreamEvent::InProgress(_) => "response.in_progress",
        s::StreamEvent::Completed(_) => "response.completed",
        s::StreamEvent::Failed(_) => "response.failed",
        s::StreamEvent::Incomplete(_) => "response.incomplete",
        s::StreamEvent::OutputItemAdded(_) => "response.output_item.added",
        s::StreamEvent::OutputItemDone(_) => "response.output_item.done",
        s::StreamEvent::ContentPartAdded(_) => "response.content_part.added",
        s::StreamEvent::ContentPartDone(_) => "response.content_part.done",
        s::StreamEvent::OutputTextDelta(_) => "response.output_text.delta",
        s::StreamEvent::OutputTextDone(_) => "response.output_text.done",
        s::StreamEvent::OutputTextAnnotationAdded(_) => "response.output_text.annotation.added",
        s::StreamEvent::RefusalDelta(_) => "response.refusal.delta",
        s::StreamEvent::RefusalDone(_) => "response.refusal.done",
        s::StreamEvent::ReasoningTextDelta(_) => "response.reasoning_text.delta",
        s::StreamEvent::ReasoningTextDone(_) => "response.reasoning_text.done",
        s::StreamEvent::ReasoningSummaryPartAdded(_) => "response.reasoning_summary_part.added",
        s::StreamEvent::ReasoningSummaryPartDone(_) => "response.reasoning_summary_part.done",
        s::StreamEvent::ReasoningSummaryTextDelta(_) => "response.reasoning_summary_text.delta",
        s::StreamEvent::ReasoningSummaryTextDone(_) => "response.reasoning_summary_text.done",
        s::StreamEvent::FunctionCallArgumentsDelta(_) => "response.function_call_arguments.delta",
        s::StreamEvent::FunctionCallArgumentsDone(_) => "response.function_call_arguments.done",
        s::StreamEvent::CustomToolInputDelta(_) => "response.custom_tool_call_input.delta",
        s::StreamEvent::CustomToolInputDone(_) => "response.custom_tool_call_input.done",
        s::StreamEvent::CodeInterpreterCodeDelta(_) => "response.code_interpreter_call_code.delta",
        s::StreamEvent::CodeInterpreterCodeDone(_) => "response.code_interpreter_call_code.done",
        s::StreamEvent::AudioDelta(_) => "response.audio.delta",
        s::StreamEvent::AudioDone(_) => "response.audio.done",
        s::StreamEvent::AudioTranscriptDelta(_) => "response.audio.transcript.delta",
        s::StreamEvent::AudioTranscriptDone(_) => "response.audio.transcript.done",
        s::StreamEvent::ImagePartial(_) => "response.image_generation_call.partial_image",
        s::StreamEvent::ImageCall(_) => "response.image_generation_call.in_progress",
        s::StreamEvent::ImageGenerating(_) => "response.image_generation_call.generating",
        s::StreamEvent::ImageCompleted(_) => "response.image_generation_call.completed",
        s::StreamEvent::CodeInterpreterInProgress(_) => {
            "response.code_interpreter_call.in_progress"
        }
        s::StreamEvent::CodeInterpreterInterpreting(_) => {
            "response.code_interpreter_call.interpreting"
        }
        s::StreamEvent::CodeInterpreterCompleted(_) => "response.code_interpreter_call.completed",
        s::StreamEvent::FileSearchInProgress(_) => "response.file_search_call.in_progress",
        s::StreamEvent::FileSearchSearching(_) => "response.file_search_call.searching",
        s::StreamEvent::FileSearchCompleted(_) => "response.file_search_call.completed",
        s::StreamEvent::WebSearchInProgress(_) => "response.web_search_call.in_progress",
        s::StreamEvent::WebSearchSearching(_) => "response.web_search_call.searching",
        s::StreamEvent::WebSearchCompleted(_) => "response.web_search_call.completed",
        s::StreamEvent::McpArgumentsDelta(_) => "response.mcp_call_arguments.delta",
        s::StreamEvent::McpArgumentsDone(_) => "response.mcp_call_arguments.done",
        s::StreamEvent::McpInProgress(_) => "response.mcp_call.in_progress",
        s::StreamEvent::McpCompleted(_) => "response.mcp_call.completed",
        s::StreamEvent::McpFailed(_) => "response.mcp_call.failed",
        s::StreamEvent::McpListToolsInProgress(_) => "response.mcp_list_tools.in_progress",
        s::StreamEvent::McpListToolsCompleted(_) => "response.mcp_list_tools.completed",
        s::StreamEvent::McpListToolsFailed(_) => "response.mcp_list_tools.failed",
        s::StreamEvent::Error(_) => "error",
    }
}
