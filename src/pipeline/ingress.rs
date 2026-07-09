//! Part 1 of inbound header/query handling: the GLOBAL blacklist.
//!
//! Applied ONCE in the pipeline — after auth, before channel selection — so no
//! channel can ever forward the caller's credentials/cookies upstream. The
//! per-channel allow-list (Part 2) runs later, inside `Channel::prepare`
//! (`channel::http_util::allow_headers` / `allow_query`). The two layers take
//! effect at deliberately different pipeline positions.

use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use bytes::Bytes;
use http::{HeaderMap, HeaderValue, header};
use serde_json::{Map, Value};

use crate::channel::http_util::{HOP_BY_HOP, connection_nominated};
use crate::pipeline::context::RequestCtx;
use crate::pipeline::error::PipelineError;
use crate::transform::TransformError;

/// Inbound headers globally denied upstream regardless of channel. A hard floor
/// the per-channel allow-list cannot override:
/// - hop-by-hop (`HOP_BY_HOP`);
/// - the caller's own credentials + cookies;
/// - `Host` (a fresh one is derived from the upstream URI);
/// - front-proxy / client-network metadata (would leak the client IP and is
///   meaningless upstream);
/// - `accept-encoding` (compression is managed by the transport, which also
///   matches the impersonated client; a forwarded value breaks auto-decompress
///   and content-encoding stripping).
fn is_denied_header(name: &str) -> bool {
    HOP_BY_HOP.contains(&name)
        || matches!(
            name,
            // caller credentials / cookies
            "authorization" | "x-api-key" | "x-goog-api-key" | "api-key" | "cookie"
            // host (re-derived from the upstream URI)
            | "host"
            // front-proxy / client-network metadata
            | "via" | "forwarded" | "x-forwarded-for" | "x-forwarded-host"
            | "x-forwarded-proto" | "x-real-ip"
            // transport-managed compression
            | "accept-encoding"
        )
}

/// Query parameters globally denied upstream — the inbound `?key=` used solely
/// for downstream (client → proxy) authentication.
const DENIED_QUERY: &[&str] = &["key"];

/// Apply the global blacklist to the request in place (Part 1). MUST run after
/// authentication (which reads the credential headers/params) and before the
/// channel's `prepare`. Headers the caller's `Connection:` nominates are
/// hop-by-hop too (RFC 7230 §6.1) and dropped alongside the fixed set.
pub fn apply_global_blacklist(ctx: &mut RequestCtx) {
    let nominated = connection_nominated(&ctx.headers);
    let mut headers = HeaderMap::with_capacity(ctx.headers.len());
    for (name, value) in ctx.headers.iter() {
        let n = name.as_str();
        if !is_denied_header(n) && !nominated.iter().any(|t| t == n) {
            headers.append(name.clone(), value.clone());
        }
    }
    ctx.headers = headers;
    ctx.query = ctx.query.as_deref().and_then(strip_denied_query);
}

/// Canonicalize multipart/form-data bodies into a JSON object before the
/// request reaches protocol-specific stages. Text fields stay strings; file
/// fields become `data:<mime>;base64,...`; repeated or bracket-array fields
/// become JSON arrays.
pub fn normalize_multipart_form_body(ctx: &mut RequestCtx) -> Result<(), PipelineError> {
    if !is_multipart_form(&ctx.headers) {
        return Ok(());
    }

    let body = multipart_form_to_json(&ctx.body, &ctx.headers)?;
    ctx.body = body;
    ctx.headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    ctx.headers.remove(header::CONTENT_LENGTH);
    Ok(())
}

fn strip_denied_query(query: &str) -> Option<String> {
    let kept: Vec<&str> = query
        .split('&')
        .filter(|pair| !DENIED_QUERY.contains(&pair.split('=').next().unwrap_or("")))
        .collect();
    (!kept.is_empty()).then(|| kept.join("&"))
}

