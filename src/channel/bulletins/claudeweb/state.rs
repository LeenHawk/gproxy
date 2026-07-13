//! In-memory continuation state for Claude Web client tools.
//!
//! Claude keeps the original `/completion` response open while it waits for a
//! POST to `/tool_result`. The downstream Anthropic API, however, ends the
//! first response at `stop_reason=tool_use` and sends the result in a second
//! request. We park the unread upstream stream by `tool_use_id`, then resume it
//! for that second request.

use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex, OnceLock};

use bytes::Bytes;
use futures_util::StreamExt;
use serde_json::Value;

use crate::http::client::{RespStream, UpstreamClient};
use crate::transform::common::SseDecoder;

const PENDING_TTL_MS: u64 = 10 * 60 * 1000;

pub(super) struct Pending {
    pub stream: RespStream,
    pub client: Arc<dyn UpstreamClient>,
    pub base: String,
    pub organization: String,
    pub conversation: String,
    pub session_key: String,
    pub device_id: Option<String>,
    pub model: String,
    pub message_id: String,
    pub input_tokens: u64,
    pub output_tokens: Arc<AtomicU64>,
    created_at_ms: u64,
}

pub(super) struct StreamMeta {
    pub client: Arc<dyn UpstreamClient>,
    pub base: String,
    pub organization: String,
    pub conversation: String,
    pub session_key: String,
    pub device_id: Option<String>,
    pub model: String,
    pub message_id: String,
    pub input_tokens: u64,
    pub output_tokens: Arc<AtomicU64>,
}

struct Active {
    stream: RespStream,
    meta: StreamMeta,
    detector: ToolDetector,
}

struct ToolDetector {
    decoder: SseDecoder,
    current_tool_use_id: Option<String>,
    message_id: Option<String>,
}

impl Default for ToolDetector {
    fn default() -> Self {
        Self {
            decoder: SseDecoder::new(),
            current_tool_use_id: None,
            message_id: None,
        }
    }
}

impl ToolDetector {
    fn push(&mut self, bytes: &[u8]) -> Option<String> {
        for frame in self.decoder.push(bytes) {
            let Ok(value) = serde_json::from_str::<Value>(&frame.data) else {
                continue;
            };
            match value.get("type").and_then(Value::as_str) {
                Some("message_start") => {
                    self.message_id = value
                        .get("message")
                        .and_then(|message| message.get("id"))
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                }
                Some("content_block_start")
                    if value
                        .get("content_block")
                        .and_then(|block| block.get("type"))
                        .and_then(Value::as_str)
                        == Some("tool_use") =>
                {
                    self.current_tool_use_id = value
                        .get("content_block")
                        .and_then(|block| block.get("id"))
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                }
                Some("content_block_stop") if self.current_tool_use_id.is_some() => {
                    return self.current_tool_use_id.take();
                }
                _ => {}
            }
        }
        None
    }
}

fn pending() -> &'static Mutex<HashMap<String, Pending>> {
    static PENDING: OnceLock<Mutex<HashMap<String, Pending>>> = OnceLock::new();
    PENDING.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(super) fn take(tool_use_id: &str) -> Option<Pending> {
    let now = crate::util::time::unix_now_ms();
    let (found, expired) = {
        let mut map = pending().lock().expect("claudeweb pending lock poisoned");
        let expired_ids = map
            .iter()
            .filter(|(_, pending)| now.saturating_sub(pending.created_at_ms) >= PENDING_TTL_MS)
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        let expired = expired_ids
            .into_iter()
            .filter_map(|id| map.remove(&id))
            .collect::<Vec<_>>();
        (map.remove(tool_use_id), expired)
    };
    for pending in expired {
        cleanup(pending.into_meta());
    }
    found
}

pub(super) fn discard(pending: Pending) {
    cleanup(pending.into_meta());
}

fn insert(tool_use_id: String, pending_state: Pending) {
    let replaced = pending()
        .lock()
        .expect("claudeweb pending lock poisoned")
        .insert(tool_use_id, pending_state);
    if let Some(replaced) = replaced {
        cleanup(replaced.into_meta());
    }
}

impl Pending {
    pub(super) fn into_meta(self) -> StreamMeta {
        StreamMeta {
            client: self.client,
            base: self.base,
            organization: self.organization,
            conversation: self.conversation,
            session_key: self.session_key,
            device_id: self.device_id,
            model: self.model,
            message_id: self.message_id,
            input_tokens: self.input_tokens,
            output_tokens: self.output_tokens,
        }
    }
}

/// End the downstream stream at the first client-tool boundary while retaining
/// ownership of the unread upstream response.
pub(super) fn pause_on_tool_use(stream: RespStream, meta: StreamMeta) -> RespStream {
    futures_util::stream::unfold(
        Some(Active {
            stream,
            meta,
            detector: ToolDetector::default(),
        }),
        |state| async move {
            let mut active = state?;
            match active.stream.next().await {
                Some(Ok(bytes)) => {
                    if let Some(tool_use_id) = active.detector.push(&bytes) {
                        if let Some(message_id) = active.detector.message_id.take() {
                            active.meta.message_id = message_id;
                        }
                        let Active { stream, meta, .. } = active;
                        insert(
                            tool_use_id,
                            Pending {
                                stream,
                                client: meta.client,
                                base: meta.base,
                                organization: meta.organization,
                                conversation: meta.conversation,
                                session_key: meta.session_key,
                                device_id: meta.device_id,
                                model: meta.model,
                                message_id: meta.message_id,
                                input_tokens: meta.input_tokens,
                                output_tokens: meta.output_tokens,
                                created_at_ms: crate::util::time::unix_now_ms(),
                            },
                        );
                        Some((Ok(bytes), None))
                    } else {
                        Some((Ok(bytes), Some(active)))
                    }
                }
                Some(Err(error)) => {
                    cleanup(active.meta);
                    Some((Err(error), None))
                }
                None => {
                    cleanup(active.meta);
                    None
                }
            }
        },
    )
    .boxed()
}

fn cleanup(meta: StreamMeta) {
    tokio::spawn(async move {
        let url = format!(
            "{}/api/organizations/{}/chat_conversations/{}",
            meta.base, meta.organization, meta.conversation
        );
        let Ok(mut request) = http::Request::delete(url).body(Bytes::new()) else {
            return;
        };
        if super::auth::apply_browser_headers(
            &mut request,
            &meta.session_key,
            &meta.base,
            &format!("{}/chat/{}", meta.base, meta.conversation),
        )
        .and_then(|()| super::auth::apply_device_header(&mut request, meta.device_id.as_deref()))
        .is_ok()
        {
            let _ = meta.client.send(request).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detector_finds_tool_use_boundary_and_message_id() {
        let mut detector = ToolDetector::default();
        let bytes = b"data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\"}}\n\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_1\",\"name\":\"weather\"}}\n\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n";
        assert_eq!(detector.push(bytes).as_deref(), Some("toolu_1"));
        assert_eq!(detector.message_id.as_deref(), Some("msg_1"));
    }
}
