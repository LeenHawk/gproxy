//! Native LLMRawChatV2 request fields, model configuration and identity headers.
//! The host converts downstream dialects to OpenAI Chat before this boundary.
use super::{
    common::{insert, invalid, random_hex},
    config::{APP_ID, TraeConfig, VERSION_CODE, fact},
};
use crate::channel::{
    ChannelError, ChannelHeaders, CredentialView, HeaderAllowlist, PrepareContext, forwardable,
};
use crate::channels::shared::compatible::http::insert_configured;
use gproxy_protocol::{HttpBody, connection::Bytes};
use http::{HeaderMap, Method};
use serde_json::{Value, json};

pub const CLI_HEADERS: ChannelHeaders = ChannelHeaders {
    names: &[
        "x-conversation-id",
        "x-session-id",
        "x-app-id",
        "x-app-version",
        "x-ide-version",
        "x-ide-version-code",
        "x-app-version-code",
        "x-device-id",
        "x-machine-id",
        "x-device-type",
        "x-device-brand",
        "x-device-cpu",
        "x-os-version",
        "x-user-region",
        "x-uid",
        "x-request-id",
        "x-trae-request-id",
        "x-custom-trace-id",
        "x-flow-traceparent",
        "request-traffic-type",
        "x-ide-version-type",
    ],
    prefixes: &[],
};
const OWNED: &[&str] = &[
    "x-cloudide-token",
    "x-ide-token",
    "x-user-auth-token",
    "cookie",
    "content-encoding",
];

pub(super) fn headers(
    config: &TraeConfig,
    credential: &CredentialView<'_>,
) -> Result<HeaderMap, ChannelError> {
    let token = credential.secret["access_token"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or(ChannelError::InvalidCredential)?;
    let mut headers = HeaderMap::new();
    for (name, value) in &config.headers {
        insert_configured(&mut headers, name, value)?;
    }
    insert(&mut headers, "content-type", "application/json")?;
    insert(
        &mut headers,
        "authorization",
        &format!("Cloud-IDE-JWT {token}"),
    )?;
    insert(&mut headers, "x-cloudide-token", token)?;
    for (name, value) in [
        ("x-app-id", APP_ID),
        ("x-app-version", "default"),
        ("x-ide-version", config.version()),
        ("x-ide-version-code", VERSION_CODE),
        ("x-app-version-code", VERSION_CODE),
        ("x-ide-version-type", "stable"),
        ("request-traffic-type", "prod"),
        ("x-device-type", "Linux"),
    ] {
        insert(&mut headers, name, value)?;
    }
    for (key, name) in [
        ("device_id", "x-device-id"),
        ("machine_id", "x-machine-id"),
        ("user_id", "x-uid"),
    ] {
        insert(
            &mut headers,
            name,
            fact(credential, key).ok_or(ChannelError::InvalidCredential)?,
        )?;
    }
    if let Some(region) = fact(credential, "region") {
        insert(&mut headers, "x-user-region", region)?;
    }
    let trace = random_hex::<16>()?;
    insert(&mut headers, "x-custom-trace-id", &trace)?;
    insert(
        &mut headers,
        "x-flow-traceparent",
        &format!("04-{trace}-{}-01", random_hex::<8>()?),
    )?;
    Ok(headers)
}

pub(super) fn chat(
    ctx: PrepareContext<'_>,
    selected: &Value,
) -> Result<http::Request<HttpBody>, ChannelError> {
    let config = TraeConfig::from_view(ctx.provider)?;
    let allowlist = HeaderAllowlist::from_view_for(ctx.provider, CLI_HEADERS)?;
    let mut drop_headers = CLI_HEADERS.names.to_vec();
    drop_headers.extend_from_slice(OWNED);
    let mut outgoing = forwardable(&ctx.request.headers, allowlist.as_ref(), &drop_headers);
    outgoing.extend(headers(&config, &ctx.credential)?);
    insert(&mut outgoing, "accept", "text/event-stream")?;
    let request_id = random_hex::<16>()?;
    insert(&mut outgoing, "x-request-id", &request_id)?;
    insert(&mut outgoing, "x-trae-request-id", &request_id)?;
    let session = ["x-conversation-id", "x-session-id"]
        .iter()
        .find_map(|name| {
            if allowlist
                .as_ref()
                .is_some_and(|a| !a.allows(&http::HeaderName::from_static(name)))
            {
                return None;
            }
            ctx.request
                .headers
                .get(*name)
                .and_then(|v| v.to_str().ok())
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
        })
        .map(Ok)
        .unwrap_or_else(random_hex::<16>)?;
    let HttpBody::Bytes(bytes) = ctx.request.body else {
        return Err(ChannelError::InvalidConfig(
            "Trae requires a JSON request body".into(),
        ));
    };
    let mut input: Value =
        serde_json::from_slice(&bytes).map_err(|e| ChannelError::InvalidConfig(e.to_string()))?;
    let messages = input
        .get_mut("messages")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| ChannelError::InvalidConfig("Trae: messages must be an array".into()))?;
    for message in messages.iter_mut() {
        if message["role"] == "developer" {
            message["role"] = json!("system");
        }
        match message.get("content") {
            Some(Value::String(text)) => message["content"] = json!([{"type":"text","text":text}]),
            None | Some(Value::Null) => message["content"] = json!([]),
            Some(Value::Array(_)) => {}
            _ => {
                return Err(ChannelError::InvalidConfig(
                    "Trae: unsupported message content".into(),
                ));
            }
        }
    }
    let detail = selected["model_detail_list"]
        .as_array()
        .and_then(|a| a.first())
        .ok_or_else(|| invalid("model has no detail"))?;
    let model_name = detail["model_name"]
        .as_str()
        .ok_or_else(|| invalid("model detail has no model_name"))?;
    let mut body = json!({
        "config_name":selected["config_name"],"model_name":model_name,
        "messages":messages,"session_id":session,"conversation_id":session,
        "stream":true,"is_custom_model":false,"config_source":1,"raw_chat_function":"chat_v3",
    });
    if let Some(params) = detail.get("encrypted_model_params") {
        body["encrypted_model_params"] = params.clone();
    }
    for key in ["tools", "tool_choice", "reasoning_effort"] {
        if let Some(value) = input.get(key) {
            body[key] = value.clone();
        }
    }
    let url = ctx.endpoint_override.map(str::to_owned).unwrap_or_else(|| {
        format!(
            "{}/api/ide/v2/llm_raw_chat",
            config.api(ctx.provider, &ctx.credential)
        )
    });
    let mut request = http::Request::builder()
        .method(Method::POST)
        .uri(url)
        .body(HttpBody::Bytes(Bytes::from(body.to_string())))
        .map_err(invalid)?;
    *request.headers_mut() = outgoing;
    Ok(request)
}
