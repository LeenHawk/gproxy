use gproxy_protocol::{claude, openai};

use crate::TransformError;
use crate::common::{stop, usage};
use crate::models::common::wire_string;

pub(crate) fn transform(body: bytes::Bytes) -> Result<bytes::Bytes, TransformError> {
    let input: claude::CreateMessageResponseBody = serde_json::from_slice(&body)?;
    let output = transform_typed(input)?;
    Ok(bytes::Bytes::from(serde_json::to_vec(&output)?))
}

pub(crate) fn transform_typed(
    input: claude::CreateMessageResponseBody,
) -> Result<openai::ChatCompletionResponse, TransformError> {
    let service_tier = claude_service_tier(&input.usage)?;
    let refusal = stop::refusal_text(&input.stop_reason, input.stop_details.as_ref());
    let mut rendered = Vec::new();
    let mut calls = Vec::new();
    for block in input.content {
        match block {
            claude::ResponseContentBlock::Text(block) => rendered.push(block.text),
            claude::ResponseContentBlock::Thinking(block) => rendered.push(block.thinking),
            claude::ResponseContentBlock::RedactedThinking(_) => {}
            claude::ResponseContentBlock::ToolUse(block) => calls.push(
                openai::ChatToolCall::Function(crate::wire!(openai::ChatFunctionToolCall {
                    id: block.id,
                    type_: openai::FunctionToolChoiceType::Function,
                    function: openai::FunctionCall {
                        arguments: serde_json::to_string(&block.input)?,
                        name: block.name,
                        rest: Default::default(),
                    },
                    rest: Default::default(),
                })),
            ),
            claude::ResponseContentBlock::ServerToolUse(block) => {
                calls.push(custom_call(
                    block.id,
                    wire_string(&block.name)?,
                    block.input,
                ));
            }
            claude::ResponseContentBlock::McpToolUse(block) => {
                calls.push(custom_call(
                    block.id,
                    format!("mcp:{}:{}", block.server_name, block.name),
                    block.input,
                ));
            }
            claude::ResponseContentBlock::Raw(_) => {}
            _ => {}
        }
    }
    let output = crate::wire!(openai::ChatCompletionResponse {
        id: input.id,
        choices: vec![crate::wire!(openai::ChatCompletionChoice {
            finish_reason: stop::claude_to_chat(&input.stop_reason),
            index: 0,
            logprobs: None,
            message: openai::ChatMessage {
                role: openai::ChatCompletionMessageRole::Assistant,
                content: (!rendered.is_empty()).then(|| rendered.join("\n")),
                refusal,
                annotations: None,
                audio: None,
                function_call: None,
                reasoning_content: None,
                tool_calls: (!calls.is_empty()).then_some(calls),
                rest: Default::default(),
            },
            rest: Default::default(),
        })],
        created: Some(0),
        model: wire_string(&input.model)?.into(),
        object: openai::ChatCompletionObjectType::ChatCompletion,
        moderation: None,
        service_tier,
        system_fingerprint: None,
        usage: usage::claude_to_chat(input.usage),
        rest: Default::default(),
    });
    Ok(output)
}

fn custom_call(id: String, name: String, input: claude::JsonObject) -> openai::ChatToolCall {
    openai::ChatToolCall::Custom(crate::wire!(openai::ChatCustomToolCall {
        id,
        type_: openai::CustomToolChoiceType::Custom,
        custom: openai::CustomToolCall {
            input: serde_json::to_string(&input).unwrap_or_else(|_| "{}".into()),
            name,
            rest: Default::default(),
        },
        rest: Default::default(),
    }))
}

fn claude_service_tier(
    usage: &claude::Usage,
) -> Result<Option<openai::ServiceTier>, TransformError> {
    if matches!(
        usage.speed,
        Some(claude::Speed::Known(claude::SpeedKnown::Fast))
    ) {
        return Ok(Some(openai::ServiceTier::Priority));
    }
    let Some(tier) = usage.service_tier.as_ref() else {
        return Ok(None);
    };
    Ok(Some(if wire_string(tier)? == "priority" {
        openai::ServiceTier::Priority
    } else {
        openai::ServiceTier::Default
    }))
}
