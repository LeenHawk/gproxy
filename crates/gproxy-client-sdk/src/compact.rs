//! Compacting a portable conversation with the Responses compaction
//! operation (`POST /v1/responses/compact`). The request is rendered as
//! OpenAI Responses whatever its model's dialect: GPROXY accepts compaction
//! only in that shape and converts it for the route's upstream. The
//! replacement history comes back as IR messages to continue from.
use crate::*;
use gproxy_protocol::{
    Operation,
    openai::{
        compact::{
            ClientCompactRequestBody, ClientCompactResponseBody, ClientFunctionCallOutput,
            ClientMessage, ClientResponseItem, ContentItem, FunctionCallOutputBody,
        },
        responses::{input as i, response as r},
    },
};
use http::Method;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

/// The history that replaces the compacted messages, in order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Compaction {
    pub messages: Vec<Message>,
    #[serde(default)]
    pub warnings: Vec<Warning>,
}

impl<B: Backend> Client<B> {
    /// Compact `request.messages` into a shorter replacement history. Tools,
    /// reasoning and OpenAI Responses options are sent as `generate` would
    /// send them; sampling and output limits are not part of compaction.
    pub async fn compact(&self, request: &GenerateRequest) -> Result<Compaction> {
        let rendered = params(request)?;
        let body = self.compact_content(&rendered.request).await?;
        Ok(Compaction {
            messages: messages(body.output)?,
            warnings: rendered.warnings,
        })
    }

    /// Send a native compaction request and return the native replacement
    /// history.
    pub async fn compact_content(
        &self,
        request: &ClientCompactRequestBody,
    ) -> Result<ClientCompactResponseBody> {
        let wire = wire_json(Method::POST, "/v1/responses/compact".into(), request)?;
        self.json(
            OperationKey {
                operation: Operation::CompactContent,
                dialect: protocol::Dialect::OpenAi,
            },
            Some(&request.model),
            wire,
        )
        .await
    }
}

/// The native compaction request for `request`, for callers that send it
/// themselves (see [`Client::compact_content`]).
pub fn params(request: &GenerateRequest) -> Result<Rendered<ClientCompactRequestBody>> {
    // Rendered as the Responses request it is sent as, so warnings describe
    // what that drops; sampling and output limits have no compaction field.
    let mut request = request.clone();
    request.model.dialect = Dialect::Openai;
    request.settings.max_output_tokens = None;
    request.settings.temperature = None;
    request.settings.top_p = None;
    request.settings.stop_sequences.clear();
    let Rendered {
        request: body,
        warnings,
    } = dialects::openai::render(&request, false)?;
    let input = match body.input {
        None => Vec::new(),
        Some(i::Input::Text(text)) => vec![ClientResponseItem::Message(
            ClientMessage::builder(
                "user".into(),
                vec![ContentItem::InputText(
                    gproxy_protocol::openai::compact::ContentItemInputText::builder(text).build(),
                )],
            )
            .build(),
        )],
        Some(i::Input::Items(items)) => items
            .iter()
            .map(|item| {
                history_item(item).map_err(|error| {
                    Error::invalid(format!("history item cannot be compacted: {error}"))
                })
            })
            .collect::<Result<_>>()?,
    };
    let mut compact = ClientCompactRequestBody::builder(
        body.model.unwrap_or_else(|| request.model.id.clone()),
        input,
        body.instructions.flatten().unwrap_or_default(),
        // The Responses default.
        body.parallel_tool_calls.flatten().unwrap_or(true),
    )
    .build();
    compact.tools = body
        .tools
        .map(|tools| serde_json::to_value(tools).map_err(Error::invalid))
        .transpose()?;
    compact.reasoning = body.reasoning.flatten().map(convert).transpose()?;
    compact.text = body.text.map(convert).transpose()?;
    compact.prompt_cache_key = body.prompt_cache_key.flatten();
    compact.service_tier = body
        .service_tier
        .flatten()
        .map(|tier| match serde_json::to_value(tier) {
            Ok(Value::String(tier)) => Ok(tier),
            Ok(_) => Err(Error::invalid("service tier is not a string")),
            Err(error) => Err(Error::invalid(error)),
        })
        .transpose()?;
    Ok(Rendered {
        request: compact,
        warnings,
    })
}

