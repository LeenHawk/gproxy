//! One HTTP POST carrying one JSON-RPC message.

use futures_util::StreamExt;
use gproxy_protocol::{
    HttpBody, WireResponse,
    codec::{CodecLimits, SseDecoder, SseFrame},
    connection::Bytes,
};
use http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use serde_json::{Value, json};

use super::{Answer, CallError, Target};

/// The largest answer read from a server.
const MAX_BODY: u64 = 16 << 20;
const LIMITS: CodecLimits = CodecLimits {
    max_buffer_bytes: MAX_BODY,
    max_value_bytes: MAX_BODY,
    max_body_bytes: MAX_BODY,
    max_line_bytes: MAX_BODY,
    max_part_bytes: MAX_BODY,
    max_parts: 1,
};

/// The id of every request this client sends: one request per POST, so
/// nothing has to tell them apart.
const REQUEST_ID: u64 = 1;

pub(super) struct Reply {
    pub status: StatusCode,
    pub headers: HeaderMap,
    /// The JSON-RPC response to the request, when the body held one.
    pub response: Option<Value>,
}

impl Reply {
    /// The answer to a request, for a caller that has dealt with the
    /// statuses that mean something to it.
    pub fn answer(self) -> Result<Answer, CallError> {
        let Some(mut response) = self.response else {
            return Err(match self.status.is_success() {
                true => CallError::Invalid("no response to the request".to_owned()),
                false => CallError::Status(self.status),
            });
        };
        if let Some(error) = response.get_mut("error").map(Value::take) {
            return Ok(Err(error));
        }
        match response.get_mut("result").map(Value::take) {
            Some(result) => Ok(Ok(result)),
            None => Err(CallError::Invalid(
                "a response with neither result nor error".to_owned(),
            )),
        }
    }
}

pub(super) fn request_body(method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": REQUEST_ID, "method": method, "params": params })
}

fn is_response(message: &Value) -> bool {
    message.get("id").and_then(Value::as_u64) == Some(REQUEST_ID)
        && (message.get("result").is_some() || message.get("error").is_some())
}

fn limit() -> CallError {
    CallError::Invalid("the answer is larger than 16 MiB".to_owned())
}

/// Read an event stream up to the response; what a server sends before it
/// (progress, logging) is dropped, and the stream is not read past it.
async fn response_from_events(body: HttpBody) -> Result<Option<Value>, CallError> {
    let mut decoder = SseDecoder::new(LIMITS);
    let find = |frames: Vec<SseFrame>| {
        frames.into_iter().find_map(|frame| match frame {
            SseFrame::Event(event) => serde_json::from_str::<Value>(&event.data)
                .ok()
                .filter(is_response),
            SseFrame::Done => None,
        })
    };
    let invalid = |error: gproxy_protocol::codec::CodecError| CallError::Invalid(error.to_string());
    match body {
        HttpBody::Bytes(bytes) => Ok(find(decoder.push(&bytes).map_err(invalid)?)),
        HttpBody::Stream(mut chunks) => {
            while let Some(chunk) = chunks.next().await {
                let chunk = chunk.map_err(|error| CallError::Invalid(error.to_string()))?;
                if let Some(response) = find(decoder.push(&chunk).map_err(invalid)?) {
                    return Ok(Some(response));
                }
            }
            Ok(None)
        }
    }
}

async fn whole(body: HttpBody) -> Result<Bytes, CallError> {
    gproxy_protocol::codec::read_http_body(body, LIMITS)
        .await
        .map_err(|_| limit())
}

pub(super) async fn send(
    target: Target<'_>,
    method: Method,
    extra: HeaderMap,
    body: Option<&Value>,
) -> Result<Reply, CallError> {
    let mut headers = target.headers.clone();
    headers.insert(
        header::ACCEPT,
        HeaderValue::from_static("application/json, text/event-stream"),
    );
    if body.is_some() {
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
    }
    headers.extend(extra);
    let mut request = http::Request::builder().method(method).uri(target.endpoint);
    if let Some(map) = request.headers_mut() {
        *map = headers;
    }
    let payload = body.map(|body| body.to_string()).unwrap_or_default();
    let request = request
        .body(HttpBody::Bytes(Bytes::from(payload)))
        .map_err(|error| CallError::Invalid(format!("{}: {error}", target.endpoint)))?;
    let WireResponse {
        status,
        headers,
        body,
    } = target.client.send(request).await?;
    if status == StatusCode::UNAUTHORIZED {
        return Err(CallError::Unauthorized);
    }
    let is_events = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("text/event-stream"));
    let response = if is_events {
        response_from_events(body).await?
    } else {
        // An error status may carry a JSON-RPC error; anything else in a
        // body that is not one is no answer.
        serde_json::from_slice::<Value>(&whole(body).await?)
            .ok()
            .filter(is_response)
    };
    Ok(Reply {
        status,
        headers,
        response,
    })
}

pub(super) fn header(
    name: &'static str,
    value: &str,
) -> Result<(header::HeaderName, HeaderValue), CallError> {
    HeaderValue::from_str(value)
        .map(|value| (header::HeaderName::from_static(name), value))
        .map_err(|_| CallError::Invalid(format!("`{value}` cannot be sent as `{name}`")))
}
