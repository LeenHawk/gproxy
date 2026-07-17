//! Downstream OpenAI Responses WebSocket bridge.
//!
//! Each inbound `response.create` text frame is converted into an internal
//! streaming `POST /v1/responses` request and executed through the normal
//! pipeline. The pipeline emits Responses SSE; this module strips SSE framing
//! back to JSON text messages for the WebSocket client.

use bytes::Bytes;
use futures_util::StreamExt as _;
use http::header::{
    ACCEPT, CONNECTION, CONTENT_TYPE, SEC_WEBSOCKET_ACCEPT, SEC_WEBSOCKET_EXTENSIONS,
    SEC_WEBSOCKET_KEY, SEC_WEBSOCKET_PROTOCOL, SEC_WEBSOCKET_VERSION, UPGRADE,
};
use http::{HeaderMap, HeaderValue, Method, StatusCode, Uri};
use serde_json::{Value, json};

use crate::app::AppState;
use crate::http::server::extract::build_ctx;
use crate::pipeline;
use crate::pipeline::classify::RESPONSES_WEBSOCKET_CLASSIFY_HEADER;
use crate::pipeline::error::PipelineError;
use crate::pipeline::outcome::{ExecOutcome, ResponseBody};
use crate::transform::TransformError;
use crate::transform::generate_content::openai_responses_websocket::{
    ResponseWebSocketSseDecoder, validate_response_create_frame,
};

#[derive(Clone)]
pub(crate) struct ResponsesWsRequestBase {
    uri: Uri,
    headers: HeaderMap,
}

impl ResponsesWsRequestBase {
    pub(crate) fn new(uri: Uri, headers: HeaderMap) -> Self {
        Self { uri, headers }
    }

    pub(crate) fn from_parts(parts: &http::request::Parts) -> Self {
        Self::new(parts.uri.clone(), parts.headers.clone())
    }
}

pub(crate) fn is_responses_websocket_path(path: &str) -> bool {
    path == "/v1/responses" || scoped_provider(path).is_some()
}

pub(crate) fn is_scoped_responses_websocket_path(path: &str) -> bool {
    scoped_provider(path).is_some()
}

fn scoped_provider(path: &str) -> Option<&str> {
    let trimmed = path.trim_start_matches('/');
    let (provider, rest) = trimmed.split_once('/')?;
    if provider.is_empty() || matches!(provider, "v1" | "v1beta" | "console") {
        return None;
    }
    (rest == "v1/responses").then_some(provider)
}

pub(crate) async fn execute_frame(
    state: &AppState,
    base: &ResponsesWsRequestBase,
    scoped: bool,
    frame: &str,
) -> Result<ExecOutcome, WsFrameError> {
    if frame.len() > crate::config::MAX_BODY_BYTES {
        return Err(WsFrameError::plain(
            StatusCode::PAYLOAD_TOO_LARGE,
            "request body too large",
        ));
    }
    validate_response_create_frame(frame.as_bytes()).map_err(WsFrameError::from_transform)?;
    let body = Bytes::copy_from_slice(frame.as_bytes());

    let ctx = build_ctx(internal_post_parts(base), body, scoped).map_err(WsFrameError::from)?;
    pipeline::execute(state, ctx)
        .await
        .map_err(WsFrameError::from)
}

fn internal_post_parts(base: &ResponsesWsRequestBase) -> http::request::Parts {
    let mut parts = http::Request::builder()
        .method(Method::POST)
        .uri(base.uri.clone())
        .body(())
        .expect("internal websocket request parts")
        .into_parts()
        .0;
    parts.headers = base.headers.clone();
    strip_websocket_upgrade_headers(&mut parts.headers);
    parts
        .headers
        .insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    parts
        .headers
        .insert(ACCEPT, HeaderValue::from_static("text/event-stream"));
    parts.headers.insert(
        RESPONSES_WEBSOCKET_CLASSIFY_HEADER,
        HeaderValue::from_static("1"),
    );
    parts
}

