//! Stable CLI conversation metadata. Counters live in host-owned, credential-
//! scoped state rather than a process-global cache inside the channel.
use super::{proto::Message, request::Auth};
use crate::channel::{ChannelError, ChannelState};
use gproxy_protocol::{
    capability::{CapabilityErrorKind, CasResult, StateWrite},
    connection::Bytes,
};
use http::HeaderMap;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::time::{Duration, SystemTime};

pub(super) struct Session {
    id: String,
    cascade: String,
    turn: u64,
}

fn uuid(seed: &str) -> String {
    let mut bytes = Sha256::digest(seed.as_bytes());
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    )
}
fn text(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}
impl Session {
    pub async fn resolve(
        body: &Value,
        headers: &HeaderMap,
        auth: &Auth<'_>,
        state: &dyn ChannelState,
    ) -> Result<Self, ChannelError> {
        let metadata = body
            .pointer("/metadata/user_id")
            .or_else(|| body.get("user"))
            .and_then(Value::as_str)
            .and_then(|value| serde_json::from_str::<Value>(value).ok());
        let key = [
            "x-claude-code-session-id",
            "session-id",
            "session_id",
            "x-session-id",
            "thread-id",
        ]
        .into_iter()
        .find_map(|name| {
            headers
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::trim)
                .filter(|v| !v.is_empty())
        })
        .or_else(|| text(body.get("session_id")))
        .or_else(|| text(body.pointer("/metadata/session_id")))
        .or_else(|| text(metadata.as_ref().and_then(|v| v.get("session_id"))))
        .or_else(|| text(body.get("conversation_id")))
        .or_else(|| text(body.get("prompt_cache_key")));
        let Some(key) = key else {
            let id = super::request::uuid()?;
            return Ok(Self {
                cascade: id.clone(),
                id,
                turn: 0,
            });
        };
        let id = uuid(&format!("devin-session:{}:{key}", auth.fingerprint));
        let cascade = text(body.get("cascade_id"))
            .map(|key| uuid(&format!("devin-cascade:{}:{key}", auth.fingerprint)))
            .unwrap_or_else(|| id.clone());
        let state_key = format!("devin:turn:{id}");
        loop {
            let current = state.get(&state_key).await?;
            let turn = match &current {
                Some(entry) => serde_json::from_slice::<u64>(&entry.payload)
                    .map_err(|_| super::bad_request("invalid stored Devin turn index"))?,
                None => 0,
            };
            let next = turn
                .checked_add(1)
                .ok_or_else(|| super::bad_request("Devin turn index exhausted"))?;
            let write = StateWrite {
                payload: Bytes::from(next.to_string()),
                expires_at: Some(
                    SystemTime::UNIX_EPOCH + Duration::from_secs(super::now_secs() + 24 * 60 * 60),
                ),
            };
            match state
                .compare_exchange(&state_key, current.map(|entry| entry.version), Some(write))
                .await
            {
                Ok(CasResult::Applied(_)) => return Ok(Self { id, cascade, turn }),
                Ok(CasResult::Conflict) => continue,
                Err(error) if error.kind() == CapabilityErrorKind::Unsupported => {
                    // Stateless hosts can still replay full histories consistently.
                    let turn = body
                        .get("messages")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter(|m| m.get("role").and_then(Value::as_str) == Some("assistant"))
                        .count() as u64;
                    return Ok(Self { id, cascade, turn });
                }
                Err(error) => return Err(error.into()),
            }
        }
    }
    pub fn write(&self, body: &mut Vec<u8>, request: &Value) {
        let mut metadata = Message::new();
        metadata.string(1, &self.id).varint(3, 4);
        if self.turn > 0 {
            metadata.varint(2, self.turn);
        }
        let messages = request.get("messages").and_then(Value::as_array);
        if let Some(messages) = messages {
            let is_user = |m: &Value| m.get("role").and_then(Value::as_str) == Some("user");
            if messages.last().is_some_and(is_user)
                && (self.turn == 0 || messages.len() < 2 || !is_user(&messages[messages.len() - 2]))
            {
                metadata.varint(4, 14);
            }
        }
        let mut fields = Message::new();
        fields.message(15, &metadata).string(16, &self.cascade);
        body.extend_from_slice(fields.as_bytes());
    }
}
