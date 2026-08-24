use gproxy_channel_api::{ChannelError, PrepareCtx, PreparedRequest};
use gproxy_protocol::{Operation, OperationKey, WireFamily};
use serde_json::Value;

struct Target {
    method: http::Method,
    path: &'static str,
    endpoint: &'static str,
}

pub(super) fn request(ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
    let target = target(ctx.key)?;
    let uri = endpoint(&ctx, &target)?;
    let body = super::model::rewrite(&ctx)?;
    let mut headers = http::HeaderMap::new();
    super::identity::apply(&mut headers, ctx.secret, &body)?;
    let mut request = http::Request::builder()
        .method(target.method)
        .uri(crate::shared::http::strip_userinfo(uri)?)
        .body(body)
        .map_err(|error| ChannelError::Prepare(error.to_string()))?;
    *request.headers_mut() = headers;
    Ok(PreparedRequest {
        request,
        framing: None,
        websocket: false,
        profile: Some(&super::profile::CLIENT_PROFILE),
    })
}

fn target(key: OperationKey) -> Result<Target, ChannelError> {
    if key == OperationKey::family(Operation::ListModels, WireFamily::OpenAi) {
        Ok(Target {
            method: http::Method::GET,
            path: "/models",
            endpoint: "openai_list_models",
        })
    } else if key.kind
        == gproxy_protocol::OperationKind::ContentGeneration(
            gproxy_protocol::ContentGenerationKind::OpenAiChat,
        )
    {
        Ok(Target {
            method: http::Method::POST,
            path: "/chat/completions",
            endpoint: "openai_chat_completions",
        })
    } else {
        Err(ChannelError::Prepare(
            "operation is unsupported by Copilot CLI".into(),
        ))
    }
}

fn endpoint(ctx: &PrepareCtx<'_>, target: &Target) -> Result<http::Uri, ChannelError> {
    if let Some(url) = ctx
        .provider_settings
        .get("endpoints")
        .and_then(|endpoints| endpoints.get(target.endpoint))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|url| !url.is_empty())
    {
        let model = crate::shared::http::encode_component(ctx.upstream_model.trim());
        return crate::shared::http::exact(&url.replace("{model}", &model), None);
    }
    let base = ctx
        .provider_settings
        .get("base_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|base| !base.is_empty())
        .unwrap_or_else(|| super::auth::account_base(ctx.secret));
    crate::shared::http::join(base, target.path, None)
}
