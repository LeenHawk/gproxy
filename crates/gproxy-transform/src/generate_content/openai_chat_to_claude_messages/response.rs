use gproxy_protocol::{claude, openai};

use crate::TransformError;
use crate::common::{stop, usage};
use crate::models::common::wire_string;

pub(crate) fn transform(body: bytes::Bytes) -> Result<bytes::Bytes, TransformError> {
    let input: claude::CreateMessageResponseBody = serde_json::from_slice(&body)?;
    let mut rest = input.rest;
    let created = rest
        .remove("openai_created")
        .map(serde_json::from_value)
        .transpose()?;
    let mut text = Vec::new();
    let mut reasoning = Vec::new();
    let mut calls = Vec::new();
    let mut raw = Vec::new();
    for block in input.content {
        match block {
            claude::ResponseContentBlock::Text(block) => text.push(block.text),
            claude::ResponseContentBlock::Thinking(block) => reasoning.push(block.thinking),
            claude::ResponseContentBlock::RedactedThinking(block) => {
                raw.push(serde_json::to_value(block)?);
            }
            claude::ResponseContentBlock::ToolUse(block) => calls.push(
                openai::ChatToolCall::Function(openai::ChatFunctionToolCall {
                    id: block.id,
                    type_: openai::FunctionToolChoiceType::Function,
                    function: openai::FunctionCall {
                        arguments: serde_json::to_string(&block.input)?,
                        name: block.name,
                        rest: block.rest,
                    },
                    rest: Default::default(),
                }),
            ),
            claude::ResponseContentBlock::Raw(value) => raw.push(value),
            other => raw.push(serde_json::to_value(other)?),
        }
    }
    let mut message_rest: openai::Rest = Default::default();
    if !raw.is_empty() {
        message_rest.insert(
            "claude_content_blocks".into(),
            serde_json::Value::Array(raw),
        );
    }
    let output = openai::ChatCompletionResponse {
        id: input.id,
        choices: vec![openai::ChatCompletionChoice {
            finish_reason: stop::claude_to_chat(&input.stop_reason),
            index: 0,
            logprobs: None,
            message: openai::ChatMessage {
                role: openai::ChatCompletionMessageRole::Assistant,
                content: (!text.is_empty()).then(|| text.join("")),
                refusal: None,
                annotations: None,
                audio: None,
                function_call: None,
                reasoning_content: (!reasoning.is_empty()).then(|| reasoning.join("")),
                tool_calls: (!calls.is_empty()).then_some(calls),
                rest: message_rest,
            },
            rest: Default::default(),
        }],
        created,
        model: wire_string(&input.model)?.into(),
        object: openai::ChatCompletionObjectType::ChatCompletion,
        moderation: None,
        service_tier: None,
        system_fingerprint: None,
        usage: usage::claude_to_chat(input.usage),
        rest,
    };
    Ok(bytes::Bytes::from(serde_json::to_vec(&output)?))
}