/// A Responses input item as the Codex history item with the same JSON. A
/// message's plain-string content becomes one text part, the only form Codex
/// messages take.
fn history_item(item: &i::InputItem) -> Result<ClientResponseItem> {
    let mut value = serde_json::to_value(item).map_err(Error::invalid)?;
    if let i::InputItem::Easy(message) = item
        && let i::MessageContent::Text(text) = &message.content
    {
        let kind = if message.role == i::MessageRole::Assistant {
            "output_text"
        } else {
            "input_text"
        };
        value["content"] = serde_json::json!([{ "type": kind, "text": text }]);
    }
    serde_json::from_value(value).map_err(Error::invalid)
}

/// Same JSON, another protocol type: Codex compaction items are the history
/// subset of Responses items.
fn convert<T: DeserializeOwned>(value: impl Serialize) -> Result<T> {
    serde_json::to_value(value)
        .and_then(serde_json::from_value)
        .map_err(Error::invalid)
}

/// The replacement history as IR messages. Consecutive items of one role
/// share a message; function outputs are Tool messages.
pub fn messages(output: Vec<ClientResponseItem>) -> Result<Vec<Message>> {
    let mut messages: Vec<Message> = Vec::new();
    for (index, item) in output.into_iter().enumerate() {
        let (role, content) = match item {
            ClientResponseItem::Message(message) => message_blocks(message)?,
            ClientResponseItem::FunctionCallOutput(output) => {
                (Role::Tool, vec![function_output(output)?])
            }
            other => (Role::Assistant, item_blocks(other, index as u32)?),
        };
        match messages.last_mut() {
            Some(last) if last.role == role => last.content.extend(content),
            _ => messages.push(Message { role, content }),
        }
    }
    Ok(messages)
}

fn message_blocks(message: ClientMessage) -> Result<(Role, Vec<Block>)> {
    let role = match message.role.as_str() {
        "user" => Role::User,
        "assistant" => Role::Assistant,
        "system" | "developer" => Role::System,
        other => Role::Unknown(other.into()),
    };
    let content = message
        .content
        .into_iter()
        .map(|part| {
            Ok(match part {
                ContentItem::InputText(part) => Block::text(part.text),
                ContentItem::OutputText(part) => Block::text(part.text),
                ContentItem::InputImage(part) => Block::new(Content::Image(Image {
                    source: Asset::Url {
                        url: part.image_url,
                    },
                    media_type: None,
                    detail: part.detail.map(convert).transpose()?.unwrap_or_default(),
                })),
                other => Block::new(Content::Unknown(
                    serde_json::to_value(other).map_err(Error::invalid)?,
                )),
            })
        })
        .collect::<Result<_>>()?;
    Ok((role, content))
}

fn function_output(output: ClientFunctionCallOutput) -> Result<Block> {
    let value = match output.output {
        FunctionCallOutputBody::Text(text) => ToolOutput::Json(Value::String(text)),
        FunctionCallOutputBody::ContentItems(parts) => ToolOutput::Blocks(
            parts
                .into_iter()
                .map(|part| dialects::openai::parse_output_part(convert(part)?))
                .collect::<Result<_>>()?,
        ),
    };
    Ok(Block::new(Content::ToolResult(ToolResult {
        id: output.call_id.unwrap_or_default(),
        name: output.name.unwrap_or_default(),
        output: value,
        is_error: false,
        provider_executed: false,
    })))
}

/// Items a Responses output also carries parse as one; other history items
/// stay Opaque and are sent back verbatim. Items Responses cannot take as
/// input (Codex client-only kinds) are kept as Unknown and never sent.
fn item_blocks(item: ClientResponseItem, index: u32) -> Result<Vec<Block>> {
    let value = serde_json::to_value(item).map_err(Error::invalid)?;
    if let Ok(item) = serde_json::from_value::<r::ResponseOutputItem>(value.clone()) {
        return dialects::openai::parse_item(item, index);
    }
    Ok(vec![
        match serde_json::from_value::<i::InputItem>(value.clone()) {
            Ok(item) => dialects::with_native(Content::Opaque, Vendor::Openai, item)?,
            Err(_) => Block::new(Content::Unknown(value)),
        },
    ])
}
