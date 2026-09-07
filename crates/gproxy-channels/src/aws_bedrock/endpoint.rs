use gproxy_channel_api::{ChannelError, PrepareCtx};
use gproxy_protocol::{Operation, StreamFraming};
use http::Method;
use serde_json::Value;

const DEFAULT_REGION: &str = "us-east-1";
pub(super) struct Target {
    pub method: Method,
    pub uri: http::Uri,
    pub framing: Option<StreamFraming>,
    pub compact: bool,
}

pub(super) fn resolve(ctx: &PrepareCtx<'_>) -> Result<Target, ChannelError> {
    let region = region(ctx.provider_settings)?;
    let compact = super::shape::is_compact(ctx.body);
    let (method, path, endpoint, control, framing) = match ctx.key.operation() {
        Operation::ListModels => (
            Method::GET,
            "/foundation-models".into(),
            "openai_list_models",
            true,
            None,
        ),
        Operation::GetModel => (
            Method::GET,
            format!("/foundation-models/{}", model(ctx.upstream_model)?),
            "openai_get_model",
            true,
            None,
        ),
        Operation::CountTokens => (
            Method::POST,
            format!("/model/{}/count-tokens", model(ctx.upstream_model)?),
            "claude_count_tokens",
            false,
            None,
        ),
        Operation::GenerateContent | Operation::StreamGenerateContent if compact => (
            Method::POST,
            format!("/model/{}/invoke", model(ctx.upstream_model)?),
            "openai_compact",
            false,
            None,
        ),
        Operation::GenerateContent | Operation::StreamGenerateContent
            if super::messages::enabled(ctx) =>
        {
            (
                Method::POST,
                format!(
                    "/model/{}/{}",
                    model(ctx.upstream_model)?,
                    if ctx.stream {
                        "invoke-with-response-stream"
                    } else {
                        "invoke"
                    }
                ),
                "claude_messages",
                false,
                ctx.stream.then_some(StreamFraming::Sse),
            )
        }
        Operation::GenerateContent => (
            Method::POST,
            format!("/model/{}/converse", model(ctx.upstream_model)?),
            "claude_messages",
            false,
            None,
        ),
        Operation::StreamGenerateContent => (
            Method::POST,
            format!("/model/{}/converse-stream", model(ctx.upstream_model)?),
            "claude_messages",
            false,
            Some(StreamFraming::Sse),
        ),
        Operation::CreateVideo => (
            Method::POST,
            "/async-invoke".into(),
            "openai_video_create",
            false,
            None,
        ),
        Operation::RetrieveVideo => {
            let arn = super::resource::request_arn(ctx.path)?;
            (
                Method::GET,
                format!(
                    "/async-invoke/{}",
                    crate::shared::http::encode_component(&arn)
                ),
                "openai_video_retrieve",
                false,
                None,
            )
        }
        _ => {
            return Err(ChannelError::Prepare(
                "operation is unsupported by AWS Bedrock".into(),
            ));
        }
    };
    let query = if ctx.key.operation() == Operation::ListModels {
        crate::policy::request_query(crate::policy::AWS_BEDROCK, ctx)?
    } else {
        None
    };
    let uri = if let Some(url) = endpoint_override(ctx, endpoint) {
        crate::shared::http::exact(&url, query.as_deref())?
    } else {
        let configured = string(ctx.provider_settings, "base_url");
        let generated = if control {
            format!("https://bedrock.{region}.amazonaws.com")
        } else {
            format!("https://bedrock-runtime.{region}.amazonaws.com")
        };
        crate::shared::http::join(configured.unwrap_or(&generated), &path, query.as_deref())?
    };
    Ok(Target {
        method,
        uri,
        framing,
        compact,
    })
}

pub(super) fn region(settings: &Value) -> Result<&str, ChannelError> {
    let region = string(settings, "region").unwrap_or(DEFAULT_REGION);
    if region
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        Ok(region)
    } else {
        Err(ChannelError::Prepare("invalid AWS region".into()))
    }
}

fn model(model: &str) -> Result<String, ChannelError> {
    let model = model.trim();
    if model.is_empty() {
        Err(ChannelError::Prepare(
            "AWS Bedrock request has no model".into(),
        ))
    } else {
        Ok(crate::shared::http::encode_component(model))
    }
}

fn endpoint_override(ctx: &PrepareCtx<'_>, name: &str) -> Option<String> {
    let model = crate::shared::http::encode_component(ctx.upstream_model);
    ctx.provider_settings
        .get("endpoints")?
        .get(name)?
        .as_str()
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(|url| url.replace("{model}", &model))
}

fn string<'a>(value: &'a Value, name: &str) -> Option<&'a str> {
    value
        .get(name)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}
