use gproxy_protocol::openai;

use crate::TransformError;
use crate::common::usage;

pub(crate) fn transform(body: bytes::Bytes) -> Result<bytes::Bytes, TransformError> {
    let input: openai::ChatCompletionResponse = serde_json::from_slice(&body)?;
    let choice = input
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| TransformError::shape("Chat response", "choice is missing"))?;
    let id = input.id.clone();
    let mut output = Vec::new();
    if let Some(reasoning) = choice.message.reasoning_content {
        output.push(openai::ResponseItem::Typed(Box::new(
            openai::TypedResponseItem::Reasoning {
                id: None,
                summary: Vec::new(),
                content: Some(vec![openai::ResponseReasoningTextPart {
                    type_: "reasoning_text".into(),
                    text: reasoning,
                    rest: Default::default(),
                }]),
                encrypted_content: None,
                status: Some(openai::ResponseItemLifecycleStatus::Completed),
                rest: Default::default(),
            },
        )));
    }
    if let Some(text) = choice.message.content {
        output.push(openai::ResponseItem::Message(
            openai::ResponseMessageItem::Output(openai::ResponseOutputMessageItem {
                type_: openai::ResponseMessageItemType::Message,
                id: Some(id.clone()),
                role: openai::ResponseOutputMessageRole::Assistant,
                content: vec![openai::ResponseMessageOutputContentPart::OutputText(
                    openai::ResponseOutputText {
                        type_: openai::ResponseOutputTextType::OutputText,
                        annotations: Vec::new(),
                        logprobs: None,
                        text,
                        rest: Default::default(),
                    },
                )],
                status: openai::ResponseItemLifecycleStatus::Completed,
                phase: None,
                rest: choice.message.rest.clone(),
            }),
        ));
    }
    for call in choice.message.tool_calls.into_iter().flatten() {
        output.push(match call {
            openai::ChatToolCall::Function(call) => {
                openai::ResponseItem::Typed(Box::new(openai::TypedResponseItem::FunctionCall {
                    arguments: call.function.arguments,
                    call_id: call.id.clone(),
                    name: call.function.name,
                    id: Some(call.id),
                    caller: None,
                    namespace: None,
                    status: Some(openai::ResponseItemLifecycleStatus::Completed),
                    rest: merge(call.rest, call.function.rest),
                }))
            }
            openai::ChatToolCall::Custom(call) => {
                openai::ResponseItem::Typed(Box::new(openai::TypedResponseItem::CustomToolCall {
                    call_id: call.id.clone(),
                    input: call.custom.input,
                    name: call.custom.name,
                    id: Some(call.id),
                    caller: None,
                    namespace: None,
                    rest: merge(call.rest, call.custom.rest),
                }))
            }
            openai::ChatToolCall::Unknown(raw) => openai::ResponseItem::Unknown(raw),
        });
    }
    if let Some(raw) = choice.message.rest.get("responses_output_items") {
        for item in raw.as_array().into_iter().flatten() {
            output.push(serde_json::from_value(item.clone())?);
        }
    }
    let status = if matches!(choice.finish_reason, openai::ChatFinishReason::Length) {
        openai::ResponseStatus::Incomplete
    } else {
        openai::ResponseStatus::Completed
    };
    let response = openai::ResponseObject {
        id,
        created_at: input.created,
        background: None,
        completed_at: None,
        conversation: None,
        error: None,
        incomplete_details: None,
        instructions: None,
        max_output_tokens: None,
        max_tool_calls: None,
        metadata: None,
        model: Some(input.model),
        moderation: None,
        multi_agent: None,
        object: openai::ResponseObjectType::Response,
        output_text: None,
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
        rest: input.rest,
    };
    Ok(bytes::Bytes::from(serde_json::to_vec(&response)?))
}

fn merge(mut left: openai::Rest, right: openai::Rest) -> openai::Rest {
    left.extend(right);
    left
}
