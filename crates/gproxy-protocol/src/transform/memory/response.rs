use crate::{
    codec::{self, CodecLimits},
    transform::TransformError,
    wire::{
        claude::generate_content as c,
        gemini as g,
        openai::{chat as o, responses as r},
    },
};

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Output {
    trace_summary: String,
    memory_summary: String,
}

pub fn parse_output(text: &str, limits: CodecLimits) -> Result<(String, String), TransformError> {
    let value: Output = codec::decode_json(text.as_bytes(), limits).map_err(|e| {
        TransformError::new(
            if e.kind() == codec::CodecErrorKind::Limit {
                crate::transform::TransformErrorKind::Limit
            } else {
                crate::transform::TransformErrorKind::InvalidResult
            },
            "memory.output",
            e.to_string(),
        )
    })?;
    Ok((value.trace_summary, value.memory_summary))
}

fn nonempty(text: String) -> Result<String, TransformError> {
    if text.is_empty() {
        Err(TransformError::invalid_result(
            "memory.response",
            "empty generated text",
        ))
    } else {
        Ok(text)
    }
}

pub fn claude_text(input: c::GenerateContentResponseBody) -> Result<String, TransformError> {
    if input.stop_reason != c::StopReason::EndTurn {
        return Err(TransformError::invalid_result(
            "memory.stop_reason",
            "memory requires fully completed end_turn",
        ));
    }
    let mut text = String::new();
    for block in input.content {
        match block {
            c::ResponseContentBlock::Text(v) => text.push_str(&v.text),
            c::ResponseContentBlock::Thinking(_) | c::ResponseContentBlock::RedactedThinking(_) => {
            }
            c::ResponseContentBlock::ToolUse(_)
            | c::ResponseContentBlock::ServerToolUse(_)
            | c::ResponseContentBlock::WebSearchToolResult(_)
            | c::ResponseContentBlock::WebFetchToolResult(_)
            | c::ResponseContentBlock::AdvisorToolResult(_)
            | c::ResponseContentBlock::CodeExecutionToolResult(_)
            | c::ResponseContentBlock::BashCodeExecutionToolResult(_)
            | c::ResponseContentBlock::TextEditorCodeExecutionToolResult(_)
            | c::ResponseContentBlock::ToolSearchToolResult(_)
            | c::ResponseContentBlock::McpToolUse(_)
            | c::ResponseContentBlock::McpToolResult(_)
            | c::ResponseContentBlock::ContainerUpload(_)
            | c::ResponseContentBlock::McpToolListing(_)
            | c::ResponseContentBlock::Compaction(_)
            | c::ResponseContentBlock::Fallback(_) => {
                return Err(TransformError::invalid_result(
                    "memory.content",
                    "unexpected tool/resource state instead of summary",
                ));
            }
        }
    }
    nonempty(text)
}

pub fn gemini_text(input: g::GenerateContentResponseBody) -> Result<String, TransformError> {
    if input
        .prompt_feedback
        .as_ref()
        .and_then(|v| v.block_reason)
        .is_some_and(|v| v != g::BlockReason::Unspecified)
    {
        return Err(TransformError::invalid_result(
            "memory.prompt_feedback",
            "prompt blocked",
        ));
    }
    let candidates = input
        .candidates
        .ok_or_else(|| TransformError::invalid_result("memory.candidates", "missing candidate"))?;
    if candidates.len() != 1 {
        return Err(TransformError::invalid_result(
            "memory.candidates",
            "exactly one summary candidate required",
        ));
    }
    let candidate = candidates.into_iter().next().unwrap();
    if candidate.finish_reason != Some(g::FinishReason::Stop)
        || candidate.index.is_some_and(|v| v != 0)
    {
        return Err(TransformError::invalid_result(
            "memory.candidate",
            "nonterminal/failed or wrong index",
        ));
    }
    let content = candidate
        .content
        .ok_or_else(|| TransformError::invalid_result("memory.content", "missing content"))?;
    if content.role.as_deref().is_some_and(|v| v != "model") {
        return Err(TransformError::invalid_result(
            "memory.role",
            "model role required",
        ));
    }
    let mut text = String::new();
    for part in content.parts.unwrap_or_default() {
        if part.inline_data.is_some()
            || part.file_data.is_some()
            || part.function_call.is_some()
            || part.function_response.is_some()
            || part.executable_code.is_some()
            || part.code_execution_result.is_some()
            || part.tool_call.is_some()
            || part.tool_response.is_some()
        {
            return Err(TransformError::invalid_result(
                "memory.part",
                "unexpected nontext summary payload",
            ));
        }
        if part.thought != Some(true) {
            text.push_str(part.text.as_deref().unwrap_or(""));
        }
    }
    nonempty(text)
}

