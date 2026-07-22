//! Amazon Bedrock Mantle channel: regional OpenAI and Anthropic compatibility APIs.

mod auth;
mod shape;

use bytes::Bytes;
use http::HeaderMap;

use super::bedrock::{self, Service};
use crate::channel::bulletins::common;
use crate::channel::http_util::{allow_headers, allow_query, build_request};
use crate::channel::{Channel, ChannelError, PrepareCtx, PreparedRequest, ShapeCtx};
use crate::protocol::{ContentGenerationKind, OperationKind, Provider};

const FORWARD_HEADERS: &[&str] = &["anthropic-beta", "openai-beta"];
const FORWARD_QUERY: &[&str] = &["after", "before", "limit", "order"];

pub struct BedrockMantleChannel;

fn is_anthropic(op: crate::protocol::OperationKey) -> bool {
    op.kind == OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages)
        || op.kind == OperationKind::Provider(Provider::Claude)
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Channel for BedrockMantleChannel {
    fn id(&self) -> &'static str {
        "bedrock-mantle"
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
        let anthropic = is_anthropic(ctx.op);
        let path = if anthropic {
            format!("/anthropic{}", ctx.path)
        } else {
            ctx.path.to_owned()
        };
        let api_key = common::resolve_api_key(&ctx)?;
        let query = allow_query(ctx.query, FORWARD_QUERY);
        let uri = bedrock::resolve_uri(
            &ctx,
            Service::Mantle,
            &path,
            query.as_deref(),
            ctx.upstream_model_id,
        )?;
        let headers = allow_headers(ctx.headers, FORWARD_HEADERS);
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
