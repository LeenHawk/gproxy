//! ChatGPT handoff conduit (calibrated live, June 2026).
//!
//! Thinking / pro / deep-research turns answer via **`stream_handoff`**: the
//! `POST /backend-api/f/conversation` response is only a stub naming a
//! `conversation-turn-{turn_id}` topic; the real turn streams over a per-user
//! conduit WebSocket. This module drives that path:
//!
//! 1. `GET /backend-api/celsius/ws/user` → `{ websocket_url }`.
//! 2. open the WSS (rides the credential's proxy + Edge emulation via
//!    [`UpstreamClient::open_conduit`]); send a `connect` then a `subscribe` to
//!    the turn topic from `offset:"0"`.
//! 3. payload shapes (mined live, June 2026):
//!      - **thinking** turns carry arrays of `conversation-turn-stream` envelopes
//!        whose `encoded_item` is the SAME SSE-v1 delta text the inline path emits;
//!      - **consumer deep research** streams BOTH its live progress (the "CoT")
//!        and its final report as **bare-object**
//!        `conversation-update`/`update-widget-state` frames: `plan.steps[]`
//!        advance pending→in_progress→completed with a `reason` narrative
//!        ([`MsgSynth::push_widget`] → `reasoning_content`), and the finished
//!        report rides in the same frame as `widget_state.report_message` (an
//!        assistant `text` message) → the content channel.
//!      - the **o3 / API deep-research** shape instead batches whole messages via
//!        `update_type: add-messages` (`thoughts` → reasoning, assistant `text`
//!        report → final answer, deduped by id); [`MsgSynth::push_messages`]
//!        handles it.
//!
//! [`fetch_turn_stream`] yields the synthesized SSE incrementally as each frame
//! arrives (vital for multi-minute deep research), feeding the channel's
//! [`super::ChatGptStreamDecoder`].

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use serde_json::Value;

use crate::http::client::UpstreamClient;

mod state;

pub(super) use state::MsgSynth;

/// Give up if no frame arrives for this long after the last one. Deep research
/// can pause minutes between steps (a long search/browse), so this is generous.
const IDLE_TIMEOUT: Duration = Duration::from_secs(180);
/// Hard ceiling on a single turn. Deep research routinely runs many minutes.
const TOTAL_DEADLINE_MS: u64 = 1_800_000;

/// The `connect` handshake frame the browser sends before subscribing.
const CONNECT_FRAME: &str = r#"[{"id":1,"command":{"type":"connect","presence":{"type":"presence","state":"background"}}}]"#;

