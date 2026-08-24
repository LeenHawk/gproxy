use bytes::Bytes;
use gproxy_channel_api::{ChannelError, PrepareCtx, PreparedRequest};
use gproxy_protocol::Operation;
use http::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderName, HeaderValue, USER_AGENT};
use serde_json::Value;
use sha2::{Digest, Sha256};

const AMZ_JSON: &str = "application/x-amz-json-1.0";
const TARGET_GENERATE: &str = "AmazonCodeWhispererStreamingService.GenerateAssistantResponse";
const TARGET_MODELS: &str = "AmazonCodeWhispererService.ListAvailableModels";
const UA_RUNTIME: &str = "aws-sdk-rust/1.3.15 ua/2.1 api/codewhispererstreaming/0.1.16551 os/linux lang/rust/1.92.0 md/appVersion-2.6.1 app/AmazonQ-For-CLI";
const UA_MANAGEMENT: &str = "aws-sdk-rust/1.3.15 ua/2.1 api/codewhispererruntime/0.1.16551 os/linux lang/rust/1.92.0 md/appVersion-2.6.1 app/AmazonQ-For-CLI";

pub(super) fn request(ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
    if ctx.key.operation == Operation::ListModels {
        return model_list(ctx);
    }
    runtime(ctx)
}

fn runtime(ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
    let token = super::auth::access_token(ctx.secret)?;
    let conversation = request_id(ctx.body, "conversation");
    let body = super::request::build(ctx.body, ctx.upstream_model, &conversation)?;
    let mut value: Value = serde_json::from_slice(&body)
        .map_err(|error| ChannelError::Prepare(format!("Kiro body JSON: {error}")))?;
    if value.get("profileArn").is_none()
        && let Some(profile) = super::auth::profile_arn(ctx.secret, ctx.provider_settings)
    {
        value["profileArn"] = Value::String(profile.into());
    }
    let body = serde_json::to_vec(&value)
        .map(Bytes::from)
        .map_err(json_error)?;
    let uri = if let Some(url) = super::endpoint::exact(
        ctx.provider_settings,
        "openai_responses",
        ctx.upstream_model,
    ) {
        crate::shared::http::exact(&url, None)?
    } else {
        crate::shared::http::join(&super::endpoint::runtime(ctx.provider_settings)?, "/", None)?
    };
    prepared(uri, body, token, TARGET_GENERATE, UA_RUNTIME, true)
}

fn model_list(ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
    let token = super::auth::access_token(ctx.secret)?;
    let profile = super::auth::profile_arn(ctx.secret, ctx.provider_settings)
        .ok_or_else(|| ChannelError::Secret("Kiro model list requires profile_arn".into()))?;
    let query = format!(
        "origin=KIRO_CLI&profileArn={}",
        crate::shared::http::encode_component(profile)
    );
    let uri = if let Some(url) = super::endpoint::exact(
        ctx.provider_settings,
        "openai_list_models",
        ctx.upstream_model,
    ) {
        crate::shared::http::exact(&url, Some(&query))?
    } else {
        crate::shared::http::join(
            &super::endpoint::management(ctx.provider_settings)?,
            "/",
            Some(&query),
        )?
    };
    let body = Bytes::from(
        serde_json::to_vec(&serde_json::json!({"origin":"KIRO_CLI","profileArn":profile}))
            .map_err(json_error)?,
    );
    prepared(uri, body, token, TARGET_MODELS, UA_MANAGEMENT, false)
}

fn prepared(
    uri: http::Uri,
    body: Bytes,
    token: &str,
    target: &'static str,
    user_agent: &'static str,
    streaming: bool,
) -> Result<PreparedRequest, ChannelError> {
    let mut headers = http::HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|error| ChannelError::Secret(format!("access_token is invalid: {error}")))?,
    );
    headers.insert(CONTENT_TYPE, HeaderValue::from_static(AMZ_JSON));
    headers.insert(ACCEPT, HeaderValue::from_static("*/*"));
    headers.insert(USER_AGENT, HeaderValue::from_static(user_agent));
    for (name, value) in [
        ("x-amz-user-agent", user_agent),
        ("x-amz-target", target),
        ("x-amzn-codewhisperer-optout", "false"),
        ("amz-sdk-request", "attempt=1; max=3"),
    ] {
        headers.insert(
            HeaderName::from_static(name),
            HeaderValue::from_static(value),
        );
    }
    let invocation = request_id(&body, "invocation");
    headers.insert(
        HeaderName::from_static("amz-sdk-invocation-id"),
        HeaderValue::from_str(&invocation)
            .map_err(|error| ChannelError::Prepare(format!("invocation id: {error}")))?,
    );
    let mut request = http::Request::post(crate::shared::http::strip_userinfo(uri)?)
        .body(body)
        .map_err(|error| ChannelError::Prepare(error.to_string()))?;
    *request.headers_mut() = headers;
    Ok(PreparedRequest {
        request,
        framing: streaming.then_some(gproxy_protocol::StreamFraming::Sse),
        websocket: false,
        profile: Some(&super::profile::CLIENT_PROFILE),
    })
}

fn request_id(seed: &[u8], label: &str) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before epoch")
        .as_nanos();
    let digest = Sha256::digest([seed, label.as_bytes(), &now.to_be_bytes()].concat());
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "{}-{}-{}-{}-{}",
        hex(&bytes[..4]),
        hex(&bytes[4..6]),
        hex(&bytes[6..8]),
        hex(&bytes[8..10]),
        hex(&bytes[10..])
    )
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn json_error(error: serde_json::Error) -> ChannelError {
    ChannelError::Prepare(format!("Kiro request JSON: {error}"))
}
