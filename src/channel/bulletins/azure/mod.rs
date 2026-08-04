//! Microsoft Foundry / Azure OpenAI channel.
//!
//! OpenAI v1 APIs live under `/openai/v1`, Claude APIs under `/anthropic/v1`,
//! while image APIs still use deployment-bound `/openai/deployments/...` paths.

mod auth;
mod shape;

use bytes::Bytes;
use http::HeaderMap;

use crate::channel::bulletins::common::{self, ApiKeyDefaults};
use crate::channel::http_util::{allow_headers, allow_query, build_request};
use crate::channel::{Channel, ChannelError, PrepareCtx, PreparedRequest, ShapeCtx};
use crate::protocol::{ContentGenerationKind, Operation, OperationKind, Provider};

const DEFAULTS: ApiKeyDefaults = ApiKeyDefaults {
    default_base_url: None,
    forward_headers: &["anthropic-beta", "openai-beta"],
    forward_query: &["api-version", "azure-beta"],
};
const DEFAULT_IMAGE_API_VERSION: &str = "2025-04-01-preview";

fn is_anthropic(op: crate::protocol::OperationKey) -> bool {
    op.kind() == OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages)
        || op.kind() == OperationKind::Provider(Provider::Claude)
}

fn upstream_path(ctx: &PrepareCtx<'_>) -> String {
    match ctx.op.operation() {
        Operation::CreateImage => format!(
            "/openai/deployments/{}/images/generations",
            crate::channel::oauth::percent_encode(ctx.upstream_model_id)
        ),
        Operation::EditImage => format!(
            "/openai/deployments/{}/images/edits",
            crate::channel::oauth::percent_encode(ctx.upstream_model_id)
        ),
        _ if is_anthropic(ctx.op) => format!("/anthropic{}", ctx.path),
        _ => format!("/openai{}", ctx.path),
    }
}

fn query(ctx: &PrepareCtx<'_>) -> Option<String> {
    let mut query = allow_query(ctx.query, DEFAULTS.forward_query);
    let endpoint_query = crate::channel::settings::endpoint_url(
        ctx.provider_settings,
        ctx.op,
        ctx.stream,
        ctx.upstream_model_id,
    )
    .and_then(|url| url.split_once('?').map(|(_, query)| query.to_owned()));
    if !matches!(
        ctx.op.operation(),
        Operation::CreateImage | Operation::EditImage
    ) || query.as_deref().is_some_and(has_api_version)
        || endpoint_query.as_deref().is_some_and(has_api_version)
    {
        return query;
    }
    let version = ctx
        .provider_settings
        .get("api_version")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|version| !version.is_empty())
        .unwrap_or(DEFAULT_IMAGE_API_VERSION);
    let pair = format!("api-version={version}");
    query = Some(match query {
        Some(query) => format!("{query}&{pair}"),
        None => pair,
    });
    query
}

fn has_api_version(query: &str) -> bool {
    query
        .split('&')
        .any(|pair| pair.split('=').next() == Some("api-version"))
}

pub struct AzureChannel;

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Channel for AzureChannel {
    fn id(&self) -> &'static str {
        "azure"
    }

    fn provider_family(&self) -> Provider {
        Provider::OpenAi
    }

    fn routing_table(&self) -> crate::channel::routes::RouteList {
        use crate::channel::routes::{cg, local, pass, pv, responses_ws_to, xform};
        use crate::protocol::{ContentGenerationKind::*, Operation::*, Provider as P};
        let mut routes = vec![
            pass(ListModels, pv(P::OpenAi)),
            xform(ListModels, pv(P::Claude), ListModels, pv(P::OpenAi)),
            xform(ListModels, pv(P::Gemini), ListModels, pv(P::OpenAi)),
            pass(GetModel, pv(P::OpenAi)),
            xform(GetModel, pv(P::Claude), GetModel, pv(P::OpenAi)),
            xform(GetModel, pv(P::Gemini), GetModel, pv(P::OpenAi)),
            local(CountTokens, pv(P::OpenAi)),
            pass(CountTokens, pv(P::Claude)),
            local(CountTokens, pv(P::Gemini)),
            pass(GenerateContent, cg(OpenAiResponses)),
            pass(GenerateContent, cg(OpenAiChatCompletions)),
            pass(GenerateContent, cg(ClaudeMessages)),
            xform(
                GenerateContent,
                cg(GeminiGenerateContent),
                GenerateContent,
                cg(OpenAiChatCompletions),
            ),
            pass(StreamGenerateContent, cg(OpenAiResponses)),
            pass(StreamGenerateContent, cg(OpenAiChatCompletions)),
            pass(StreamGenerateContent, cg(ClaudeMessages)),
            xform(
                StreamGenerateContent,
                cg(GeminiGenerateContent),
                StreamGenerateContent,
                cg(OpenAiChatCompletions),
            ),
            pass(CreateEmbedding, pv(P::OpenAi)),
            xform(
                CreateEmbedding,
                pv(P::Gemini),
                CreateEmbedding,
                pv(P::OpenAi),
            ),
            pass(CreateImage, pv(P::OpenAi)),
            pass(EditImage, pv(P::OpenAi)),
            pass(CompactContent, pv(P::OpenAi)),
        ];
        routes.extend(responses_ws_to(cg(OpenAiResponses)));
        routes
    }

    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        let path = upstream_path(&ctx);
        let api_key = common::resolve_api_key(&ctx)?;
        let query = query(&ctx);
        let uri = common::resolve_uri(&ctx, &DEFAULTS, &path, query.as_deref())?;
        let headers = allow_headers(ctx.headers, DEFAULTS.forward_headers);
        let anthropic = is_anthropic(ctx.op);
        let mut req = build_request(ctx.method, uri, headers, ctx.body)?;
        auth::apply(&mut req, &api_key, anthropic)?;
        Ok(PreparedRequest::new(req))
    }

    fn shape_request(&self, body: Bytes, headers: &mut HeaderMap, ctx: &ShapeCtx) -> Bytes {
        shape::request(body, headers, ctx)
    }
}

#[cfg(test)]
mod tests;