pub fn chat_text(input: o::GenerateContentResponseBody) -> Result<String, TransformError> {
    if input.choices.len() != 1 {
        return Err(TransformError::invalid_result(
            "memory.choices",
            "exactly one choice required",
        ));
    }
    let choice = input.choices.into_iter().next().unwrap();
    if choice.index != 0
        || choice.finish_reason != o::FinishReason::Stop
        || choice.message.refusal.is_some()
        || choice.message.function_call.is_some()
        || choice
            .message
            .tool_calls
            .as_ref()
            .is_some_and(|v| !v.is_empty())
        || choice.message.audio.flatten().is_some()
    {
        return Err(TransformError::invalid_result(
            "memory.choice",
            "refused/truncated/tool/audio response is not a summary",
        ));
    }
    nonempty(
        choice
            .message
            .content
            .ok_or_else(|| TransformError::invalid_result("memory.content", "missing content"))?,
    )
}

pub fn responses_text(input: r::GenerateContentResponseBody) -> Result<String, TransformError> {
    if input.status != Some(r::ResponseStatus::Completed)
        || input.error.is_some()
        || input.incomplete_details.is_some()
    {
        return Err(TransformError::invalid_result(
            "memory.status",
            "Responses must be completed without error/incomplete state",
        ));
    }
    let mut text = String::new();
    for item in input.output {
        if crate::transform::generate::multi_agent::excluded_output(&item) {
            continue;
        }
        match item {
            r::ResponseOutputItem::Message(message) => {
                if message.status != r::OutputMessageStatus::Completed {
                    return Err(TransformError::invalid_result(
                        "memory.message.status",
                        "message not completed",
                    ));
                }
                for part in message.content {
                    match part {
                        r::OutputContent::Text(part) => text.push_str(&part.text),
                        r::OutputContent::Refusal(_) => {
                            return Err(TransformError::invalid_result(
                                "memory.refusal",
                                "refused summary",
                            ));
                        }
                    }
                }
            }
            r::ResponseOutputItem::Reasoning(value) => {
                if matches!(
                    value.status,
                    Some(r::ReasoningStatus::InProgress | r::ReasoningStatus::Incomplete)
                ) {
                    return Err(TransformError::invalid_result(
                        "memory.reasoning.status",
                        "completed response contains unfinished reasoning",
                    ));
                }
            }
            r::ResponseOutputItem::FunctionCall(_)
            | r::ResponseOutputItem::FunctionCallOutput(_)
            | r::ResponseOutputItem::FileSearchCall(_)
            | r::ResponseOutputItem::WebSearchCall(_)
            | r::ResponseOutputItem::ComputerCall(_)
            | r::ResponseOutputItem::ComputerCallOutput(_)
            | r::ResponseOutputItem::Program(_)
            | r::ResponseOutputItem::ProgramOutput(_)
            | r::ResponseOutputItem::ToolSearchCall(_)
            | r::ResponseOutputItem::ToolSearchOutput(_)
            | r::ResponseOutputItem::AdditionalTools(_)
            | r::ResponseOutputItem::MultiAgentCall(_)
            | r::ResponseOutputItem::MultiAgentCallOutput(_)
            | r::ResponseOutputItem::AgentMessage(_)
            | r::ResponseOutputItem::ConfigurationUpdate(_)
            | r::ResponseOutputItem::Compaction(_)
            | r::ResponseOutputItem::ImageGenerationCall(_)
            | r::ResponseOutputItem::CodeInterpreterCall(_)
            | r::ResponseOutputItem::LocalShellCall(_)
            | r::ResponseOutputItem::LocalShellCallOutput(_)
            | r::ResponseOutputItem::ShellCall(_)
            | r::ResponseOutputItem::ShellCallOutput(_)
            | r::ResponseOutputItem::ApplyPatchCall(_)
            | r::ResponseOutputItem::ApplyPatchCallOutput(_)
            | r::ResponseOutputItem::McpCall(_)
            | r::ResponseOutputItem::McpListTools(_)
            | r::ResponseOutputItem::McpApprovalRequest(_)
            | r::ResponseOutputItem::McpApprovalResponse(_)
            | r::ResponseOutputItem::CustomToolCall(_)
            | r::ResponseOutputItem::CustomToolCallOutput(_) => {
                return Err(TransformError::invalid_result(
                    "memory.output",
                    "unexpected native execution output",
                ));
            }
        }
    }
    if let Some(Some(alias)) = input.output_text {
        if !text.is_empty() && text != alias {
            return Err(TransformError::invalid_result(
                "memory.output_text",
                "alias conflicts with typed output",
            ));
        }
        if text.is_empty() {
            text = alias;
        }
    }
    nonempty(text)
}
