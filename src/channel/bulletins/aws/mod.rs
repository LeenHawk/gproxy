//! AWS channel: Bedrock Mantle APIs with Runtime used only for CountTokens.

mod auth;
mod shape;

use bytes::Bytes;
use http::HeaderMap;

use crate::channel::bulletins::common;
use crate::channel::http_util::{allow_headers, allow_query, build_request, exact_url, join_url};
use crate::channel::settings::{endpoint_by_key, endpoint_key};
use crate::channel::{Channel, ChannelError, PrepareCtx, PreparedRequest, ShapeCtx};
use crate::protocol::{ContentGenerationKind, Operation, OperationKind, Provider};

const DEFAULT_REGION: &str = "us-east-1";
const FORWARD_HEADERS: &[&str] = &["anthropic-beta", "openai-beta"];
const FORWARD_QUERY: &[&str] = &["after", "before", "limit", "order"];

pub struct AwsChannel;

fn is_count_tokens(op: crate::protocol::OperationKey) -> bool {
    op.operation == Operation::CountTokens && op.kind == OperationKind::Provider(Provider::Claude)
}

fn is_anthropic_mantle(op: crate::protocol::OperationKey) -> bool {
    !is_count_tokens(op)
        && (op.kind == OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages)
            || op.kind == OperationKind::Provider(Provider::Claude))
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Channel for AwsChannel {
    fn id(&self) -> &'static str {
        "aws"
    }

    fn provider_family(&self) -> Provider {
        Provider::OpenAi
    }

    fn routing_table(&self) -> crate::channel::routes::RouteList {
        use crate::channel::routes::{cg, pass, pv, responses_ws_to, xform};
        use crate::protocol::{ContentGenerationKind::*, Operation::*, Provider as P};
        let mut routes = vec![
            pass(ListModels, pv(P::OpenAi)),
            xform(ListModels, pv(P::Claude), ListModels, pv(P::OpenAi)),
            xform(ListModels, pv(P::Gemini), ListModels, pv(P::OpenAi)),
            pass(GetModel, pv(P::OpenAi)),
            xform(GetModel, pv(P::Claude), GetModel, pv(P::OpenAi)),
            xform(GetModel, pv(P::Gemini), GetModel, pv(P::OpenAi)),
            xform(CountTokens, pv(P::OpenAi), CountTokens, pv(P::Claude)),
            pass(CountTokens, pv(P::Claude)),
            xform(CountTokens, pv(P::Gemini), CountTokens, pv(P::Claude)),
            pass(GenerateContent, cg(OpenAiResponses)),
            pass(GenerateContent, cg(OpenAiChatCompletions)),
            pass(GenerateContent, cg(ClaudeMessages)),
            xform(
                GenerateContent,
                cg(GeminiGenerateContent),
                GenerateContent,
                cg(OpenAiResponses),
            ),
            pass(StreamGenerateContent, cg(OpenAiResponses)),
            pass(StreamGenerateContent, cg(OpenAiChatCompletions)),
            pass(StreamGenerateContent, cg(ClaudeMessages)),
            xform(
                StreamGenerateContent,
                cg(GeminiGenerateContent),
                StreamGenerateContent,
                cg(OpenAiResponses),
            ),
            xform(
                CompactContent,
                pv(P::OpenAi),
                GenerateContent,
                cg(OpenAiResponses),
            ),
        ];
        routes.extend(responses_ws_to(cg(OpenAiResponses)));
        routes
    }

    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        let count_tokens = is_count_tokens(ctx.op);
        let encoded_model = crate::channel::oauth::percent_encode(ctx.upstream_model_id);
        let path = if count_tokens {
            if ctx.upstream_model_id.trim().is_empty() {
                return Err(ChannelError::Build(
                    "AWS CountTokens requires an upstream model id".into(),
                ));
            }
            format!("/model/{encoded_model}/count-tokens")
        } else if is_anthropic_mantle(ctx.op) {
            format!("/anthropic{}", ctx.path)
        } else {
            ctx.path.to_owned()
        };
        let api_key = common::resolve_api_key(&ctx)?;
        let query = allow_query(ctx.query, FORWARD_QUERY);
        let uri = resolve_uri(&ctx, &path, query.as_deref(), &encoded_model, count_tokens)?;
        let headers = allow_headers(ctx.headers, FORWARD_HEADERS);
        let mut req = build_request(ctx.method, uri, headers, ctx.body)?;
        auth::apply(&mut req, &api_key, is_anthropic_mantle(ctx.op))?;
        Ok(PreparedRequest::new(req))
    }

    fn shape_request(&self, body: Bytes, headers: &mut HeaderMap, ctx: &ShapeCtx) -> Bytes {
        shape::request(body, headers, ctx)
    }

    fn shape_response(&self, body: Bytes, ctx: &ShapeCtx) -> Bytes {
        shape::response(body, ctx)
    }
}

fn resolve_uri(
    ctx: &PrepareCtx<'_>,
    path: &str,
    query: Option<&str>,
    model: &str,
    runtime: bool,
) -> Result<http::Uri, ChannelError> {
    if let Some(url) = endpoint_by_key(
        ctx.provider_settings,
        endpoint_key(ctx.op, ctx.stream),
        model,
    ) {
        return exact_url(&url, query);
    }
    let key = if runtime {
        "runtime_base_url"
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
    let generated = if runtime {
        format!("https://bedrock-runtime.{region}.amazonaws.com")
    } else {
        format!("https://bedrock-mantle.{region}.api.aws")
    };
    join_url(configured.unwrap_or(&generated), path, query)
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

#[cfg(test)]
mod tests;
