//! Bounded request/response helpers for the calls a channel makes outside
//! the operation path: quota probes, token refresh and device login. The
//! operation path never comes through here; it keeps its body lazy.

use crate::channel::ChannelError;
use futures_util::StreamExt;
use gproxy_protocol::HttpBody;
use gproxy_protocol::connection::Bytes;
use http::{HeaderMap, HeaderName, HeaderValue};

/// Ability replies are small documents; anything larger is a wrong endpoint.
const MAX_ABILITY_BODY: usize = 1024 * 1024;

pub(crate) fn invalid_response(message: impl Into<String>) -> ChannelError {
    ChannelError::InvalidResponse(message.into())
}

/// Read a response body into memory under `MAX_ABILITY_BODY`.
pub(crate) async fn read_body(body: HttpBody) -> Result<Bytes, ChannelError> {
    match body {
        HttpBody::Bytes(bytes) => Ok(bytes),
        HttpBody::Stream(mut stream) => {
            let mut out = Vec::new();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|error| invalid_response(error.to_string()))?;
                out.extend_from_slice(&chunk);
                if out.len() > MAX_ABILITY_BODY {
                    return Err(invalid_response("response exceeds the ability read limit"));
                }
            }
            Ok(Bytes::from(out))
        }
    }
}

/// A static header from provider configuration, named by the operator.
pub(crate) fn insert_configured(
    headers: &mut HeaderMap,
    name: &str,
    value: &str,
) -> Result<(), ChannelError> {
    headers.insert(
        HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| ChannelError::InvalidConfig(format!("header `{name}`")))?,
        HeaderValue::from_str(value)
            .map_err(|_| ChannelError::InvalidConfig(format!("header `{name}`")))?,
    );
    Ok(())
}

/// Remove `key`, `access_token` and `api_key` parameters, keeping the rest
/// byte for byte in their original order. Source authentication never
/// reaches an upstream, in the query string any more than in a header.
pub(crate) fn strip_query_auth(query: &str) -> String {
    query
        .split('&')
        .filter(|segment| {
            let name = segment.split('=').next().unwrap_or("");
            !matches!(name, "key" | "access_token" | "api_key")
        })
        .collect::<Vec<_>>()
        .join("&")
}
