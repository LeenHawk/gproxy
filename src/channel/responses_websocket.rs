//! Shared upstream OpenAI Responses WebSocket transport.

use bytes::Bytes;
#[cfg(not(target_arch = "wasm32"))]
use http::StatusCode;
#[cfg(not(target_arch = "wasm32"))]
use http::header::CONTENT_TYPE;
use http::header::{HeaderName, HeaderValue};
use http::{HeaderMap, Method, Request, Uri};

use crate::channel::{ChannelError, PreparedRequest};

const OPENAI_BETA: HeaderName = HeaderName::from_static("openai-beta");
const RESPONSES_WEBSOCKETS_BETA: &str = "responses_websockets=2026-02-06";

pub(crate) fn is_target(method: &Method, path: &str) -> bool {
    *method == Method::GET && path == "/v1/responses"
}

pub(crate) fn apply_beta(headers: &mut HeaderMap) {
    headers.insert(
        OPENAI_BETA,
        HeaderValue::from_static(RESPONSES_WEBSOCKETS_BETA),
    );
}

pub(crate) fn websocket_uri(uri: &Uri) -> Result<Uri, ChannelError> {
    let mut text = uri.to_string();
    if let Some(rest) = text.strip_prefix("https://") {
        text = format!("wss://{rest}");
    } else if let Some(rest) = text.strip_prefix("http://") {
        text = format!("ws://{rest}");
    }
    text.parse()
        .map_err(|error| ChannelError::Build(format!("bad websocket uri: {error}")))
}

pub(crate) fn prepare(request: Request<Bytes>) -> Result<PreparedRequest, ChannelError> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        Ok(PreparedRequest::custom_stream(Box::new(move |client| {
            Box::pin(async move { stream_request(client, request).await })
        })))
    }
    #[cfg(target_arch = "wasm32")]
    {
        Ok(PreparedRequest::custom(Box::new(move |client| {
            Box::pin(async move { client.send_websocket(request).await })
        })))
    }
}

#[cfg(not(target_arch = "wasm32"))]
async fn stream_request(
    client: std::sync::Arc<dyn crate::http::client::UpstreamClient>,
    request: Request<Bytes>,
) -> Result<
    (StatusCode, HeaderMap, crate::http::client::RespStream),
    crate::http::client::ClientError,
> {
    use futures_util::StreamExt as _;

    let frame = String::from_utf8(request.body().to_vec()).map_err(|error| {
        crate::http::client::ClientError::Transport(format!(
            "responses websocket request is not UTF-8 JSON: {error}"
        ))
    })?;
    let mut socket = client.open_websocket(request).await?;
    socket.send_text(frame).await?;

    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("text/event-stream"));
    let stream = futures_util::stream::unfold((socket, false), |(mut socket, done)| async move {
        if done {
            return None;
        }
        match socket.recv_text().await {
            Some(Ok(text)) => {
                let done = terminal_frame(&text);
                Some((Ok(Bytes::from(text_to_sse(&text))), (socket, done)))
            }
            Some(Err(error)) => Some((Err(error), (socket, true))),
            None => None,
        }
    })
    .boxed();
    Ok((StatusCode::OK, headers, stream))
}

#[cfg(not(target_arch = "wasm32"))]
fn terminal_frame(text: &str) -> bool {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return false;
    };
    matches!(
        value.get("type").and_then(serde_json::Value::as_str),
        Some("response.completed" | "response.done" | "response.failed" | "error")
    )
}

#[cfg(not(target_arch = "wasm32"))]
fn text_to_sse(text: &str) -> Vec<u8> {
    let name = serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .and_then(|value| {
            value
                .get("type")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "message".to_owned());
    crate::transform::common::sse::SseFrame::event(name, text.to_owned())
        .encode()
        .into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn websocket_uri_rewrites_http_schemes() {
        let https: Uri = "https://api.openai.com/v1/responses".parse().unwrap();
        assert_eq!(
            websocket_uri(&https).unwrap(),
            "wss://api.openai.com/v1/responses"
        );
        let http: Uri = "http://localhost:1234/v1/responses".parse().unwrap();
        assert_eq!(
            websocket_uri(&http).unwrap(),
            "ws://localhost:1234/v1/responses"
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn text_frame_becomes_responses_sse() {
        let out = text_to_sse(r#"{"type":"response.completed"}"#);
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "event: response.completed\ndata: {\"type\":\"response.completed\"}\n\n"
        );
    }
}