fn is_multipart_form(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .split(';')
                .next()
                .is_some_and(|mime| mime.trim().eq_ignore_ascii_case("multipart/form-data"))
        })
}

fn multipart_form_to_json(body: &Bytes, headers: &HeaderMap) -> Result<Bytes, PipelineError> {
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok());
    let parts = parse_multipart(body, content_type).map_err(invalid_multipart_form)?;
    let mut map = Map::new();

    for part in parts {
        let Some(name) = part.name.as_deref().filter(|name| !name.is_empty()) else {
            continue;
        };
        let (name, force_array) = canonical_form_name(name);
        insert_form_value(&mut map, name, part_value(part), force_array);
    }

    serde_json::to_vec(&Value::Object(map))
        .map(Bytes::from)
        .map_err(|e| {
            PipelineError::TransformRequest(TransformError::Serialization {
                reason: e.to_string(),
            })
        })
}

fn invalid_multipart_form(reason: impl Into<String>) -> PipelineError {
    PipelineError::TransformRequest(TransformError::InvalidInput {
        reason: format!("multipart/form-data: {}", reason.into()),
    })
}

#[derive(Debug)]
struct MultipartPart {
    name: Option<String>,
    filename: Option<String>,
    content_type: Option<String>,
    body: Vec<u8>,
}

fn parse_multipart(body: &[u8], content_type: Option<&str>) -> Result<Vec<MultipartPart>, String> {
    let boundary = boundary(content_type, body).ok_or("missing boundary")?;
    let mut delimiter = Vec::with_capacity(boundary.len() + 2);
    delimiter.extend_from_slice(b"--");
    delimiter.extend_from_slice(&boundary);

    let first = memmem(body, &delimiter).ok_or("first boundary not found")?;
    let mut rest = &body[first + delimiter.len()..];
    let mut parts = Vec::new();

    loop {
        if rest.starts_with(b"--") {
            break;
        }
        rest = strip_prefix_newline(rest);
        if rest.is_empty() {
            break;
        }
        let end = memmem(rest, &delimiter).ok_or("trailing boundary not found")?;
        let part = strip_suffix_newline(&rest[..end]);
        if !part.is_empty() {
            parts.push(parse_part(part)?);
        }
        rest = &rest[end + delimiter.len()..];
    }

    Ok(parts)
}

fn boundary(content_type: Option<&str>, body: &[u8]) -> Option<Vec<u8>> {
    if let Some(value) = content_type {
        for param in value.split(';').skip(1) {
            let Some((key, value)) = param.trim().split_once('=') else {
                continue;
            };
            if key.trim().eq_ignore_ascii_case("boundary") {
                let value = trim_quotes(value.trim());
                if !value.is_empty() {
                    return Some(value.as_bytes().to_vec());
                }
            }
        }
    }

    let first_line = body.split(|b| *b == b'\n').next()?;
    let first_line = first_line.strip_suffix(b"\r").unwrap_or(first_line);
    first_line
        .strip_prefix(b"--")
        .filter(|value| !value.is_empty())
        .map(|value| value.to_vec())
}

fn strip_prefix_newline(value: &[u8]) -> &[u8] {
    value
        .strip_prefix(b"\r\n")
        .or_else(|| value.strip_prefix(b"\n"))
        .unwrap_or(value)
}

fn strip_suffix_newline(value: &[u8]) -> &[u8] {
    value
        .strip_suffix(b"\r\n")
        .or_else(|| value.strip_suffix(b"\n"))
        .unwrap_or(value)
}

