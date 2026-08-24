mod content;
mod media;
mod response;
mod response_usage;
mod results;
mod tools;

use bytes::Bytes;
use gproxy_channel_api::ChannelError;
use gproxy_protocol::{aws, claude};
use serde_json::{Map, Value};

pub(super) fn request(
    body: &Bytes,
    count_tokens: bool,
    stream: bool,
) -> Result<Bytes, ChannelError> {
    let mut value: Value = serde_json::from_slice(body)
        .map_err(|error| ChannelError::Prepare(format!("Claude request JSON: {error}")))?;
    crate::shared::claude::cache::sanitize(&mut value);
    if count_tokens {
        let input: claude::CountTokensRequestBody =
            serde_json::from_value(value).map_err(json_prepare)?;
        let parts = Parts {
            messages: input.messages,
            system: input.system,
            cache: input.cache_control,
            tools: input.tools,
            tool_choice: input.tool_choice,
            max_tokens: None,
            temperature: None,
            top_p: None,
            stop_sequences: None,
            top_k: None,
            thinking: input.thinking.map(to_value).transpose()?,
            output_config: input.output_config.map(to_value).transpose()?,
            speed: input.speed,
            rest: input.rest,
        };
        return encode(count(parts)?);
    }
    let input: claude::CreateMessageRequestBody =
        serde_json::from_value(value).map_err(json_prepare)?;
    let parts = Parts {
        messages: input.messages,
        system: input.system,
        cache: input.cache_control,
        tools: input.tools,
        tool_choice: input.tool_choice,
        max_tokens: Some(input.max_tokens),
        temperature: input.temperature,
        top_p: input.top_p,
        stop_sequences: input.stop_sequences,
        top_k: input.top_k,
        thinking: input.thinking.map(to_value).transpose()?,
        output_config: input.output_config.map(to_value).transpose()?,
        speed: input.speed,
        rest: input.rest,
    };
    let request = converse(parts)?;
    if stream {
        let value = serde_json::to_value(request).map_err(json_prepare)?;
        let request: aws::ConverseStreamRequest =
            serde_json::from_value(value).map_err(json_prepare)?;
        encode(request)
    } else {
        encode(request)
    }
}

pub(super) fn response(body: &Bytes) -> Result<Bytes, ChannelError> {
    response::convert(body)
}

struct Parts {
    messages: Vec<claude::MessageParam>,
    system: Option<claude::SystemPrompt>,
    cache: Option<claude::CacheControl>,
    tools: Option<Vec<claude::Tool>>,
    tool_choice: Option<claude::ToolChoice>,
    max_tokens: Option<u64>,
    temperature: Option<f64>,
    top_p: Option<f64>,
    stop_sequences: Option<Vec<String>>,
    top_k: Option<i64>,
    thinking: Option<Value>,
    output_config: Option<Value>,
    speed: Option<claude::Speed>,
    rest: Map<String, Value>,
}

fn converse(parts: Parts) -> Result<aws::ConverseRequest, ChannelError> {
    let (messages, system, tool_config, additional, service_tier) = common(&parts)?;
    Ok(aws::ConverseRequest {
        additional_model_request_fields: additional,
        additional_model_response_field_paths: None,
        guardrail_config: None,
        inference_config: Some(aws::InferenceConfiguration {
            max_tokens: parts.max_tokens,
            stop_sequences: parts.stop_sequences,
            temperature: parts.temperature,
            top_p: parts.top_p,
            rest: Default::default(),
        }),
        messages: Some(messages),
        output_config: None,
        performance_config: None,
        prompt_variables: None,
        request_metadata: None,
        service_tier,
        system,
        tool_config,
        rest: Default::default(),
    })
}

fn count(parts: Parts) -> Result<aws::CountTokensRequest, ChannelError> {
    let (messages, system, tool_config, additional, _) = common(&parts)?;
    Ok(aws::CountTokensRequest {
        input: aws::CountTokensInput::Converse {
            converse: aws::ConverseTokensRequest {
                additional_model_request_fields: additional,
                messages: Some(messages),
                system,
                tool_config,
                rest: Default::default(),
            },
            rest: Default::default(),
        },
        rest: Default::default(),
    })
}

type Common = (
    Vec<aws::Message>,
    Option<Vec<aws::SystemContentBlock>>,
    Option<aws::ToolConfiguration>,
    Option<Value>,
    Option<aws::ServiceTier>,
);

fn common(parts: &Parts) -> Result<Common, ChannelError> {
    let mut messages = content::messages(parts.messages.clone())?;
    let mut system = content::system(parts.system.clone())?;
    if let Some(cache) = parts.cache.clone() {
        let cache_point = content::cache_point(cache);
        if let Some(message) = messages.last_mut() {
            message.content.push(aws::ContentBlock::CachePoint {
                cache_point,
                rest: Default::default(),
            });
        } else {
            system
                .get_or_insert_default()
                .push(aws::SystemContentBlock::CachePoint {
                    cache_point,
                    rest: Default::default(),
                });
        }
    }
    let mut additional = parts.rest.clone();
    for (name, value) in [
        ("thinking", parts.thinking.clone()),
        ("output_config", parts.output_config.clone()),
        ("top_k", parts.top_k.map(Value::from)),
    ] {
        if let Some(value) = value {
            additional.insert(name.into(), value);
        }
    }
    let fast = parts.speed.as_ref().is_some_and(|speed| {
        serde_json::to_value(speed)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            == Some("fast".into())
    });
    Ok((
        messages,
        system,
        tools::config(parts.tools.clone(), parts.tool_choice.clone())?,
        (!additional.is_empty()).then_some(Value::Object(additional)),
        fast.then(|| aws::ServiceTier {
            type_: aws::ServiceTierType::Known(aws::ServiceTierTypeKnown::Priority),
            rest: Default::default(),
        }),
    ))
}

fn encode(value: impl serde::Serialize) -> Result<Bytes, ChannelError> {
    serde_json::to_vec(&value)
        .map(Bytes::from)
        .map_err(json_prepare)
}
fn to_value(value: impl serde::Serialize) -> Result<Value, ChannelError> {
    serde_json::to_value(value).map_err(json_prepare)
}
fn json_prepare(error: serde_json::Error) -> ChannelError {
    ChannelError::Prepare(error.to_string())
}
