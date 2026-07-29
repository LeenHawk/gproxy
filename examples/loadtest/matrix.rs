//! The 4×4×2 combination matrix and per-combination request templates.

use bytes::Bytes;
use gproxy::pipeline::{RequestCtx, RoutingMode};
use http::{HeaderMap, Method};
use serde_json::json;

/// One content-generation wire format (used for both inbound and upstream).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Wire {
    Chat,
    Responses,
    Claude,
    Gemini,
}

impl Wire {
    pub const ALL: [Wire; 4] = [Wire::Chat, Wire::Responses, Wire::Claude, Wire::Gemini];

    pub fn name(self) -> &'static str {
        match self {
            Wire::Chat => "chat",
            Wire::Responses => "responses",
            Wire::Claude => "claude",
            Wire::Gemini => "gemini",
        }
    }

    pub fn parse(s: &str) -> Option<Wire> {
        Wire::ALL.into_iter().find(|w| w.name() == s)
    }

    /// Global alias routing to the provider whose upstream speaks this wire.
    pub fn alias(self) -> &'static str {
        match self {
            Wire::Chat => "m-chat",
            Wire::Responses => "m-responses",
            Wire::Claude => "m-claude",
            Wire::Gemini => "m-gemini",
        }
    }

    /// `ContentGenerationKind` serde string (routing_rules `kind`/`dest_kind`).
    pub fn kind_str(self) -> &'static str {
        match self {
            Wire::Chat => "open_ai_chat_completions",
            Wire::Responses => "open_ai_responses",
            Wire::Claude => "claude_messages",
            Wire::Gemini => "gemini_generate_content",
        }
    }
}

/// One matrix cell: inbound wire × upstream wire × stream flag.
#[derive(Clone, Copy)]
pub struct Combo {
    pub inbound: Wire,
    pub upstream: Wire,
    pub stream: bool,
}

impl Combo {
    /// All 32 combinations, grouped stream-last per (inbound, upstream) pair.
    pub fn all() -> Vec<Combo> {
        let mut v = Vec::with_capacity(32);
        for inbound in Wire::ALL {
            for upstream in Wire::ALL {
                for stream in [false, true] {
                    v.push(Combo {
                        inbound,
                        upstream,
                        stream,
                    });
                }
            }
        }
        v
    }

    pub fn label(&self) -> String {
        format!(
            "{}->{} {}",
            self.inbound.name(),
            self.upstream.name(),
            if self.stream { "stream" } else { "buffered" }
        )
    }
}

/// Prebuilt request parts for a combo; per-request cost is a couple of clones.
pub struct RequestTemplate {
    path: String,
    query: Option<String>,
    headers: HeaderMap,
    body: Bytes,
}

impl RequestTemplate {
    pub fn new(combo: &Combo) -> Self {
        let model = combo.upstream.alias();
        let (path, query, body) = match combo.inbound {
            Wire::Chat => (
                "/v1/chat/completions".to_owned(),
                None,
                json!({ "model": model, "stream": combo.stream,
                        "messages": [{ "role": "user", "content": "hi" }] }),
            ),
            Wire::Responses => (
                "/v1/responses".to_owned(),
                None,
                json!({ "model": model, "stream": combo.stream, "input": "hi" }),
            ),
            Wire::Claude => (
                "/v1/messages".to_owned(),
                None,
                json!({ "model": model, "max_tokens": 128, "stream": combo.stream,
                        "messages": [{ "role": "user", "content": "hi" }] }),
            ),
            Wire::Gemini => {
                let verb = if combo.stream {
                    "streamGenerateContent"
                } else {
                    "generateContent"
                };
                (
                    format!("/v1beta/models/{model}:{verb}"),
                    combo.stream.then(|| "alt=sse".to_owned()),
                    json!({ "contents": [{ "role": "user", "parts": [{ "text": "hi" }] }] }),
                )
            }
        };
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer sk-test".parse().unwrap());
        headers.insert("content-type", "application/json".parse().unwrap());
        Self {
            path,
            query,
            headers,
            body: Bytes::from(serde_json::to_vec(&body).expect("request body")),
        }
    }

    pub fn ctx(&self, request_seq: u64) -> RequestCtx {
        RequestCtx {
            request_id: format!("lt-{request_seq}"),
            method: Method::POST,
            path: self.path.clone(),
            query: self.query.clone(),
            headers: self.headers.clone(),
            body: self.body.clone(),
            mode: RoutingMode::Aggregated,
            identity: None,
            op: None,
            stream: false,
            body_model: None,
            route_name: None,
            pending_micros: 0,
        }
    }
}
