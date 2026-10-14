//! Devin signs reasoning with both opaque bytes and a provider/type tag.
//! Carry both through the client's reasoning field, never through server state.
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::channel::ChannelError;

pub(super) const PREFIX: &str = "devin:";

#[derive(Default, Deserialize, Serialize)]
pub(super) struct Reasoning {
    #[serde(skip)]
    pub text: String,
    pub signature: Vec<u8>,
    pub signature_type: String,
}

impl Reasoning {
    pub fn carrier(&self) -> String {
        format!(
            "{PREFIX}{}",
            STANDARD.encode(serde_json::to_vec(self).expect("reasoning fields serialize"))
        )
    }
    pub fn details(&self, id: &str) -> Result<Vec<Value>, ChannelError> {
        let format = match self.signature_type.as_str() {
            "anthropic" => Some("anthropic-claude-v1"),
            "gemini" => Some("google-gemini-v1"),
            "openai" => Some("openai-responses-v1"),
            _ => None,
        };
        if let Some(format) = format {
            // These providers' native signatures are opaque strings. Do not
            // re-encode them or put them inside a Devin-specific envelope.
            let signature = std::str::from_utf8(&self.signature).map_err(|_| {
                ChannelError::InvalidResponse("non-UTF8 native reasoning signature".into())
            })?;
            if format == "openai-responses-v1" {
                let mut details = Vec::new();
                if !self.text.is_empty() {
                    details.push(json!({"type":"reasoning.text","format":format,"id":id,"index":0,"text":self.text}));
                }
                details.push(json!({"type":"reasoning.encrypted","format":format,"id":id,"index":details.len(),"data":signature}));
                return Ok(details);
            }
            return Ok(vec![
                json!({"type":"reasoning.text","format":format,"id":id,"index":0,"text":self.text,"signature":signature}),
            ]);
        }
        // Devin's own state uses the standard encrypted-reasoning transport.
        // The payload carries its type/bytes, interpreted only by this channel.
        let mut details = Vec::new();
        if !self.text.is_empty() {
            details.push(json!({"type":"reasoning.text", "format":"openai-responses-v1", "id":id, "index":0, "text":self.text}));
        }
        details.push(json!({"type":"reasoning.encrypted", "format":"openai-responses-v1", "id":id, "index":details.len(), "data":self.carrier()}));
        Ok(details)
    }

    pub fn from_message(message: &Value) -> Result<Self, ChannelError> {
        let mut out = Self {
            text: message
                .get("reasoning_content")
                .or_else(|| message.get("reasoning"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            ..Self::default()
        };
        let mut detail_text = String::new();
        if let Some(details) = message.get("reasoning_details").and_then(Value::as_array) {
            for detail in details {
                if let Some(text) = detail
                    .get("text")
                    .or_else(|| detail.get("summary"))
                    .and_then(Value::as_str)
                {
                    detail_text.push_str(text);
                }
                let signature = detail
                    .get("signature")
                    .or_else(|| {
                        (detail.get("type").and_then(Value::as_str) == Some("reasoning.encrypted"))
                            .then(|| detail.get("data"))
                            .flatten()
                    })
                    .and_then(Value::as_str);
                let Some(signature) = signature else { continue };
                if let Some(encoded) = signature.strip_prefix(PREFIX) {
                    let bytes = STANDARD
                        .decode(encoded)
                        .map_err(|_| super::bad_request("invalid Devin reasoning signature"))?;
                    let decoded: Self = serde_json::from_slice(&bytes)
                        .map_err(|_| super::bad_request("invalid Devin reasoning signature"))?;
                    if !out.signature.is_empty()
                        && (out.signature != decoded.signature
                            || out.signature_type != decoded.signature_type)
                    {
                        return Err(super::bad_request(
                            "conflicting Devin reasoning signatures in one assistant turn",
                        ));
                    }
                    out.signature = decoded.signature;
                    out.signature_type = decoded.signature_type;
                } else if !signature.is_empty() {
                    let kind = if signature.starts_with("sealed.v1.") {
                        "sealed"
                    } else {
                        match detail.get("format").and_then(Value::as_str) {
                            Some("anthropic-claude-v1") => "anthropic",
                            Some("google-gemini-v1") => "gemini",
                            Some("openai-responses-v1") => "openai",
                            _ => continue,
                        }
                    };
                    if !out.signature.is_empty()
                        && (out.signature != signature.as_bytes() || out.signature_type != kind)
                    {
                        return Err(super::bad_request(
                            "multiple signed reasoning blocks cannot share one Devin assistant turn",
                        ));
                    }
                    out.signature = signature.as_bytes().to_vec();
                    out.signature_type = kind.to_owned();
                }
            }
        }
        if !detail_text.is_empty() {
            out.text = detail_text;
        }
        Ok(out)
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty() && self.signature.is_empty()
    }
}
