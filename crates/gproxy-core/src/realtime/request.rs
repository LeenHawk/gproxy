//! Model selection in WebRTC offers: JSON session envelopes and multipart
//! session fields. SDP and all unrelated fields survive model mapping.
use crate::form::{field, multipart};
use crate::{CoreError, CoreResult, PathSegment};
use gproxy_protocol::{HttpBody, WireRequest, codec::CodecLimits, connection::Bytes};
use http::{HeaderMap, header};
use serde_json::Value;

fn invalid(error: impl std::fmt::Display) -> CoreError {
    CoreError::InvalidTarget(format!("invalid realtime offer: {error}"))
}

/// Read audio settings from either the JSON envelope or the multipart session.
/// Voice ownership checks use the same decoding rules as model selection.
pub(crate) async fn session_value(
    headers: &HeaderMap,
    body: &[u8],
    limits: CodecLimits,
) -> CoreResult<Option<Value>> {
    let bytes = Bytes::copy_from_slice(body);
    if let Some((_, parts)) = multipart(headers, &bytes, limits).await? {
        let mut session = None;
        for part in parts {
            if field(&part.headers, header::CONTENT_DISPOSITION, "name").as_deref()
                != Some("session")
            {
                continue;
            }
            if session.is_some() {
                return Err(invalid("duplicate session field"));
            }
            let HttpBody::Bytes(bytes) = part.body else {
                unreachable!()
            };
            session = Some(serde_json::from_slice(&bytes).map_err(invalid)?);
        }
        return Ok(session);
    }
    Ok(serde_json::from_slice::<Value>(body)
        .ok()
        .map(|mut value| value.get_mut("session").map(Value::take).unwrap_or(value)))
}

/// Extract a model before admission, without consuming or changing the offer.
pub async fn model(
    headers: &HeaderMap,
    bytes: &Bytes,
    limits: CodecLimits,
) -> CoreResult<Option<String>> {
    if let Some((_, parts)) = multipart(headers, bytes, limits).await? {
        let mut model = None;
        let mut seen = false;
        for part in parts {
            if field(&part.headers, header::CONTENT_DISPOSITION, "name").as_deref()
                != Some("session")
            {
                continue;
            }
            if seen {
                return Err(invalid("duplicate session field"));
            }
            seen = true;
            let HttpBody::Bytes(bytes) = part.body else {
                unreachable!()
            };
            let session: Value = serde_json::from_slice(&bytes).map_err(invalid)?;
            model = session
                .get("model")
                .and_then(Value::as_str)
                .map(str::to_owned);
        }
        return Ok(model);
    }
    let body: Value = match serde_json::from_slice(bytes) {
        Ok(body) => body,
        // An application/sdp offer has no model selector.
        Err(_) => return Ok(None),
    };
    Ok(body
        .pointer("/session/model")
        .or_else(|| body.get("model"))
        .and_then(Value::as_str)
        .map(str::to_owned))
}

pub(crate) async fn apply_model(
    request: &mut WireRequest<HttpBody>,
    model: &str,
    limits: CodecLimits,
) -> CoreResult<()> {
    let HttpBody::Bytes(bytes) = &request.body else {
        return Ok(());
    };
    let rewrite = |bytes: &[u8], path: Vec<PathSegment>| {
        crate::rewrite::json_path::rewrite_at_paths(
            std::str::from_utf8(bytes).ok()?,
            &[path],
            &mut |old| (old != model).then(|| model.to_owned()),
        )
    };
    if let Some((boundary, mut parts)) = multipart(&request.headers, bytes, limits).await? {
        let mut changed = false;
        for part in &mut parts {
            if field(&part.headers, header::CONTENT_DISPOSITION, "name").as_deref()
                == Some("session")
                && let HttpBody::Bytes(bytes) = &part.body
                && let Some(value) = rewrite(bytes, vec![PathSegment::Key("model".into())])
            {
                part.body = HttpBody::Bytes(Bytes::from(value));
                part.headers.remove(header::CONTENT_LENGTH);
                changed = true;
            }
        }
        if changed {
            request.body = HttpBody::Bytes(crate::form::encode(boundary, parts, limits).await?);
            request.headers.remove(header::CONTENT_LENGTH);
        }
    } else if let Some(value) = rewrite(
        bytes,
        vec![
            PathSegment::Key("session".into()),
            PathSegment::Key("model".into()),
        ],
    ) {
        request.body = HttpBody::Bytes(Bytes::from(value));
        request.headers.remove(header::CONTENT_LENGTH);
    }
    Ok(())
}
