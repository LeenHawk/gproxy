//! Pieces the vendor service implementations share: route classification,
//! local JSON answers, stable synthetic ids, bounded body reads and the
//! binding mechanics behind resource routes. Policy lives in each channel's
//! route table; this module only executes what a class asks for.

use crate::channel::{
    ChannelError, CredentialContext, ResourceBindingRecord, ServiceCaller, ServiceRoute,
};
use futures_util::StreamExt;
use gproxy_protocol::{HttpBody, WireResponse, connection::Bytes};
use http::{HeaderMap, HeaderValue, StatusCode, header};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Bodies the channel buffers itself (creation replies) are small.
const MAX_LOCAL_BODY: usize = 4 * 1024 * 1024;

/// The first declared route matching `method` and `path`, with its captured
/// parameters. Tables list specific routes before prefix families, so the
/// first match is the most specific one.
pub(crate) fn classify<'r, 'p>(
    routes: &'r [ServiceRoute],
    method: &http::Method,
    path: &'p str,
) -> Option<(&'r ServiceRoute, Vec<(&'static str, &'p str)>)> {
    routes
        .iter()
        .find_map(|route| route.matches(method, path).map(|params| (route, params)))
}

pub(crate) fn param<'p>(params: &[(&'static str, &'p str)], name: &str) -> Option<&'p str> {
    params
        .iter()
        .find_map(|(key, value)| (*key == name).then_some(*value))
}

pub(crate) fn json_response(status: StatusCode, value: &Value) -> WireResponse<HttpBody> {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    WireResponse {
        status,
        headers,
        body: HttpBody::Bytes(Bytes::from(value.to_string())),
    }
}

/// A vendor-shaped id that is stable for one (channel, kind, provider,
/// identity) tuple and reveals nothing about the account (v3 `stable_id`).
pub(crate) fn stable_id(channel: &str, kind: &str, provider_id: &str, identity: &str) -> String {
    let digest = Sha256::digest(format!("gproxy-{channel}-{kind}:{provider_id}:{identity}"));
    let mut suffix = String::with_capacity(24);
    for byte in &digest[..12] {
        use std::fmt::Write as _;
        let _ = write!(&mut suffix, "{byte:02x}");
    }
    format!("gproxy-{kind}-{suffix}")
}

pub(crate) fn unix_now_ms() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// Buffer a body the channel needs to look at, bounded.
pub(crate) async fn read_body(body: HttpBody) -> Result<Bytes, ChannelError> {
    match body {
        HttpBody::Bytes(bytes) => Ok(bytes),
        HttpBody::Stream(mut stream) => {
            let mut out = Vec::new();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|e| ChannelError::InvalidResponse(e.to_string()))?;
                out.extend_from_slice(&chunk);
                if out.len() > MAX_LOCAL_BODY {
                    return Err(ChannelError::InvalidResponse(
                        "service body exceeds the read limit".into(),
                    ));
                }
            }
            Ok(Bytes::from(out))
        }
    }
}

pub(crate) fn json_of(bytes: &[u8]) -> Option<Value> {
    serde_json::from_slice(bytes).ok()
}

/// Send a creation request and, when the vendor accepted it, bind the
/// created resource to the caller with the credential that created it.
/// `id_of` reads the vendor id from the reply body; `fallback_id` is the id
/// the path already named (installs). The reply is returned buffered.
pub(crate) async fn create_bound(
    caller: &dyn ServiceCaller,
    kind: &str,
    account: &CredentialContext<'_>,
    upstream: http::Request<HttpBody>,
    id_of: fn(&Value) -> Option<String>,
    fallback_id: Option<&str>,
) -> Result<WireResponse<HttpBody>, ChannelError> {
    let WireResponse {
        status,
        headers,
        body,
    } = account.client.send(upstream).await?;
    let bytes = read_body(body).await?;
    if status.is_success() {
        let summary = json_of(&bytes).unwrap_or(Value::Null);
        let upstream_id = id_of(&summary).or_else(|| fallback_id.map(str::to_owned));
        if let Some(upstream_id) = upstream_id {
            caller
                .save_binding(ResourceBindingRecord {
                    kind: kind.to_owned(),
                    upstream_id,
                    credential_id: account.credential.id.to_owned(),
                    summary,
                })
                .await?;
        }
    }
    Ok(WireResponse {
        status,
        headers,
        body: HttpBody::Bytes(bytes),
    })
}

/// Case-insensitive keyword filter over the text of a summary's named
/// fields, as the vendors' `search` routes do.
pub(crate) fn keyword_filter(
    items: Vec<Value>,
    keywords: &[String],
    fields: &[&str],
) -> Vec<Value> {
    if keywords.is_empty() {
        return items;
    }
    items
        .into_iter()
        .filter(|item| {
            let haystack = fields
                .iter()
                .filter_map(|field| item.get(field).and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join(" ")
                .to_ascii_lowercase();
            keywords.iter().any(|keyword| haystack.contains(keyword))
        })
        .collect()
}

/// Lower-cased `keywords` array of a search body.
pub(crate) fn keywords(body: Option<&Value>) -> Vec<String> {
    body.and_then(|value| value.get("keywords"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str().map(str::to_ascii_lowercase))
        .collect()
}
