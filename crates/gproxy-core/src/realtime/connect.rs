//! Model selectors for Realtime handshakes and Gemini's first setup message.
use crate::{CoreError, CoreResult, PathSegment};
use gproxy_protocol::{Dialect, Operation, OperationKey, WireRequest, connection::WsFrame};

/// Resolve the client selector before admission/routing. An explicit selector
/// has already been qualified by the host's mount. Continuations use their
/// cached binding instead of treating a query model as a new session.
pub fn model(
    operation: OperationKey,
    path: &str,
    query: Option<&str>,
    explicit: Option<&str>,
) -> CoreResult<Option<String>> {
    if operation.operation != Operation::ConnectRealtime {
        return Ok(explicit.map(str::to_owned));
    }
    if let Some(model) = explicit {
        return required(model).map(Some);
    }
    match operation.dialect {
        Dialect::OpenAi => {
            if super::request_call_id(path, query)?.is_some() {
                return Ok(None);
            }
            let mut models = form_urlencoded::parse(query.unwrap_or("").as_bytes())
                .filter(|(key, _)| key == "model");
            let value = models
                .next()
                .ok_or_else(|| invalid("Realtime requires model or call_id"))?;
            if models.next().is_some() {
                return Err(invalid("duplicate Realtime model selector"));
            }
            required(&value.1).map(Some)
        }
        Dialect::Gemini => Err(invalid(
            "Gemini Live requires setup.model before connecting",
        )),
        _ => Ok(None),
    }
}

/// Read only the routing field; future setup controls remain opaque to the
/// gateway, and the original frame is forwarded after binding its model.
pub fn setup_model(bytes: &[u8]) -> CoreResult<String> {
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|e| invalid(e.to_string()))?;
    let model = value
        .pointer("/setup/model")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| invalid("first Gemini Live message requires setup.model"))?;
    required(model.strip_prefix("models/").unwrap_or(model))
}

fn required(model: &str) -> CoreResult<String> {
    if model.trim().is_empty() {
        Err(invalid("Realtime model must not be empty"))
    } else {
        Ok(model.to_owned())
    }
}
fn invalid(message: impl Into<String>) -> CoreError {
    CoreError::InvalidTarget(message.into())
}

pub(crate) fn apply_query_model(request: &mut WireRequest<()>, model: &str) {
    let encoded = form_urlencoded::Serializer::new(String::new())
        .append_pair("model", model)
        .finish();
    let mut pairs = request
        .query
        .as_deref()
        .unwrap_or("")
        .split('&')
        .filter(|pair| {
            !pair.is_empty()
                && !form_urlencoded::parse(pair.as_bytes()).any(|(key, _)| key == "model")
        })
        .collect::<Vec<_>>();
    pairs.push(&encoded);
    request.query = Some(pairs.join("&"));
}

pub(crate) fn apply_setup_model(frame: &mut WsFrame, model: &str) {
    let (text, binary) = match frame {
        WsFrame::Text(text) => (text.as_str(), false),
        WsFrame::Binary(bytes) => match std::str::from_utf8(bytes) {
            Ok(text) => (text, true),
            Err(_) => return,
        },
        _ => return,
    };
    let model = if model.starts_with("models/") {
        model.to_owned()
    } else {
        format!("models/{model}")
    };
    if let Some(rewritten) = crate::rewrite::json_path::rewrite_at_paths(
        text,
        &[vec![
            PathSegment::Key("setup".into()),
            PathSegment::Key("model".into()),
        ]],
        &mut |old| (old != model).then(|| model.clone()),
    ) {
        *frame = if binary {
            WsFrame::Binary(rewritten.into_bytes().into())
        } else {
            WsFrame::Text(rewritten)
        };
    }
}
