use bytes::Bytes;
use gproxy_channel_api::{
    ChannelError, CredentialId, SurfaceBody, SurfaceReply, SurfaceRequest, SurfaceServices,
};
use http::{HeaderMap, HeaderValue, Method, StatusCode};
use serde_json::{Value, json};

pub(super) const FILE_KIND: &str = "claude:file";
pub(super) const SKILL_KIND: &str = "claude:skill";
pub(super) const FILES_BETA: &str = "files-api-2025-04-14";
pub(super) const SKILLS_BETA: &str = "skills-2025-10-02";

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

pub(super) fn param<'a>(
    params: &'a [(&'static str, String)],
    name: &str,
) -> Result<&'a str, ChannelError> {
    params
        .iter()
        .find_map(|(candidate, value)| (*candidate == name).then_some(value.as_str()))
        .ok_or_else(|| ChannelError::Prepare(format!("surface parameter `{name}` is missing")))
}

pub(super) fn resource_headers(input: &HeaderMap, beta: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    for name in [http::header::CONTENT_TYPE, http::header::ACCEPT] {
        if let Some(value) = input.get(&name) {
            headers.insert(name, value.clone());
        }
    }
    let existing = input
        .get("anthropic-beta")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let mut tokens = existing
        .split(',')
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if !tokens.contains(&beta) {
        tokens.push(beta);
    }
    let combined = tokens.join(",");
    headers.insert(
        "anthropic-beta",
        HeaderValue::from_str(&combined).expect("beta tokens came from a valid header"),
    );
    headers
}

pub(super) fn safe_query(query: Option<&str>) -> Option<String> {
    let kept = query?
        .split('&')
        .filter(|part| decode_component(part.split('=').next().unwrap_or_default()) != "key")
        .collect::<Vec<_>>();
    (!kept.is_empty()).then(|| kept.join("&"))
}

pub(super) fn skills_query(query: Option<&str>) -> String {
    let safe = safe_query(query);
    if safe
        .as_deref()
        .is_some_and(|query| query.split('&').any(|part| part == "beta=true"))
    {
        safe.unwrap_or_default()
    } else if let Some(query) = safe.filter(|query| !query.is_empty()) {
        format!("beta=true&{query}")
    } else {
        "beta=true".to_owned()
    }
}

pub(super) fn request(
    label: &'static str,
    method: Method,
    path: String,
    query: Option<String>,
    headers: HeaderMap,
    body: Bytes,
    credential: CredentialId,
) -> SurfaceRequest {
    SurfaceRequest {
        label,
        key: None,
        stream: false,
        method,
        upstream_path: path,
        query,
        headers,
        body,
        credential: Some(credential),
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
            "resource mutation returned a streaming body".into(),
        ));
    };
    serde_json::from_slice(body)
        .map_err(|error| ChannelError::Decode(format!("resource response JSON: {error}")))
}

pub(super) async fn save_resource(
    services: &SurfaceServices<'_>,
    kind: &'static str,
    id: &str,
    credential: CredentialId,
    resource: Value,
) -> Result<(), ChannelError> {
    services
        .bindings
        .save(
            services.provider.id,
            services.identity.user_id,
            kind,
            id,
            credential,
            json!({ "resource": resource }),
        )
        .await
        .map_err(|error| ChannelError::Prepare(error.to_string()))
}

pub(super) async fn delete_resource(
    services: &SurfaceServices<'_>,
    kind: &'static str,
    id: &str,
) -> Result<(), ChannelError> {
    services
        .bindings
        .delete(services.provider.id, services.identity.user_id, kind, id)
        .await
        .map_err(|error| ChannelError::Prepare(error.to_string()))
}

pub(super) fn decode_component(value: &str) -> String {
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