fn parse_part(raw: &[u8]) -> Result<MultipartPart, String> {
    let (header_bytes, body) =
        split_headers_body(raw).ok_or("part header/body separator missing")?;
    let (mut name, mut filename, mut content_type) = (None, None, None);

    for line in header_bytes.split(|b| *b == b'\n') {
        let line = std::str::from_utf8(line)
            .unwrap_or("")
            .trim_end_matches('\r');
        let Some((header_name, value)) = line.split_once(':') else {
            continue;
        };
        if header_name.eq_ignore_ascii_case("content-disposition") {
            for param in value.split(';').skip(1) {
                let Some((key, value)) = param.trim().split_once('=') else {
                    continue;
                };
                let value = trim_quotes(value.trim()).to_owned();
                if key.trim().eq_ignore_ascii_case("name") {
                    name = Some(value);
                } else if key.trim().eq_ignore_ascii_case("filename") {
                    filename = Some(value);
                }
            }
        } else if header_name.eq_ignore_ascii_case("content-type") {
            content_type = Some(value.trim().to_owned());
        }
    }

    Ok(MultipartPart {
        name,
        filename,
        content_type,
        body: body.to_vec(),
    })
}

fn split_headers_body(raw: &[u8]) -> Option<(&[u8], &[u8])> {
    memmem(raw, b"\r\n\r\n")
        .map(|idx| (&raw[..idx], &raw[idx + 4..]))
        .or_else(|| memmem(raw, b"\n\n").map(|idx| (&raw[..idx], &raw[idx + 2..])))
}

fn trim_quotes(value: &str) -> &str {
    value.trim_matches('"')
}

fn memmem(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    (0..=haystack.len() - needle.len()).find(|&i| haystack[i..i + needle.len()] == *needle)
}

fn part_value(part: MultipartPart) -> Value {
    let MultipartPart {
        filename,
        content_type,
        body,
        ..
    } = part;

    if filename.is_some() || content_type.is_some() {
        let mime_type = content_type
            .or_else(|| filename.as_deref().map(guess_mime_from_name))
            .unwrap_or_else(|| "application/octet-stream".to_owned());
        return Value::String(format!("data:{mime_type};base64,{}", B64.encode(body)));
    }

    Value::String(String::from_utf8_lossy(&body).into_owned())
}

fn canonical_form_name(name: &str) -> (String, bool) {
    if let Some(base) = name.strip_suffix("[]") {
        return (base.to_owned(), true);
    }
    if let Some((base, rest)) = name.split_once('[')
        && rest.ends_with(']')
        && !base.is_empty()
    {
        return (base.to_owned(), true);
    }
    (name.to_owned(), false)
}

fn insert_form_value(map: &mut Map<String, Value>, name: String, value: Value, force_array: bool) {
    match map.remove(&name) {
        Some(Value::Array(mut values)) => {
            values.push(value);
            map.insert(name, Value::Array(values));
        }
        Some(existing) => {
            map.insert(name, Value::Array(vec![existing, value]));
        }
        None if force_array => {
            map.insert(name, Value::Array(vec![value]));
        }
        None => {
            map.insert(name, value);
        }
    }
}

