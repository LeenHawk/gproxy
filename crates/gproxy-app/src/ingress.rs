use std::io::Read as _;

use bytes::Bytes;
use gproxy_core::RoutingMode;
use http::HeaderMap;

use crate::AppHandle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodeError {
    pub status: http::StatusCode,
    pub message: &'static str,
}

pub fn decode_body(
    headers: &mut HeaderMap,
    body: Bytes,
    max_bytes: usize,
) -> Result<Bytes, DecodeError> {
    let Some(encoding) = headers.get(http::header::CONTENT_ENCODING) else {
        return Ok(body);
    };
    let encoding = encoding.to_str().map_err(|_| unsupported_encoding())?;
    if encoding.eq_ignore_ascii_case("identity") {
        headers.remove(http::header::CONTENT_ENCODING);
        return Ok(body);
    }
    if !encoding.eq_ignore_ascii_case("zstd") {
        return Err(unsupported_encoding());
    }
    let decoder = ruzstd::decoding::StreamingDecoder::new(std::io::Cursor::new(body))
        .map_err(|_| invalid_encoding())?;
    let mut decoded = Vec::with_capacity(max_bytes.min(64 * 1024));
    decoder
        .take(max_bytes.saturating_add(1) as u64)
        .read_to_end(&mut decoded)
        .map_err(|_| invalid_encoding())?;
    if decoded.len() > max_bytes {
        return Err(DecodeError {
            status: http::StatusCode::PAYLOAD_TOO_LARGE,
            message: "decoded request body too large",
        });
    }
    headers.remove(http::header::CONTENT_ENCODING);
    headers.remove(http::header::CONTENT_LENGTH);
    Ok(Bytes::from(decoded))
}

fn unsupported_encoding() -> DecodeError {
    DecodeError {
        status: http::StatusCode::UNSUPPORTED_MEDIA_TYPE,
        message: "unsupported content encoding",
    }
}

fn invalid_encoding() -> DecodeError {
    DecodeError {
        status: http::StatusCode::BAD_REQUEST,
        message: "invalid zstd request body",
    }
}

pub fn normalize_path(
    app: &AppHandle,
    method: &http::Method,
    path: &str,
    upgrade: bool,
) -> (RoutingMode, String) {
    normalize_path_with(
        path,
        |name| app.inner.host.services.control.has_named_target(name),
        |candidate| app.inner.core.matches_ingress(method, candidate, upgrade),
    )
}

fn normalize_path_with(
    path: &str,
    has_named_target: impl FnOnce(&str) -> bool,
    matches_ingress: impl FnOnce(&str) -> bool,
) -> (RoutingMode, String) {
    let Some((name, remainder)) = path.strip_prefix('/').and_then(|path| path.split_once('/'))
    else {
        return (RoutingMode::Aggregated, path.to_owned());
    };
    let remainder = format!("/{remainder}");
    if name.is_empty() || !has_named_target(name) || !matches_ingress(&remainder) {
        return (RoutingMode::Aggregated, path.to_owned());
    }
    (
        RoutingMode::Named {
            name: name.to_owned(),
        },
        remainder,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruzstd::encoding::{CompressionLevel, compress_to_vec};

    #[test]
    fn decodes_zstd_and_removes_framing_headers() {
        let original = br#"{"model":"gpt-test"}"#;
        let compressed = compress_to_vec(original.as_slice(), CompressionLevel::Fastest);
        let mut headers = HeaderMap::new();
        headers.insert(http::header::CONTENT_ENCODING, "zstd".parse().unwrap());
        headers.insert(
            http::header::CONTENT_LENGTH,
            compressed.len().to_string().parse().unwrap(),
        );

        let decoded = decode_body(&mut headers, Bytes::from(compressed), 1024).unwrap();

        assert_eq!(decoded.as_ref(), original);
        assert!(!headers.contains_key(http::header::CONTENT_ENCODING));
        assert!(!headers.contains_key(http::header::CONTENT_LENGTH));
    }

    #[test]
    fn decoded_limit_and_unknown_encoding_fail_at_ingress() {
        let compressed = compress_to_vec([0_u8; 32].as_slice(), CompressionLevel::Fastest);
        let mut headers = HeaderMap::new();
        headers.insert(http::header::CONTENT_ENCODING, "zstd".parse().unwrap());
        assert_eq!(
            decode_body(&mut headers, Bytes::from(compressed), 16)
                .unwrap_err()
                .status,
            http::StatusCode::PAYLOAD_TOO_LARGE
        );

        headers.insert(http::header::CONTENT_ENCODING, "br".parse().unwrap());
        assert_eq!(
            decode_body(&mut headers, Bytes::new(), 16)
                .unwrap_err()
                .status,
            http::StatusCode::UNSUPPORTED_MEDIA_TYPE
        );
    }

    #[test]
    fn named_prefix_accepts_every_declared_remainder() {
        for (path, remainder) in [
            ("/codex/v1/responses", "/v1/responses"),
            (
                "/codex/backend-api/codex/responses",
                "/backend-api/codex/responses",
            ),
            ("/codex/backend-api/wham/usage", "/backend-api/wham/usage"),
            ("/codex/oauth/token", "/oauth/token"),
        ] {
            assert_eq!(
                normalize_path_with(path, |name| name == "codex", |value| value == remainder),
                (
                    RoutingMode::Named {
                        name: "codex".into()
                    },
                    remainder.into()
                )
            );
        }
    }

    #[test]
    fn unknown_prefix_or_remainder_stays_aggregated() {
        for path in [
            "/backend-api/codex/responses",
            "/missing/v1/responses",
            "/codex/not-a-surface",
        ] {
            assert_eq!(
                normalize_path_with(
                    path,
                    |name| name == "codex",
                    |value| { value == "/v1/responses" }
                ),
                (RoutingMode::Aggregated, path.into())
            );
        }
    }
}
