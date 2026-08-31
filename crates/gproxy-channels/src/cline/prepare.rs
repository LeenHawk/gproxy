use gproxy_channel_api::{ChannelError, PrepareCtx, PreparedRequest};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, OperationKind, WireFamily};
use http::header::{CONTENT_TYPE, HeaderValue};
use serde_json::Value;

const DEFAULT_BASE_URL: &str = "https://api.cline.bot/api/v1";

struct Target {
    method: http::Method,
    path: &'static str,
    endpoint: &'static str,
}

pub(super) fn request(ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
    let target = target(ctx.key)?;
    let uri = endpoint(&ctx, &target)?;
    let body = super::model::rewrite(&ctx)?;
    let mut headers = crate::policy::request_headers(crate::policy::CLINE, &ctx)?;
    super::auth::apply(&mut headers, ctx.secret)?;
    if !body.is_empty() {
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    }
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
        profile: None,
    })
}

pub(super) fn base_url(settings: &Value) -> &str {
    settings
        .get("base_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|base| !base.is_empty())
        .unwrap_or(DEFAULT_BASE_URL)
        .trim_end_matches('/')
}

fn target(key: OperationKey) -> Result<Target, ChannelError> {
    if key == OperationKey::family(Operation::ListModels, WireFamily::OpenAi) {
        Ok(Target {
            method: http::Method::GET,
            path: "/ai/cline/recommended-models",
            endpoint: "openai_list_models",
        })
    } else if key.kind == OperationKind::ContentGeneration(ContentGenerationKind::OpenAiChat) {
        Ok(Target {
            method: http::Method::POST,
            path: "/chat/completions",
            endpoint: "openai_chat_completions",
        })
    } else {
        Err(ChannelError::Prepare(
            "operation is unsupported by Cline".into(),
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
    crate::shared::http::join(base_url(ctx.provider_settings), target.path, None)
}