fn strip_websocket_upgrade_headers(headers: &mut HeaderMap) {
    headers.remove(CONNECTION);
    headers.remove(UPGRADE);
    headers.remove(SEC_WEBSOCKET_ACCEPT);
    headers.remove(SEC_WEBSOCKET_EXTENSIONS);
    headers.remove(SEC_WEBSOCKET_KEY);
    headers.remove(SEC_WEBSOCKET_PROTOCOL);
    headers.remove(SEC_WEBSOCKET_VERSION);
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn execute_frame_collect(
    state: &AppState,
    base: &ResponsesWsRequestBase,
    scoped: bool,
    frame: &str,
) -> Vec<String> {
    match execute_frame(state, base, scoped, frame).await {
        Ok(outcome) => match outcome_to_messages(outcome).await {
            Ok(messages) => messages,
            Err(error) => vec![error.to_frame()],
        },
        Err(error) => vec![error.to_frame()],
    }
}

pub(crate) async fn outcome_to_messages(outcome: ExecOutcome) -> Result<Vec<String>, WsFrameError> {
    if !outcome.status.is_success() {
        let headers = outcome.headers.clone();
        let body = collect_body(outcome.body).await?;
        let text = String::from_utf8_lossy(&body);
        return Ok(vec![
            WsFrameError::upstream(outcome.status, &text, &headers).to_frame(),
        ]);
    }

    let mut decoder = ResponseWebSocketSseDecoder::new();
    let mut messages = Vec::new();
    match outcome.body {
        ResponseBody::Full(body) => {
            messages.extend(decoder.push(&body));
            messages.extend(decoder.finish());
            if messages.is_empty() && !body.is_empty() {
                messages.push(json_body_to_message(&body)?);
            }
        }
        ResponseBody::Stream(mut stream) => {
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|error| {
                    WsFrameError::plain(StatusCode::BAD_GATEWAY, &error.to_string())
                })?;
                messages.extend(decoder.push(&chunk));
            }
            messages.extend(decoder.finish());
        }
    }
    Ok(messages)
}

fn json_body_to_message(body: &Bytes) -> Result<String, WsFrameError> {
    serde_json::from_slice::<Value>(body)
        .map_err(|error| {
            WsFrameError::plain(
                StatusCode::BAD_GATEWAY,
                &format!("upstream returned non-SSE websocket response: {error}"),
            )
        })
        .map(|_| String::from_utf8_lossy(body).into_owned())
}

async fn collect_body(body: ResponseBody) -> Result<Bytes, WsFrameError> {
    match body {
        ResponseBody::Full(body) => Ok(body),
        ResponseBody::Stream(mut stream) => {
            let mut out = Vec::new();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|error| {
                    WsFrameError::plain(StatusCode::BAD_GATEWAY, &error.to_string())
                })?;
                out.extend_from_slice(&chunk);
            }
            Ok(Bytes::from(out))
        }
    }
}

#[derive(Debug)]
pub(crate) struct WsFrameError {
    status: StatusCode,
    body: String,
    retry_after_secs: Option<u64>,
    headers: Box<HeaderMap>,
}

impl WsFrameError {
    pub(crate) fn plain(status: StatusCode, message: &str) -> Self {
        Self {
            status,
            body: json!({ "error": { "message": message, "type": "gproxy_error" } }).to_string(),
            retry_after_secs: None,
            headers: Box::new(HeaderMap::new()),
        }
    }

    fn from_transform(error: TransformError) -> Self {
        Self::plain(StatusCode::UNPROCESSABLE_ENTITY, &error.to_string())
    }

    fn upstream(status: StatusCode, body: &str, headers: &HeaderMap) -> Self {
        Self {
            status,
            body: body.to_owned(),
            retry_after_secs: None,
            headers: Box::new(headers.clone()),
        }
    }

    pub(crate) fn to_frame(&self) -> String {
        let mut payload = json!({
            "type": "error",
            "status": self.status.as_u16(),
            "status_code": self.status.as_u16(),
            "error": error_value(&self.body),
        });
        if let Some(secs) = self.retry_after_secs {
            payload["headers"] = json!({ "retry-after": secs.to_string() });
        }
        let headers = headers_json(&self.headers);
        if !headers.is_empty() {
            let target = payload
                .as_object_mut()
                .expect("error frame payload is object")
                .entry("headers")
                .or_insert_with(|| Value::Object(Default::default()));
            if let Some(target) = target.as_object_mut() {
                target.extend(headers);
            }
        }
        payload.to_string()
    }
}

impl From<PipelineError> for WsFrameError {
    fn from(error: PipelineError) -> Self {
        Self {
            status: error.status(),
            body: error.error_json(),
            retry_after_secs: error.retry_after_secs(),
            headers: Box::new(HeaderMap::new()),
        }
    }
}

fn error_value(body: &str) -> Value {
    let Ok(value) = serde_json::from_str::<Value>(body) else {
        return json!({ "message": body, "type": "gproxy_error" });
    };
    value
        .get("error")
        .cloned()
        .unwrap_or_else(|| json!({ "message": value.to_string(), "type": "gproxy_error" }))
}

