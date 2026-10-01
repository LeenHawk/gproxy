//! Model selection in WebRTC offers: JSON session envelopes and multipart
//! session fields. SDP and all unrelated fields survive model mapping.
use crate::{CoreError, CoreResult, PathSegment};
use gproxy_protocol::{
    HttpBody, WireRequest,
    codec::{CodecLimits, MultipartDecoder, MultipartEncoder, read_http_body},
    connection::{Bytes, MultipartPart},
};
use http::{HeaderMap, header};
use serde_json::Value;

fn invalid(error: impl std::fmt::Display) -> CoreError {
    CoreError::InvalidTarget(format!("invalid realtime offer: {error}"))
}

fn field(headers: &HeaderMap, name: http::HeaderName, key: &str) -> Option<String> {
    headers
        .get(name)?
        .to_str()
        .ok()?
        .split(';')
        .skip(1)
        .find_map(|part| {
            let (name, value) = part.trim().split_once('=')?;
            name.eq_ignore_ascii_case(key)
                .then(|| value.trim().trim_matches('"').to_owned())
        })
}

async fn multipart(
    headers: &HeaderMap,
    bytes: &Bytes,
    limits: CodecLimits,
) -> CoreResult<Option<(String, Vec<MultipartPart>)>> {
    if !headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| {
            v.split(';')
                .next()
                .is_some_and(|m| m.trim().eq_ignore_ascii_case("multipart/form-data"))
        })
    {
        return Ok(None);
    }
    let boundary = field(headers, header::CONTENT_TYPE, "boundary")
        .filter(|v| !v.is_empty())
        .ok_or_else(|| invalid("multipart boundary missing"))?;
    let mut decoder = MultipartDecoder::new(HttpBody::Bytes(bytes.clone()), &boundary, limits)
        .map_err(invalid)?;
    let mut parts = Vec::new();
    while let Some(mut part) = decoder.next_part().await.map_err(invalid)? {
        part.body = HttpBody::Bytes(read_http_body(part.body, limits).await.map_err(invalid)?);
        parts.push(part);
    }
    Ok(Some((boundary, parts)))
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
            let mut encoder = MultipartEncoder::new(boundary, parts, limits).map_err(invalid)?;
            let mut bytes = Vec::new();
            while let Some(chunk) = encoder.next_chunk().await.map_err(invalid)? {
                bytes.extend_from_slice(&chunk);
            }
            request.body = HttpBody::Bytes(Bytes::from(bytes));
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
