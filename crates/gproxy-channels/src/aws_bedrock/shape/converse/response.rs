use bytes::Bytes;
use gproxy_channel_api::ChannelError;
use gproxy_protocol::{aws, claude};
use sha2::{Digest as _, Sha256};

pub(super) fn convert(body: &Bytes) -> Result<Bytes, ChannelError> {
    let response: aws::ConverseResponse = serde_json::from_slice(body)
        .map_err(|error| ChannelError::Observe(format!("Bedrock Converse JSON: {error}")))?;
    let aws::ConverseOutput::Message { message, rest } = response.output else {
        return Err(observe("Bedrock Converse response has no message"));
    };
    let mut output_rest = response.rest;
    output_rest.insert(
        "metrics".into(),
        serde_json::to_value(response.metrics).map_err(json_observe)?,
    );
    if let Some(fields) = response.additional_model_response_fields {
        output_rest.insert("additionalModelResponseFields".into(), fields);
    }
    if let Some(performance) = response.performance_config {
        output_rest.insert(
            "performanceConfig".into(),
            serde_json::to_value(performance).map_err(json_observe)?,
        );
    }
    output_rest.extend(rest);
    let output = claude::CreateMessageResponseBody {
        id: message_id(body),
        type_: claude::MessageObjectType::Known(claude::MessageObjectTypeKnown::Message),
        role: claude::AssistantRole::Known(claude::AssistantRoleKnown::Assistant),
        content: message
            .content
            .into_iter()
            .map(content)
            .collect::<Result<_, _>>()?,
        model: claude::ClaudeModel::Unknown("aws-bedrock".into()),
        stop_reason: stop_reason(response.stop_reason),
        stop_sequence: None,
        usage: super::response_usage::usage(&response.usage, response.service_tier.as_ref()),
        container: None,
        context_management: None,
        diagnostics: None,
        input_transformations: None,
        stop_details: None,
        rest: output_rest,
    };
    serde_json::to_vec(&output)
        .map(Bytes::from)
        .map_err(json_observe)
}

fn content(block: aws::ContentBlock) -> Result<claude::ContentBlock, ChannelError> {
    Ok(match block {
        aws::ContentBlock::Text { text, rest } => {
            claude::ResponseContentBlock::Text(claude::ResponseTextBlock {
                citations: None,
                text,
                type_: claude::TextBlockType::Text,
                rest,
            })
        }
        aws::ContentBlock::ToolUse { tool_use, rest } => {
            let input = tool_use
                .input
                .as_object()
                .cloned()
                .ok_or_else(|| observe("Bedrock tool input is not an object"))?;
            let mut block_rest = tool_use.rest;
            block_rest.extend(rest);
            claude::ResponseContentBlock::ToolUse(claude::ResponseToolUseBlock {
                id: tool_use.tool_use_id,
                input,
                name: tool_use.name,
                type_: claude::ToolUseBlockType::ToolUse,
                caller: None,
                rest: block_rest,
            })
        }
        aws::ContentBlock::ReasoningContent {
            reasoning_content,
            rest,
        } => match reasoning_content {
            aws::ReasoningContentBlock::ReasoningText {
                reasoning_text,
                rest: reasoning_rest,
            } => {
                let mut block_rest = reasoning_text.rest;
                block_rest.extend(reasoning_rest);
                block_rest.extend(rest);
                claude::ResponseContentBlock::Thinking(claude::ThinkingBlock {
                    signature: reasoning_text.signature,
                    thinking: reasoning_text.text,
                    type_: claude::ThinkingBlockType::Thinking,
                    rest: block_rest,
                })
            }
            aws::ReasoningContentBlock::RedactedContent {
                redacted_content,
                rest: reasoning_rest,
            } => {
                let mut block_rest = reasoning_rest;
                block_rest.extend(rest);
                claude::ResponseContentBlock::RedactedThinking(claude::RedactedThinkingBlock {
                    data: redacted_content,
                    type_: claude::RedactedThinkingBlockType::RedactedThinking,
                    rest: block_rest,
                })
            }
            aws::ReasoningContentBlock::Raw(value) => claude::ResponseContentBlock::Raw(value),
        },
        other => {
            claude::ResponseContentBlock::Raw(serde_json::to_value(other).map_err(json_observe)?)
        }
    })
}

fn stop_reason(reason: aws::StopReason) -> claude::StopReason {
    let reason = super::response_usage::enum_string(&reason);
    claude::StopReason::Unknown(
        match reason.as_str() {
            "guardrail_intervened" | "content_filtered" => "refusal",
            "malformed_model_output" | "malformed_tool_use" => "end_turn",
            other => other,
        }
        .into(),
    )
}

fn message_id(body: &[u8]) -> String {
    format!("msg_{}", &hex(Sha256::digest(body))[..24])
}
fn hex(bytes: impl AsRef<[u8]>) -> String {
    bytes
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn json_observe(error: serde_json::Error) -> ChannelError {
    ChannelError::Observe(error.to_string())
}
fn observe(message: &str) -> ChannelError {
    ChannelError::Observe(message.into())
}
