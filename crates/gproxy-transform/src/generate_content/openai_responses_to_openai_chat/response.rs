use gproxy_protocol::openai;

use crate::TransformError;
use crate::common::usage;

pub(crate) fn transform(body: bytes::Bytes) -> Result<bytes::Bytes, TransformError> {
    let input: openai::ChatCompletionResponse = serde_json::from_slice(&body)?;
    let output = transform_typed(input)?;
    Ok(bytes::Bytes::from(serde_json::to_vec(&output)?))
}

pub(crate) fn transform_typed(
    input: openai::ChatCompletionResponse,
) -> Result<openai::ResponseObject, TransformError> {
    let id = input.id.clone();
    let mut output = Vec::new();
    let mut output_text = None;
    let mut status = openai::ResponseStatus::Completed;
    let mut incomplete_details = None;
    let choice = input.choices.into_iter().next();
    if let Some(reasoning) = choice
        .as_ref()
        .and_then(|choice| choice.message.reasoning_content.clone())
    {
        output.push(openai::ResponseItem::Typed(Box::new(
            openai::TypedResponseItem::Reasoning {
                id: None,
                summary: Vec::new(),
                content: Some(vec![crate::wire!(openai::ResponseReasoningTextPart {
                    type_: openai::ResponseReasoningTextType::ReasoningText,
                    text: reasoning,
                    rest: Default::default(),
                })]),
                encrypted_content: None,
                status: Some(openai::ResponseItemLifecycleStatus::Completed),
                rest: Default::default(),
            },
        )));
    }
    if let Some(text) = choice
        .as_ref()
        .and_then(|choice| choice.message.content.clone())
    {
        output_text = (!text.is_empty()).then(|| text.clone());
        output.push(openai::ResponseItem::Message(
            openai::ResponseMessageItem::Output(crate::wire!(openai::ResponseOutputMessageItem {
                type_: openai::ResponseMessageItemType::Message,
                id: format!("msg_{}", choice.as_ref().expect("choice exists").index),
                role: openai::ResponseOutputMessageRole::Assistant,
                content: vec![openai::ResponseMessageOutputContentPart::OutputText(
                    crate::wire!(openai::ResponseOutputText {
                        type_: openai::ResponseOutputTextType::OutputText,
                        annotations: Vec::new(),
                        logprobs: None,
                        text,
                        rest: Default::default(),
                    }),
                )],
                status: openai::ResponseItemLifecycleStatus::Completed,
                phase: None,
                rest: Default::default(),
            })),
        ));
    }
    for call in choice
        .as_ref()
        .and_then(|choice| choice.message.tool_calls.clone())
        .into_iter()
        .flatten()
    {
        output.push(match call {
            openai::ChatToolCall::Function(call) => {
                let call_id = response_call_id(&call.id);
                openai::ResponseItem::Typed(Box::new(openai::TypedResponseItem::FunctionCall {
                    arguments: call.function.arguments,
                    call_id,
                    name: call.function.name,
                    id: Some(response_item_id(&call.id)),
                    caller: None,
                    namespace: None,
                    async_: None,
                    status: Some(openai::ResponseItemLifecycleStatus::Completed),
                    rest: Default::default(),
                }))
            }
            openai::ChatToolCall::Custom(call) => {
                let call_id = response_call_id(&call.id);
                openai::ResponseItem::Typed(Box::new(openai::TypedResponseItem::CustomToolCall {
                    call_id,
                    input: call.custom.input,
                    name: call.custom.name,
                    id: None,
                    caller: None,
                    namespace: None,
                    async_: None,
                    rest: Default::default(),
                }))
            }
            openai::ChatToolCall::Unknown(_) => continue,
            #[cfg(not(feature = "exhaustive"))]
            _ => {
                return Err(crate::TransformError::unsupported(
                    "protocol enum",
                    "unrecognized external variant",
                ));
            }
        });
    }
    if let Some(reason) = choice.as_ref().map(|choice| &choice.finish_reason)
        && matches!(
            reason,
            openai::ChatFinishReason::Length | openai::ChatFinishReason::ContentFilter
        )
    {
        status = openai::ResponseStatus::Incomplete;
        incomplete_details = Some(crate::wire!(openai::IncompleteDetails {
            reason: Some(
                if matches!(reason, openai::ChatFinishReason::ContentFilter) {
                    openai::IncompleteReason::ContentFilter
                } else {
                    openai::IncompleteReason::MaxOutputTokens
                },
            ),
            rest: Default::default(),
        }));
    }
    let response = crate::wire!(openai::ResponseObject {
        id,
        created_at: input.created,
        background: None,
        completed_at: input.created,
        conversation: None,
        error: None,
        incomplete_details,
        instructions: None,
        max_output_tokens: None,
        max_tool_calls: None,
        metadata: None,
        model: Some(input.model),
        moderation: None,
        multi_agent: None,
        object: openai::ResponseObjectType::Response,
        output_text,
        output,
        parallel_tool_calls: None,
        prompt: None,
        prompt_cache_key: None,
        prompt_cache_options: None,
        prompt_cache_retention: None,
        previous_response_id: None,
        reasoning: None,
        safety_identifier: None,
        service_tier: input.service_tier,
        status: Some(status),
        store: None,
        temperature: None,
        text: None,
        tool_choice: None,
        tools: None,
        top_logprobs: None,
        top_p: None,
        truncation: None,
        usage: input.usage.map(usage::chat_to_responses),
        user: None,
        rest: Default::default(),
    });
    Ok(response)
}

fn response_call_id(original: &str) -> String {
    prefixed_id(original, "call_")
}

fn response_item_id(original: &str) -> String {
    prefixed_id(original, "fc_")
}

fn prefixed_id(original: &str, prefix: &str) -> String {
    if original.starts_with(prefix.trim_end_matches('_')) {
        return original.to_owned();
    }
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in original.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{prefix}{hash:016x}")
}
