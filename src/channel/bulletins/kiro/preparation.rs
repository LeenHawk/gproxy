//! Kiro runtime endpoint and Smithy request preparation.

use bytes::Bytes;
use http::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderName, HeaderValue, USER_AGENT};
use serde_json::Value;

use crate::channel::http_util::{allow_headers, build_request, join_url};
use crate::channel::{ChannelError, PrepareCtx, PreparedRequest};

use super::{AMZ_JSON, auth, model_list, request};

const DEFAULT_REGION: &str = "us-east-1";
const TARGET_GENERATE: &str = "AmazonCodeWhispererStreamingService.GenerateAssistantResponse";
const USER_AGENT_VALUE: &str = "aws-sdk-rust/1.3.15 ua/2.1 api/codewhispererstreaming/0.1.16551 os/linux lang/rust/1.92.0 md/appVersion-2.6.1 app/AmazonQ-For-CLI";

/// The Kiro region from settings (default `us-east-1`).
pub(in crate::channel::bulletins) fn region(settings: &Value) -> String {
    settings
        .get("region")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_REGION)
        .to_string()
}

/// The management host, or a `settings.management_url` override.
pub(in crate::channel::bulletins) fn management_base(settings: &Value) -> String {
    if let Some(url) = settings
        .get("management_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return url.to_string();
    }
    format!("https://management.{}.kiro.dev", super::region(settings))
}

fn runtime_base(settings: &Value) -> String {
    for key in ["runtime_url", "base_url"] {
        if let Some(url) = settings
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return url.to_string();
        }
    }
    format!("https://runtime.{}.kiro.dev", region(settings))
}

pub(super) fn prepare(ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
    if model_list::is_model_list(&ctx.method, ctx.path) {
        let req = model_list::request(ctx.secret, ctx.provider_settings)?;
        return Ok(PreparedRequest::new(req));
    }

    let access_token = auth::access_token(ctx.secret)?.to_string();
    let profile_arn = auth::profile_arn(ctx.secret, ctx.provider_settings).map(str::to_owned);
    let base = runtime_base(ctx.provider_settings);
    let body = request::build_request_body(&ctx.body, ctx.upstream_model_id, &gen_uuid())?;
    let body = with_profile_arn(body, profile_arn.as_deref())?;

    let uri = match crate::channel::settings::endpoint_url(
        ctx.provider_settings,
        ctx.op,
        ctx.stream,
        ctx.upstream_model_id,
    ) {
        Some(url) => crate::channel::http_util::exact_url(&url, None)?,
        None => join_url(&base, "/", None)?,
    };
    let headers = allow_headers(ctx.headers, &[]);
    let mut req = build_request(ctx.method, uri, headers, Bytes::from(body))?;
    apply_headers(&mut req, &access_token, TARGET_GENERATE)?;
    Ok(PreparedRequest::new(req))
}

fn with_profile_arn(body: Vec<u8>, profile_arn: Option<&str>) -> Result<Vec<u8>, ChannelError> {
    let Some(arn) = profile_arn else {
        return Ok(body);
    };
    let mut value: Value = serde_json::from_slice(&body)
        .map_err(|e| ChannelError::Build(format!("kiro request body re-parse: {e}")))?;
    if value.get("profileArn").is_none()
        && let Some(obj) = value.as_object_mut()
    {
        obj.insert("profileArn".into(), Value::String(arn.to_string()));
    }
    serde_json::to_vec(&value)
        .map_err(|e| ChannelError::Build(format!("kiro request body re-serialize: {e}")))
}

fn apply_headers(
    req: &mut http::Request<Bytes>,
    access_token: &str,
    target: &'static str,
) -> Result<(), ChannelError> {
    let bearer = HeaderValue::from_str(&format!("Bearer {access_token}"))
        .map_err(|e| ChannelError::InvalidCredential(format!("bad access_token: {e}")))?;
    let invocation_id = HeaderValue::from_str(&gen_uuid())
        .map_err(|e| ChannelError::Build(format!("bad invocation id: {e}")))?;

    let headers = req.headers_mut();
    headers.insert(AUTHORIZATION, bearer);
    headers.insert(CONTENT_TYPE, HeaderValue::from_static(AMZ_JSON));
    headers.insert(ACCEPT, HeaderValue::from_static("*/*"));
    headers.insert(USER_AGENT, HeaderValue::from_static(USER_AGENT_VALUE));
    headers.insert(
        HeaderName::from_static("x-amz-user-agent"),
        HeaderValue::from_static(USER_AGENT_VALUE),
    );
    headers.insert(
        HeaderName::from_static("x-amz-target"),
        HeaderValue::from_static(target),
    );
    headers.insert(
        HeaderName::from_static("x-amzn-codewhisperer-optout"),
        HeaderValue::from_static("false"),
    );
    headers.insert(
        HeaderName::from_static("amz-sdk-request"),
        HeaderValue::from_static("attempt=1; max=3"),
    );
    headers.insert(
        HeaderName::from_static("amz-sdk-invocation-id"),
        invocation_id,
    );
    Ok(())
}

fn gen_uuid() -> String {
    crate::util::rand::uuid_v4()
}
