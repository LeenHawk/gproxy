use std::collections::VecDeque;

use bytes::Bytes;
use gproxy_channel_api::{Channel, TransportError, UsageCtx, WsFrame};

pub(super) fn response_usage(
    channel: &dyn Channel,
    facts: &crate::funnel::FunnelCtx,
    body: &[u8],
) -> Option<gproxy_channel_api::NormalizedUsage> {
    channel.extract_usage(UsageCtx {
        key: facts.key?,
        request_body: &facts.request_body,
        response_headers: &http::HeaderMap::new(),
        response_body: body,
    })
}

pub(super) fn wire_string(value: &impl serde::Serialize) -> Option<String> {
    serde_json::to_value(value)
        .ok()?
        .as_str()
        .map(str::to_owned)
}

pub(super) fn request_text(body: &Bytes) -> Result<String, TransportError> {
    String::from_utf8(body.to_vec())
        .map_err(|error| TransportError::Interrupted(format!("websocket request UTF-8: {error}")))
}

pub(super) fn transport(error: impl std::fmt::Display) -> TransportError {
    TransportError::Interrupted(error.to_string())
}

pub(super) fn clean_headers(mut headers: http::HeaderMap) -> http::HeaderMap {
    for name in [
        http::header::CONNECTION,
        http::header::UPGRADE,
        http::header::HOST,
        http::header::CONTENT_LENGTH,
    ] {
        headers.remove(name);
    }
    let websocket = headers
        .keys()
        .filter(|name| name.as_str().starts_with("sec-websocket-"))
        .cloned()
        .collect::<Vec<_>>();
    for name in websocket {
        headers.remove(name);
    }
    headers
}

pub(super) fn drain_sse(pending: &mut String, output: &mut VecDeque<WsFrame>) -> bool {
    *pending = pending.replace("\r\n", "\n");
    let mut terminal = false;
    while let Some(end) = pending.find("\n\n") {
        let block = pending[..end].to_owned();
        pending.drain(..end + 2);
        let data = block
            .lines()
            .filter_map(|line| line.strip_prefix("data:"))
            .map(str::trim_start)
            .collect::<Vec<_>>()
            .join("\n");
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        terminal |= serde_json::from_str::<serde_json::Value>(&data)
            .ok()
            .and_then(|value| value.get("type")?.as_str().map(str::to_owned))
            .is_some_and(|kind| matches!(kind.as_str(), "response.completed" | "response.failed"));
        output.push_back(WsFrame::Text(data));
    }
    terminal
}

pub(super) fn warmup_event() -> String {
    serde_json::json!({
        "type":"response.completed","sequence_number":0,
        "response":{"id":"gproxy-warmup","object":"response","created_at":0,
            "status":"completed","output":[],
            "usage":{"input_tokens":0,"output_tokens":0,"total_tokens":0,
                "output_tokens_details":{"reasoning_tokens":0}}}
    })
    .to_string()
}
