//! Devin's Claude surface. Vendor signatures and budget interpretation stay
//! here; shared transforms receive only ordinary, declared protocol fields.
use super::reasoning::Reasoning;
use crate::channel::ChannelError;
use gproxy_protocol::{
    connection::Bytes,
    transform::{
        generate::{claude_chat, signature},
        identity::{IdNamespace, IdentityFlow},
    },
    wire::claude::stream::StreamEvent,
};
use serde_json::{Value, json};

fn invalid(error: impl std::fmt::Display) -> ChannelError {
    ChannelError::InvalidResponse(error.to_string())
}

pub(super) fn request(mut value: Value) -> Result<Value, ChannelError> {
    let model = value
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let thinking = value.get("thinking").cloned();
    // A Gemini signature carried by the existing cross-protocol adapter must
    // reach Devin as Gemini, not be submitted as an Anthropic signature.
    for message in value
        .get_mut("messages")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        for block in message
            .get_mut("content")
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
        {
            if block.get("type").and_then(Value::as_str) != Some("thinking") {
                continue;
            }
            let Some(carried) = block
                .get("signature")
                .and_then(Value::as_str)
                .and_then(signature::Carried::parse)
            else {
                continue;
            };
            let (kind, signature) = match carried {
                signature::Carried::GeminiThought(value) => ("gemini", value),
                signature::Carried::Claude(value) => ("anthropic", value),
                signature::Carried::GeminiNext(_) => continue,
            };
            block["signature"] = json!(
                Reasoning {
                    text: String::new(),
                    signature: signature.as_bytes().to_vec(),
                    signature_type: kind.into(),
                }
                .carrier()
            );
        }
    }
    let input = serde_json::from_value(value)
        .map_err(|error| super::bad_request(format!("Devin Claude request: {error}")))?;
    let converted = claude_chat::claude_to_openai(&input, model)
        .map_err(|error| super::bad_request(error.to_string()))?;
    let mut body = serde_json::to_value(converted.value).map_err(invalid)?;
    // Exact numeric budgets have no Chat field. Only this channel maps them to
    // its selector effort; other channels' conversion behavior stays unchanged.
    if let Some(thinking) = thinking {
        body["thinking"] = thinking;
    }
    Ok(body)
}

fn project(message: &mut Value) -> Result<(), ChannelError> {
    if message.get("reasoning_details").is_none() {
        return Ok(());
    }
    let reasoning = Reasoning::from_message(message)?;
    if reasoning.signature.is_empty() {
        return Ok(());
    }
    let signature = if reasoning.signature_type == "anthropic" {
        std::str::from_utf8(&reasoning.signature)
            .map_err(invalid)?
            .to_owned()
    } else {
        reasoning.carrier()
    };
    // This is the representation of a Claude thinking block. The generic
    // converter treats its signature as opaque; only Devin interprets it.
    message["reasoning_details"] = json!([{
        "type":"reasoning.text", "format":"anthropic-claude-v1", "index":0,
        "text":reasoning.text, "signature":signature,
    }]);
    Ok(())
}

pub(super) fn response(mut value: Value) -> Result<Value, ChannelError> {
    let usage = extra_usage(&value);
    if let Some(message) = value.pointer_mut("/choices/0/message") {
        project(message)?;
    }
    let model = value
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let input = serde_json::from_value(value).map_err(invalid)?;
    let converted = claude_chat::openai_response_to_claude(&input, model, &Default::default())
        .map_err(invalid)?;
    let mut value = serde_json::to_value(converted.value).map_err(invalid)?;
    if let Some(target) = value.get_mut("usage").and_then(Value::as_object_mut) {
        target.extend(usage);
    }
    Ok(value)
}

fn extra_usage(value: &Value) -> serde_json::Map<String, Value> {
    [
        super::stream::ACTUAL_MODEL_KEY,
        "cache_creation_input_tokens",
    ]
    .into_iter()
    .filter_map(|key| {
        value
            .get("usage")?
            .get(key)
            .map(|value| (key.to_owned(), value.clone()))
    })
    .collect()
}

pub(super) struct Stream {
    inner: claude_chat::stream::ChatToClaudeStream,
    usage: serde_json::Map<String, Value>,
}
impl Stream {
    pub fn new() -> Result<Self, ChannelError> {
        let mut namespace = [0; 16];
        getrandom::fill(&mut namespace).map_err(invalid)?;
        Ok(Self {
            inner: claude_chat::stream::ChatToClaudeStream::new(
                claude_chat::stream::ChatToClaudeContext { start_usage: None },
                IdentityFlow::new(IdNamespace::with_bytes(namespace)),
                Default::default(),
            )
            .map_err(invalid)?,
            usage: Default::default(),
        })
    }
    pub fn push(&mut self, chunks: Vec<Value>) -> Result<Bytes, ChannelError> {
        let mut events = Vec::new();
        for mut chunk in chunks {
            self.usage.extend(extra_usage(&chunk));
            if let Some(delta) = chunk.pointer_mut("/choices/0/delta") {
                project(delta)?;
            }
            events.extend(
                self.inner
                    .push(serde_json::from_value(chunk).map_err(invalid)?)
                    .map_err(invalid)?
                    .value,
            );
        }
        encode(events, &self.usage)
    }
    pub fn finish(mut self) -> Result<Bytes, ChannelError> {
        self.inner.push_done().map_err(invalid)?;
        encode(self.inner.finish().map_err(invalid)?.chunks, &self.usage)
    }
}
fn encode(
    events: Vec<StreamEvent>,
    usage: &serde_json::Map<String, Value>,
) -> Result<Bytes, ChannelError> {
    let mut out = String::new();
    for event in events {
        let mut value = serde_json::to_value(event).map_err(invalid)?;
        if let Some(target) = value.get_mut("usage").and_then(Value::as_object_mut) {
            target.extend(usage.clone());
        }
        out.push_str("event: ");
        out.push_str(value["type"].as_str().unwrap_or_default());
        out.push_str("\ndata: ");
        out.push_str(&value.to_string());
        out.push_str("\n\n");
    }
    Ok(Bytes::from(out))
}