fn headers_json(headers: &HeaderMap) -> serde_json::Map<String, Value> {
    let mut out = serde_json::Map::new();
    for (name, value) in headers {
        if let Ok(value) = value.to_str() {
            out.insert(name.as_str().to_owned(), Value::String(value.to_owned()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel::Disposition;
    use crate::pipeline::classify::classify;
    use crate::protocol::{ContentGenerationKind, Operation, OperationKey};

    #[test]
    fn path_match_is_exact_for_aggregated_and_scoped_responses() {
        assert!(is_responses_websocket_path("/v1/responses"));
        assert!(is_responses_websocket_path("/codex/v1/responses"));
        assert!(!is_responses_websocket_path("/v1/chat/completions"));
        assert!(!is_responses_websocket_path("/v1/v1/responses"));
        assert!(!is_responses_websocket_path("/console/v1/responses"));
    }

    #[test]
    fn error_frame_uses_wrapped_websocket_shape() {
        let frame = WsFrameError::plain(StatusCode::UNPROCESSABLE_ENTITY, "bad frame").to_frame();
        let value: Value = serde_json::from_str(&frame).unwrap();
        assert_eq!(value["type"], "error");
        assert_eq!(value["status"], 422);
        assert_eq!(value["status_code"], 422);
        assert_eq!(value["error"]["message"], "bad frame");
    }

    #[test]
    fn internal_post_strips_websocket_upgrade_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(CONNECTION, HeaderValue::from_static("upgrade"));
        headers.insert(UPGRADE, HeaderValue::from_static("websocket"));
        headers.insert(SEC_WEBSOCKET_KEY, HeaderValue::from_static("abc"));
        headers.insert("authorization", HeaderValue::from_static("Bearer key"));
        let base = ResponsesWsRequestBase::new(Uri::from_static("/v1/responses"), headers);

        let parts = internal_post_parts(&base);

        assert_eq!(parts.method, Method::POST);
        assert_eq!(parts.headers.get(CONNECTION), None);
        assert_eq!(parts.headers.get(UPGRADE), None);
        assert_eq!(parts.headers.get(SEC_WEBSOCKET_KEY), None);
        assert_eq!(
            parts.headers.get(CONTENT_TYPE),
            Some(&HeaderValue::from_static("application/json"))
        );
        assert_eq!(
            parts.headers.get(ACCEPT),
            Some(&HeaderValue::from_static("text/event-stream"))
        );
        assert_eq!(
            parts.headers.get("authorization"),
            Some(&HeaderValue::from_static("Bearer key"))
        );
    }

    #[test]
    fn websocket_frame_enters_pipeline_as_streaming_http_responses() {
        let body =
            Bytes::from(br#"{"type":"response.create","model":"m","input":"hi"}"#.as_slice());
        let base =
            ResponsesWsRequestBase::new(Uri::from_static("/codex/v1/responses"), HeaderMap::new());
        let parts = internal_post_parts(&base);
        let ctx = build_ctx(parts, body, true).unwrap();
        let classified = classify(&ctx.method, &ctx.path, &ctx.headers, &ctx.body).unwrap();

        assert_eq!(ctx.path, "/v1/responses");
        assert_eq!(
            classified.op,
            OperationKey::content_generation(
                Operation::StreamGenerateContent,
                ContentGenerationKind::OpenAiResponsesWebSocket
            )
        );
        assert!(classified.stream);
    }

    #[tokio::test]
    async fn success_sse_body_becomes_plain_websocket_messages() {
        let outcome = ExecOutcome {
            status: StatusCode::OK,
            headers: HeaderMap::new(),
            body: ResponseBody::Full(Bytes::from_static(
                b"event: response.created\ndata: {\"type\":\"response.created\"}\n\ndata: [DONE]\n\n",
            )),
            disposition: Disposition::Success,
        };

        let messages = outcome_to_messages(outcome).await.unwrap();
        assert_eq!(messages, vec![r#"{"type":"response.created"}"#]);
    }

    #[tokio::test]
    async fn success_json_body_is_forwarded_as_one_message() {
        let outcome = ExecOutcome {
            status: StatusCode::OK,
            headers: HeaderMap::new(),
            body: ResponseBody::Full(Bytes::from_static(br#"{"type":"response.completed"}"#)),
            disposition: Disposition::Success,
        };

        let messages = outcome_to_messages(outcome).await.unwrap();
        assert_eq!(messages, vec![r#"{"type":"response.completed"}"#]);
    }
}
