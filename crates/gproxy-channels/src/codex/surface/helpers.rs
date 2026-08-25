use bytes::Bytes;
use gproxy_channel_api::{
    ChannelError, CredentialId, SurfaceBody, SurfaceReply, SurfaceRequest, SurfaceServices,
};
use http::{HeaderMap, HeaderValue, Method, StatusCode};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub(super) const TASK_KIND: &str = "codex:task";
pub(super) const FILE_KIND: &str = "codex:file";

pub(super) fn json_reply(status: StatusCode, value: Value) -> SurfaceReply {
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    SurfaceReply {
        status,
        headers,
        body: SurfaceBody::Full(Bytes::from(
            serde_json::to_vec(&value).expect("JSON Value serializes"),
        )),
    }
}

pub(super) fn request(
    label: &'static str,
    method: Method,
    path: String,
    query: Option<&str>,
    headers: &HeaderMap,
    body: Bytes,
    credential: Option<CredentialId>,
) -> SurfaceRequest {
    SurfaceRequest {
        label,
        key: None,
        stream: false,
        method,
        upstream_path: path,
        query: safe_query(query),
        headers: forwarded_headers(headers),
        body,
        credential,
    }
}

pub(super) async fn invoke(
    services: &SurfaceServices<'_>,
    request: SurfaceRequest,
) -> Result<SurfaceReply, ChannelError> {
    services
        .invoke
        .ok_or_else(|| ChannelError::Prepare("surface has no upstream capability".into()))?
        .invoke(request)
        .await
        .map_err(|error| ChannelError::Prepare(error.to_string()))
}

pub(super) fn reply_json(reply: &SurfaceReply) -> Result<Value, ChannelError> {
    let SurfaceBody::Full(body) = &reply.body else {
        return Err(ChannelError::Decode(
            "control operation returned a streaming body".into(),
        ));
    };
    serde_json::from_slice(body)
        .map_err(|error| ChannelError::Decode(format!("control response JSON: {error}")))
}

pub(super) fn param<'a>(
    params: &'a [(&'static str, String)],
    name: &str,
) -> Result<&'a str, ChannelError> {
    params
        .iter()
        .find_map(|(candidate, value)| (*candidate == name).then_some(value.as_str()))
        .ok_or_else(|| ChannelError::Prepare(format!("surface parameter `{name}` is missing")))
}

pub(super) fn canonical_path(path: &str) -> String {
    for prefix in [
        "/backend-api/wham/",
        "/backend-api/codex/",
        "/backend-api/",
        "/codex/",
    ] {
        if let Some(rest) = path.strip_prefix(prefix) {
            return format!("/api/codex/{rest}");
        }
    }
    path.strip_prefix("/ps/")
        .map(|rest| format!("/api/codex/ps/{rest}"))
        .unwrap_or_else(|| path.to_owned())
}

pub(super) fn forwarded_headers(input: &HeaderMap) -> HeaderMap {
    let mut output = HeaderMap::new();
    for name in [
        "accept",
        "content-type",
        "cache-control",
        "mcp-session-id",
        "last-event-id",
        "x-codex-turn-metadata",
        "x-codex-installation-id",
        "x-client-request-id",
        "x-codex-server-id",
    ] {
        if let Some(value) = input.get(name) {
            output.insert(http::HeaderName::from_static(name), value.clone());
        }
    }
    for (name, value) in input {
        if name.as_str().starts_with("x-codex-") {
            output.insert(name.clone(), value.clone());
        }
    }
    output
}

pub(super) fn safe_query(query: Option<&str>) -> Option<String> {
    let kept = query?
        .split('&')
        .filter(|part| {
            !part.is_empty()
                && !matches!(
                    decode_component(part.split('=').next().unwrap_or_default()).as_str(),
                    "key" | "api_key" | "x-api-key"
                )
        })
        .collect::<Vec<_>>();
    (!kept.is_empty()).then(|| kept.join("&"))
}

pub(super) fn query_pairs(query: Option<&str>) -> Vec<(String, String)> {
    query
        .into_iter()
        .flat_map(|query| query.split('&'))
        .map(|part| {
            let (key, value) = part.split_once('=').unwrap_or((part, ""));
            (decode_component(key), decode_component(value))
        })
        .collect()
}

pub(super) fn query_value<'a>(pairs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    pairs
        .iter()
        .find_map(|(key, value)| (key == name).then_some(value.as_str()))
}

pub(super) async fn save_binding(
    services: &SurfaceServices<'_>,
    kind: &'static str,
    id: &str,
    credential: CredentialId,
    summary: Value,
) -> Result<(), ChannelError> {
    services
        .bindings
        .save(
            services.provider.id,
            services.identity.user_id,
            kind,
            id,
            credential,
            summary,
        )
        .await
        .map_err(|error| ChannelError::Prepare(error.to_string()))
}

pub(super) async fn find_binding(
    services: &SurfaceServices<'_>,
    kind: &'static str,
    id: &str,
) -> Result<gproxy_channel_api::Binding, ChannelError> {
    services
        .bindings
        .find(services.provider.id, services.identity.user_id, kind, id)
        .await
        .map_err(|error| ChannelError::Prepare(error.to_string()))?
        .ok_or_else(|| ChannelError::Prepare("bound resource not found".into()))
}

pub(super) fn plan_type(settings: &Value) -> &str {
    settings
        .get("codex_pat_plan_type")
        .and_then(Value::as_str)
        .filter(|value| {
            matches!(
                *value,
                "free" | "go" | "plus" | "pro" | "team" | "business" | "enterprise" | "edu"
            )
        })
        .unwrap_or("pro")
}

pub(super) fn stable_id(kind: &str, provider_id: i64, user_id: i64) -> String {
    let digest = Sha256::digest(format!("gproxy-codex-{kind}:{provider_id}:{user_id}"));
    let suffix = digest[..12]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("gproxy-{kind}-{suffix}")
}

pub(super) fn user_name(services: &SurfaceServices<'_>) -> String {
    format!("user-{}", services.identity.user_id)
}

pub(super) fn unix_now() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .expect("system clock is before the Unix epoch")
        .as_secs()
        .try_into()
        .expect("Unix seconds fit in i64")
}

pub(super) fn transport_reply(error: impl std::fmt::Display) -> ChannelError {
    ChannelError::Prepare(error.to_string())
}

pub(super) fn empty_object() -> Bytes {
    Bytes::from_static(br#"{}"#)
}

pub(super) fn file_deleted(id: &str) -> Value {
    json!({"id":id, "object":"file", "deleted":true})
}

fn decode_component(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => output.push(b' '),
            b'%' if index + 2 < bytes.len() => {
                if let (Some(high), Some(low)) = (hex(bytes[index + 1]), hex(bytes[index + 2])) {
                    output.push(high * 16 + low);
                    index += 2;
                } else {
                    output.push(bytes[index]);
                }
            }
            byte => output.push(byte),
        }
        index += 1;
    }
    String::from_utf8_lossy(&output).into_owned()
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
