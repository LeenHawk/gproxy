use crate::channel::http_util::{exact_url, join_url};
use crate::channel::settings::{endpoint_by_key, endpoint_key};
use crate::channel::{ChannelError, PrepareCtx};
use crate::protocol::Operation;

use super::{DEFAULT_REGION, is_count_tokens};

pub(super) fn resolve(ctx: &PrepareCtx<'_>, compact: bool) -> Result<http::Uri, ChannelError> {
    if matches!(
        ctx.op.operation(),
        Operation::CreateVideo | Operation::RetrieveVideo
    ) {
        return resolve_video(ctx);
    }
    let model = crate::channel::oauth::percent_encode(ctx.upstream_model_id);
    let (control, path) = if ctx.op.operation() == Operation::ListModels {
        (true, "/foundation-models".to_owned())
    } else if ctx.op.operation() == Operation::GetModel {
        require_model(ctx)?;
        (true, format!("/foundation-models/{model}"))
    } else {
        require_model(ctx)?;
        let suffix = if is_count_tokens(ctx.op) {
            "count-tokens"
        } else if compact {
            "invoke"
        } else if ctx.stream {
            "converse-stream"
        } else {
            "converse"
        };
        (false, format!("/model/{model}/{suffix}"))
    };
    let endpoint = if compact {
        "openai_compact"
    } else {
        endpoint_key(ctx.op, ctx.stream)
    };
    if let Some(url) = endpoint_by_key(ctx.provider_settings, endpoint, &model) {
        return exact_url(&url, None);
    }
    let key = if control {
        "control_base_url"
    } else {
        "base_url"
    };
    let configured = ctx
        .provider_settings
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|base| !base.is_empty());
    let region = region(ctx.provider_settings)?;
    let generated = if control {
        format!("https://bedrock.{region}.amazonaws.com")
    } else {
        format!("https://bedrock-runtime.{region}.amazonaws.com")
    };
    join_url(configured.unwrap_or(&generated), &path, None)
}

fn resolve_video(ctx: &PrepareCtx<'_>) -> Result<http::Uri, ChannelError> {
    if let Some(url) = crate::channel::settings::endpoint_url_for_request(
        ctx.provider_settings,
        ctx.op,
        ctx.stream,
        ctx.upstream_model_id,
        ctx.path,
    ) {
        return exact_url(&url, None);
    }
    let path = if ctx.op.operation() == Operation::CreateVideo {
        "/async-invoke".to_owned()
    } else {
        let id = ctx
            .path
            .strip_prefix("/v1/videos/")
            .filter(|id| !id.is_empty() && !id.contains('/'))
            .ok_or_else(|| ChannelError::Build("invalid Bedrock video id".into()))?;
        let arn = crate::channel::bulletins::common::decode_video_task_id(id)?;
        format!(
            "/async-invoke/{}",
            crate::channel::oauth::percent_encode(&arn)
        )
    };
    let configured = ctx
        .provider_settings
        .get("base_url")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|base| !base.is_empty());
    let region = region(ctx.provider_settings)?;
    let generated = format!("https://bedrock-runtime.{region}.amazonaws.com");
    join_url(configured.unwrap_or(&generated), &path, None)
}

fn require_model(ctx: &PrepareCtx<'_>) -> Result<(), ChannelError> {
    if ctx.upstream_model_id.trim().is_empty() {
        Err(ChannelError::Build(
            "AWS Bedrock requires an upstream model id".into(),
        ))
    } else {
        Ok(())
    }
}

fn region(settings: &serde_json::Value) -> Result<&str, ChannelError> {
    let region = settings
        .get("region")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|region| !region.is_empty())
        .unwrap_or(DEFAULT_REGION);
    if region
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        Ok(region)
    } else {
        Err(ChannelError::Build("invalid AWS region".into()))
    }
}
