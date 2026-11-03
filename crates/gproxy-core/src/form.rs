//! Multipart form helpers shared by request preparation and realtime offers.
use crate::{CoreError, CoreResult};
use gproxy_protocol::{
    HttpBody,
    codec::{CodecLimits, MultipartDecoder, MultipartEncoder, read_http_body},
    connection::{Bytes, MultipartPart},
};
use http::{HeaderMap, header};

fn invalid(error: impl std::fmt::Display) -> CoreError {
    CoreError::InvalidTarget(format!("invalid multipart body: {error}"))
}

pub(crate) fn field(headers: &HeaderMap, name: http::HeaderName, key: &str) -> Option<String> {
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

pub(crate) async fn multipart(
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

/// Re-encodes edited parts with the original boundary.
pub(crate) async fn encode(
    boundary: String,
    parts: Vec<MultipartPart>,
    limits: CodecLimits,
) -> CoreResult<Bytes> {
    let mut encoder = MultipartEncoder::new(boundary, parts, limits).map_err(invalid)?;
    let mut bytes = Vec::new();
    while let Some(chunk) = encoder.next_chunk().await.map_err(invalid)? {
        bytes.extend_from_slice(&chunk);
    }
    Ok(Bytes::from(bytes))
}
