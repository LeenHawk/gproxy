#![allow(dead_code)]
use gproxy_client_sdk::*;
use protocol::{HttpBody, OperationKey, WireRequest, WireResponse, capability::CapabilityFuture};
use serde_json::{Value, json};
use std::{collections::VecDeque, sync::Mutex};
pub struct Reply {
    pub status: http::StatusCode,
    pub headers: http::HeaderMap,
    pub bytes: Vec<u8>,
}
impl Reply {
    pub fn json(value: Value) -> Self {
        Self {
            status: http::StatusCode::OK,
            headers: [(
                http::header::CONTENT_TYPE,
                http::HeaderValue::from_static("application/json"),
            )]
            .into_iter()
            .collect(),
            bytes: value.to_string().into_bytes(),
        }
    }
    pub fn binary(mime: &str, bytes: Vec<u8>) -> Self {
        Self {
            status: http::StatusCode::OK,
            headers: [
                (
                    http::header::CONTENT_TYPE,
                    http::HeaderValue::from_str(mime).unwrap(),
                ),
                (
                    http::HeaderName::from_static("x-request-id"),
                    http::HeaderValue::from_static("request-1"),
                ),
            ]
            .into_iter()
            .collect(),
            bytes,
        }
    }
    pub fn sse(events: Vec<Value>) -> Self {
        Self {
            status: http::StatusCode::OK,
            headers: [(
                http::header::CONTENT_TYPE,
                http::HeaderValue::from_static("text/event-stream"),
            )]
            .into_iter()
            .collect(),
            bytes: events
                .into_iter()
                .map(|event| format!("data: {event}\n\n"))
                .collect::<String>()
                .into_bytes(),
        }
    }
}
pub struct Sent {
    pub key: OperationKey,
    pub model: Option<String>,
    pub path: String,
    pub query: Option<String>,
    pub headers: http::HeaderMap,
    pub bytes: Vec<u8>,
}
impl Sent {
    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.bytes).unwrap()
    }
}
pub struct Mock {
    pub replies: Mutex<VecDeque<Reply>>,
    pub sent: Mutex<Vec<Sent>>,
}
impl Mock {
    pub fn new(replies: Vec<Reply>) -> Self {
        Self {
            replies: Mutex::new(replies.into()),
            sent: Mutex::new(Vec::new()),
        }
    }
}
impl Backend for Mock {
    fn send(
        &self,
        key: OperationKey,
        model: Option<&str>,
        request: WireRequest<HttpBody>,
    ) -> CapabilityFuture<'_, Result<WireResponse<HttpBody>>> {
        let model = model.map(str::to_owned);
        Box::pin(async move {
            let bytes = protocol::codec::read_http_body(request.body, DEFAULT_LIMITS)
                .await
                .unwrap()
                .to_vec();
            self.sent.lock().unwrap().push(Sent {
                key,
                model,
                path: request.path,
                query: request.query,
                headers: request.headers,
                bytes,
            });
            let response = self
                .replies
                .lock()
                .unwrap()
                .pop_front()
                .expect("unexpected extra request/retry");
            // Split every SSE byte to exercise UTF-8 and framing boundaries.
            let body = if response
                .headers
                .get(http::header::CONTENT_TYPE)
                .is_some_and(|v| v == "text/event-stream")
            {
                HttpBody::Stream(Box::pin(futures_util::stream::iter(
                    response.bytes.into_iter().map(|b| Ok(vec![b].into())),
                )))
            } else {
                HttpBody::Bytes(response.bytes.into())
            };
            Ok(WireResponse {
                status: response.status,
                headers: response.headers,
                body,
            })
        })
    }
}
pub fn gemini(parts: Vec<Value>) -> Value {
    json!({"responseId":"gemini-response","modelVersion":"served-gemini","candidates":[{"content":{"role":"model","parts":parts},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":3,"candidatesTokenCount":4,"totalTokenCount":7}})
}
pub fn image_response() -> Value {
    json!({"created":123,"output_format":"png","data":[{"b64_json":"AQID","revised_prompt":"expanded prompt","extra":"kept"}],"usage":{"input_tokens":3,"output_tokens":4,"total_tokens":7,"input_tokens_details":{"image_tokens":0,"text_tokens":3}}})
}
pub fn image_event(edit: bool, partial: bool) -> Value {
    let mut event = json!({"type":if edit {if partial{"image_edit.partial_image"}else{"image_edit.completed"}}else if partial{"image_generation.partial_image"}else{"image_generation.completed"},"b64_json":"AQID","background":"opaque","created_at":123,"output_format":"png","quality":"high","size":"1024x1024"});
    if partial {
        event["partial_image_index"] = json!(0);
    } else {
        event["usage"] = image_response()["usage"].clone();
    }
    event
}
pub fn audio() -> ContentBlock<Audio> {
    ContentBlock::new(Audio {
        source: Asset::Inline {
            bytes: vec![1, 2, 3],
        },
        format: AudioFormat::Encoded {
            media_type: "audio/wav".into(),
        },
        transcript: None,
    })
}