/// Detect a `stream_handoff` in the `/f/conversation` stub and pull the
/// `turn_exchange_id` (the conduit topic suffix). Returns `None` when the
/// response streamed inline (no handoff) and the caller should use it directly.
pub(super) fn extract_handoff_turn(stub_sse: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(stub_sse).ok()?;
    for line in text.lines() {
        let Some(data) = line.strip_prefix("data:") else {
            continue;
        };
        let data = data.trim_start();
        // cheap pre-filter before parsing
        if !data.contains("stream_handoff") {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<Value>(data)
            && v.get("type").and_then(Value::as_str) == Some("stream_handoff")
            && let Some(turn) = v.get("turn_exchange_id").and_then(Value::as_str)
        {
            return Some(turn.to_string());
        }
    }
    None
}

/// Streaming variant of the conduit reader: connect + subscribe, then return a
/// byte stream that yields synthesized SSE-v1 deltas AS each conduit frame
/// arrives — so the client sees thinking / deep-research output incrementally
/// instead of after the whole (possibly multi-minute) turn. The channel's
/// `stream_decoder` consumes the yielded SSE and emits OpenAI chunks + `[DONE]`.
pub(super) async fn fetch_turn_stream(
    client: Arc<dyn UpstreamClient>,
    secret: Value,
    base: String,
    turn_id: String,
) -> Result<crate::http::client::RespStream, String> {
    use futures_util::StreamExt;

    let ws_url = conduit_url(&client, &secret, &base).await?;
    let mut sock = client
        .open_conduit(&ws_url)
        .await
        .map_err(|e| format!("open conduit: {e}"))?;
    sock.send_text(CONNECT_FRAME.to_string())
        .await
        .map_err(|e| format!("conduit connect: {e}"))?;
    let subscribe = format!(
        r#"[{{"id":2,"command":{{"type":"subscribe","topic_id":"conversation-turn-{turn_id}","offset":"0"}}}}]"#
    );
    sock.send_text(subscribe)
        .await
        .map_err(|e| format!("conduit subscribe: {e}"))?;

    let deadline_ms = crate::util::time::unix_now_ms() + TOTAL_DEADLINE_MS;
    let stream =
        futures_util::stream::unfold(Some((sock, MsgSynth::default())), move |state| async move {
            let (mut sock, mut synth) = state?;
            loop {
                if crate::util::time::unix_now_ms() >= deadline_ms {
                    return None;
                }
                match tokio::time::timeout(IDLE_TIMEOUT, sock.recv_text()).await {
                    Ok(Some(Ok(frame))) => {
                        let mut out = String::new();
                        let done = absorb_frame(&frame, &mut out, &mut synth);
                        if !out.is_empty() {
                            let next = if done { None } else { Some((sock, synth)) };
                            return Some((Ok(Bytes::from(out)), next));
                        }
                        if done {
                            return None;
                        }
                        // No output this frame (heartbeat / noise) — keep reading.
                    }
                    // socket closed, idle timeout, or recv error → end the stream
                    _ => return None,
                }
            }
        });
    Ok(stream.boxed())
}

/// Fetch the per-user conduit `websocket_url`.
async fn conduit_url(
    client: &Arc<dyn UpstreamClient>,
    secret: &Value,
    base: &str,
) -> Result<String, String> {
    let url = format!("{base}/backend-api/celsius/ws/user");
    let mut req = http::Request::get(url)
        .body(Bytes::new())
        .map_err(|e| format!("conduit url request: {e}"))?;
    super::auth::apply_request_headers(&mut req, secret).map_err(|e| e.to_string())?;
    let resp = client
        .send(req)
        .await
        .map_err(|e| format!("conduit url send: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("celsius/ws/user: {}", resp.status()));
    }
    let v: Value =
        serde_json::from_slice(resp.body()).map_err(|e| format!("conduit url parse: {e}"))?;
    v.get("websocket_url")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "celsius/ws/user: missing websocket_url".into())
}

/// Absorb one received frame (a JSON array of pub/sub envelopes). Thinking turns
/// carry `encoded_item` SSE deltas (appended verbatim); deep-research turns carry
/// `update_type: add-messages` whole-message batches (synthesized into SSE by
/// `synth`). Returns `true` when the turn is complete.
fn absorb_frame(frame: &str, sse: &mut String, synth: &mut MsgSynth) -> bool {
    let Ok(value) = serde_json::from_str::<Value>(frame) else {
        return false;
    };
    // Thinking turns send arrays of pub/sub envelopes; consumer deep-research
    // `conversation-update` frames arrive as a single bare object. Accept both.
    let envelopes: Vec<&Value> = match &value {
        Value::Array(a) => a.iter().collect(),
        Value::Object(_) => vec![&value],
        _ => return false,
    };
    let mut done = false;
    for env in envelopes {
        // Explicit turn-complete envelope.
        if env.pointer("/payload/type").and_then(Value::as_str)
            == Some("conversation-turn-complete")
        {
            done = true;
        }
        // Deep research: a `conversation-update` with `add-messages` (the
        // update object sits at `payload` or, when wrapped, `payload.payload`).
        for cu in [env.get("payload"), env.pointer("/payload/payload")]
            .into_iter()
            .flatten()
        {
            match cu.get("update_type").and_then(Value::as_str) {
                Some("add-messages") => {
                    if let Some(msgs) = cu
                        .pointer("/update_content/messages")
                        .and_then(Value::as_array)
                    {
                        done |= synth.push_messages(msgs, sse);
                    }
                }
                // Consumer deep research streams its live PROGRESS (the "CoT")
                // AND its final report as `update-widget-state` plan widgets, not
                // `thoughts`/`add-messages`.
                Some("update-widget-state") => {
                    if let Some(uc) = cu.get("update_content") {
                        done |= synth.push_widget(uc, sse);
                    }
                }
                _ => {}
            }
        }
        // Thinking: subscribe-reply catchups replay the turn so far.
        if let Some(catchups) = env.pointer("/reply/catchups").and_then(Value::as_array) {
            for c in catchups {
                if let Some(item) = c
                    .pointer("/payload/payload/encoded_item")
                    .and_then(Value::as_str)
                {
                    done |= push_item(item, sse);
                }
            }
        }
        // Thinking: live stream item.
        if let Some(item) = env
            .pointer("/payload/payload/encoded_item")
            .and_then(Value::as_str)
        {
            done |= push_item(item, sse);
        }
    }
    done
}

/// Append one `encoded_item` SSE fragment; returns `true` if it marks the end of
/// the turn (`message_stream_complete` typed event, or the SSE `[DONE]`).
fn push_item(item: &str, sse: &mut String) -> bool {
    sse.push_str(item);
    if !item.ends_with('\n') {
        sse.push('\n');
    }
    item.contains("message_stream_complete") || item.contains("[DONE]")
}

#[cfg(test)]
mod tests;