fn guess_mime_from_name(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else if lower.ends_with(".gif") {
        "image/gif"
    } else {
        "application/octet-stream"
    }
    .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::context::RoutingMode;
    use bytes::Bytes;
    use http::Method;

    fn ctx(headers: HeaderMap, query: Option<&str>) -> RequestCtx {
        RequestCtx {
            request_id: "t".into(),
            method: Method::POST,
            path: "/v1/chat/completions".into(),
            query: query.map(str::to_string),
            headers,
            body: Bytes::new(),
            mode: RoutingMode::Aggregated,
            identity: None,
            op: None,
            stream: false,
            route_name: None,
            pending_micros: 0,
        }
    }

    #[test]
    fn strips_creds_cookies_hop_by_hop_keeps_rest() {
        let mut h = HeaderMap::new();
        h.insert(http::header::AUTHORIZATION, "Bearer c".parse().unwrap());
        h.insert("x-goog-api-key", "g".parse().unwrap());
        h.insert("cookie", "s=1".parse().unwrap());
        h.insert(http::header::CONNECTION, "keep-alive".parse().unwrap());
        h.insert(http::header::HOST, "client".parse().unwrap());
        h.insert("via", "1.1 Caddy".parse().unwrap());
        h.insert("x-forwarded-for", "1.2.3.4".parse().unwrap());
        h.insert(http::header::ACCEPT_ENCODING, "gzip, br".parse().unwrap());
        h.insert(
            http::header::CONTENT_TYPE,
            "application/json".parse().unwrap(),
        );
        // an SDK-fingerprint header survives the global floor (an impersonation
        // channel's allow-list may forward it; the floor must not).
        h.insert("x-stainless-lang", "js".parse().unwrap());
        let mut c = ctx(h, Some("key=secret&alt=sse"));
        apply_global_blacklist(&mut c);

        assert!(c.headers.get(http::header::AUTHORIZATION).is_none());
        assert!(c.headers.get("x-goog-api-key").is_none());
        assert!(c.headers.get("cookie").is_none());
        assert!(c.headers.get(http::header::CONNECTION).is_none());
        assert!(c.headers.get(http::header::HOST).is_none());
        assert!(c.headers.get("via").is_none());
        assert!(c.headers.get("x-forwarded-for").is_none());
        assert!(c.headers.get(http::header::ACCEPT_ENCODING).is_none());
        // non-denied headers survive (the channel allow-list decides them later)
        assert_eq!(
            c.headers.get(http::header::CONTENT_TYPE).unwrap(),
            "application/json"
        );
        assert_eq!(c.headers.get("x-stainless-lang").unwrap(), "js");
        // ?key= dropped, other params survive for the channel allow-list
        assert_eq!(c.query.as_deref(), Some("alt=sse"));
    }

    #[test]
    fn normalizes_multipart_form_to_json() {
        let boundary = "----GProxyBoundary";
        let mut h = HeaderMap::new();
        h.insert(
            http::header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}")
                .parse()
                .unwrap(),
        );
        let mut c = ctx(h, None);
        c.path = "/v1/images/edits".into();
        c.body = Bytes::from(format!(
            "--{boundary}\r\n\
             Content-Disposition: form-data; name=\"model\"\r\n\r\n\
             gpt-image-1.5\r\n\
             --{boundary}\r\n\
             Content-Disposition: form-data; name=\"prompt\"\r\n\r\n\
             make it blue\r\n\
             --{boundary}\r\n\
             Content-Disposition: form-data; name=\"n\"\r\n\r\n\
             2\r\n\
             --{boundary}\r\n\
             Content-Disposition: form-data; name=\"stream\"\r\n\r\n\
             true\r\n\
             --{boundary}\r\n\
             Content-Disposition: form-data; name=\"image[]\"; filename=\"a.png\"\r\n\
             Content-Type: image/png\r\n\r\n\
             IMG1\r\n\
             --{boundary}\r\n\
             Content-Disposition: form-data; name=\"image[]\"; filename=\"b.jpg\"\r\n\
             Content-Type: image/jpeg\r\n\r\n\
             IMG2\r\n\
             --{boundary}\r\n\
             Content-Disposition: form-data; name=\"mask\"; filename=\"m.png\"\r\n\
             Content-Type: image/png\r\n\r\n\
             MASK\r\n\
             --{boundary}--\r\n"
        ));

        normalize_multipart_form_body(&mut c).unwrap();

        assert_eq!(
            c.headers.get(http::header::CONTENT_TYPE).unwrap(),
            "application/json"
        );
        let v: Value = serde_json::from_slice(&c.body).unwrap();
        assert_eq!(v["model"], "gpt-image-1.5");
        assert_eq!(v["prompt"], "make it blue");
        assert_eq!(v["n"], "2");
        assert_eq!(v["stream"], "true");
        assert_eq!(v["image"].as_array().unwrap().len(), 2);
        assert_eq!(
            v["image"][0],
            format!("data:image/png;base64,{}", B64.encode("IMG1"))
        );
        assert_eq!(
            v["image"][1],
            format!("data:image/jpeg;base64,{}", B64.encode("IMG2"))
        );
        assert_eq!(
            v["mask"],
            format!("data:image/png;base64,{}", B64.encode("MASK"))
        );
    }
}
