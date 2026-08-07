//! Amazon Bedrock native APIs over bearer-token authentication.

mod auth;
mod compact;
mod converse;
mod endpoint;
mod models;
mod shape;
mod stream;

use bytes::Bytes;
use http::HeaderMap;

use crate::channel::bulletins::common;
use crate::channel::http_util::{allow_headers, build_request};
use crate::channel::{
    Channel, ChannelError, ChannelStreamDecoder, Disposition, PrepareCtx, PreparedRequest, ShapeCtx,
};
use crate::protocol::{Operation, OperationKind, Provider};

const DEFAULT_REGION: &str = "us-east-1";
const FORWARD_HEADERS: &[&str] = &["anthropic-beta", "openai-beta"];

pub struct AwsBedrockChannel;

fn is_count_tokens(op: crate::protocol::OperationKey) -> bool {
    op.operation() == Operation::CountTokens
        && op.kind() == OperationKind::Provider(Provider::Claude)
}

fn is_models(op: crate::protocol::OperationKey) -> bool {
    matches!(op.operation(), Operation::ListModels | Operation::GetModel)
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Channel for AwsBedrockChannel {
    fn id(&self) -> &'static str {
        "aws-bedrock"
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
            xform(
                GenerateContent,
                cg(OpenAiResponses),
                GenerateContent,
                cg(ClaudeMessages),
            ),
            xform(
                GenerateContent,
                cg(OpenAiChatCompletions),
                GenerateContent,
                cg(ClaudeMessages),
            ),
            pass(GenerateContent, cg(ClaudeMessages)),
            xform(
                GenerateContent,
                cg(GeminiGenerateContent),
                GenerateContent,
                cg(ClaudeMessages),
            ),
            xform(
                StreamGenerateContent,
                cg(OpenAiResponses),
                StreamGenerateContent,
                cg(ClaudeMessages),
            ),
            xform(
                StreamGenerateContent,
                cg(OpenAiChatCompletions),
                StreamGenerateContent,
                cg(ClaudeMessages),
            ),
            pass(StreamGenerateContent, cg(ClaudeMessages)),
            xform(
                StreamGenerateContent,
                cg(GeminiGenerateContent),
                StreamGenerateContent,
                cg(ClaudeMessages),
            ),
            xform(
                CompactContent,
                pv(P::OpenAi),
                GenerateContent,
                cg(ClaudeMessages),
            ),
        ];
        routes.extend(responses_ws_to(cg(ClaudeMessages)));
        routes
    }

    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        let api_key = common::resolve_api_key(&ctx)?;
        let compact = compact::is_request(&ctx.body);
        let uri = endpoint::resolve(&ctx, compact)?;
        let headers = allow_headers(ctx.headers, FORWARD_HEADERS);
        let mut req = build_request(ctx.method, uri, headers, ctx.body)?;
        auth::apply(&mut req, &api_key)?;
        Ok(PreparedRequest::new(req))
    }

    fn shape_request(&self, body: Bytes, headers: &mut HeaderMap, ctx: &ShapeCtx) -> Bytes {
        shape::request(body, headers, ctx)
    }

    fn shape_response(&self, body: Bytes, ctx: &ShapeCtx) -> Bytes {
        shape::response(body, ctx)
    }

    fn classify(
        &self,
        status: http::StatusCode,
        headers: &HeaderMap,
        _body: &Bytes,
    ) -> Disposition {
        if status == http::StatusCode::FORBIDDEN {
            Disposition::Permanent
        } else {
            Disposition::from_http(status, headers)
        }
    }

    fn stream_decoder(&self) -> Option<Box<dyn ChannelStreamDecoder>> {
        Some(Box::new(stream::ConverseStreamDecoder::new()))
    }
}

#[cfg(test)]
mod cache_tests;
#[cfg(test)]
mod tests;
