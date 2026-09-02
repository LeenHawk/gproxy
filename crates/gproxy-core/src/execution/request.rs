use gproxy_protocol::{
    Affinity, ContentGenerationKind, Operation, OperationKey, OperationKind, SettleMode,
    StreamDetect, StreamFraming, WireFamily, match_ingress_for, streaming_sibling,
};

use crate::boundary::RequestCtx;
use crate::error::CoreError;

pub(crate) struct Classified {
    pub key: OperationKey,
    pub stream: bool,
    pub framing: StreamFraming,
    pub model: Option<String>,
    resource: Option<(&'static str, String)>,
    pub(super) session: Option<super::session::SessionSubject>,
}

impl Classified {
    pub(super) fn responses_websocket(model: Option<String>) -> Self {
        Self {
            key: OperationKey::content(
                Operation::StreamGenerateContent,
                ContentGenerationKind::OpenAiResponsesWebSocket,
            ),
            stream: true,
            framing: StreamFraming::WebSocket,
            model,
            resource: None,
            session: None,
        }
    }

    pub(crate) fn routing_affinity(&self, user_key_id: i64) -> i64 {
        if self.key.operation.spec().affinity == Affinity::Session {
            super::session::selection_key(self.session, user_key_id)
        } else {
            user_key_id
        }
    }

    pub(crate) fn dedupe_key(&self, provider_id: i64) -> Option<String> {
        (self.key.operation.spec().settle == SettleMode::OnCompletedStatus)
            .then(|| {
                self.resource
                    .as_ref()
                    .map(|(kind, id)| format!("gproxy:settle:{provider_id}:{kind}:{id}"))
            })
            .flatten()
    }

    pub(crate) fn resource(&self) -> Option<(&'static str, &str)> {
        self.resource
            .as_ref()
            .map(|(kind, id)| (*kind, id.as_str()))
    }
}

pub(crate) fn classify(ctx: &RequestCtx) -> Result<Classified, CoreError> {
    let preferred = WireFamily::Claude
        .client_markers()
        .iter()
        .any(|name| ctx.headers.contains_key(*name))
        .then_some(WireFamily::Claude);
    let matched =
        match_ingress_for(&ctx.method, &ctx.path, preferred).ok_or(CoreError::Unsupported)?;
    if matched.upgrade != ctx.upgrade {
        return Err(CoreError::Unsupported);
    }
    let body = serde_json::from_slice::<serde_json::Value>(&ctx.body).ok();
    let stream = detect_stream(matched.stream, body.as_ref(), &ctx.body);
    let operation = if stream {
        streaming_sibling(matched.operation).unwrap_or(matched.operation)
    } else {
        matched.operation
    };
    let framing = if stream
        && matched.kind
            == OperationKind::ContentGeneration(ContentGenerationKind::GeminiGenerateContent)
        && query_value(ctx.query.as_deref(), "alt") == Some("sse")
    {
        StreamFraming::Sse
    } else {
        matched.framing
    };
    let spec = operation.spec();
    let model = matched
        .params
        .iter()
        .find(|(name, _)| *name == "model")
        .or_else(|| {
            (operation == Operation::GetModel)
                .then(|| matched.params.first())
                .flatten()
        })
        .map(|(_, value)| value.clone())
        .or_else(|| {
            body.as_ref().and_then(|body| {
                body.get("model")
                    .or_else(|| {
                        if operation == Operation::CreateRealtimeCall {
                            body.pointer("/session/model")
                        } else {
                            None
                        }
                    })?
                    .as_str()
                    .map(str::to_owned)
            })
        });
    let resource = match spec.affinity {
        Affinity::Resource(kind) => matched
            .params
            .iter()
            .find(|(name, _)| *name == "id")
            .map(|(_, id)| (kind, id.clone())),
        Affinity::None | Affinity::Session => None,
    };
    let session = (spec.affinity == Affinity::Session)
        .then(|| super::session::subject(ctx, matched.kind, body.as_ref()))
        .flatten();
    Ok(Classified {
        key: OperationKey {
            operation,
            kind: matched.kind,
        },
        stream,
        framing,
        model,
        resource,
        session,
    })
}

fn query_value<'a>(query: Option<&'a str>, name: &str) -> Option<&'a str> {
    query?.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        (key == name).then_some(value)
    })
}

fn detect_stream(detect: StreamDetect, json: Option<&serde_json::Value>, body: &[u8]) -> bool {
    match detect {
        StreamDetect::Never => false,
        StreamDetect::Always => true,
        StreamDetect::BodyFlag(field) => json
            .and_then(|body| body.get(field)?.as_bool())
            .unwrap_or(false),
        StreamDetect::BodyValue(field, expected) => json
            .and_then(|body| body.get(field)?.as_str())
            .is_some_and(|value| value == expected),
        StreamDetect::BodyFlagOrMultipart(field) => json
            .and_then(|body| body.get(field)?.as_bool())
            .unwrap_or_else(|| multipart_flag(body, field)),
    }
}

fn multipart_flag(body: &[u8], field: &str) -> bool {
    let marker = format!("name=\"{field}\"");
    let Some(field) = find_bytes(body, marker.as_bytes()) else {
        return false;
    };
    let rest = &body[field + marker.len()..];
    let Some(value) = find_bytes(rest, b"\r\n\r\n").map(|offset| &rest[offset + 4..]) else {
        return false;
    };
    let end = find_bytes(value, b"\r\n").unwrap_or(value.len());
    value[..end].eq_ignore_ascii_case(b"true")
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|candidate| candidate == needle)
}
