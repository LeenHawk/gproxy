//! Amazon Bedrock Runtime channel: OpenAI compatibility plus native Claude InvokeModel.

mod shape;
mod stream;

use bytes::Bytes;
use http::HeaderMap;

use super::bedrock::{self, Service};
use crate::channel::bulletins::common;
use crate::channel::http_util::{allow_headers, allow_query, build_request};
use crate::channel::{Channel, ChannelError, PrepareCtx, PreparedRequest, ShapeCtx};
use crate::protocol::{ContentGenerationKind, Operation, OperationKind, Provider};

const FORWARD_HEADERS: &[&str] = &["openai-beta"];
const FORWARD_QUERY: &[&str] = &["after", "before", "limit", "order"];

pub struct BedrockRuntimeChannel;

fn is_claude_messages(op: crate::protocol::OperationKey) -> bool {
    op.kind == OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages)
}

fn is_claude_count_tokens(op: crate::protocol::OperationKey) -> bool {
    op.operation == Operation::CountTokens && op.kind == OperationKind::Provider(Provider::Claude)
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Channel for BedrockRuntimeChannel {
    fn id(&self) -> &'static str {
        "bedrock-runtime"
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
            pass(GenerateContent, cg(OpenAiChatCompletions)),
            xform(
                GenerateContent,
                cg(OpenAiResponses),
                GenerateContent,
                cg(OpenAiChatCompletions),
            ),
            pass(GenerateContent, cg(ClaudeMessages)),
            xform(
                GenerateContent,
                cg(GeminiGenerateContent),
                GenerateContent,
                cg(OpenAiChatCompletions),
            ),
            pass(StreamGenerateContent, cg(OpenAiChatCompletions)),
            xform(
                StreamGenerateContent,
                cg(OpenAiResponses),
                StreamGenerateContent,
                cg(OpenAiChatCompletions),
            ),
            pass(StreamGenerateContent, cg(ClaudeMessages)),
            xform(
                StreamGenerateContent,
                cg(GeminiGenerateContent),
                StreamGenerateContent,
                cg(OpenAiChatCompletions),
            ),
            xform(
                CompactContent,
                pv(P::OpenAi),
                GenerateContent,
                cg(ClaudeMessages),
            ),
        ];
        routes.extend(responses_ws_to(cg(OpenAiChatCompletions)));
        routes
    }

    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        let invoke = is_claude_messages(ctx.op);
        let count_tokens = is_claude_count_tokens(ctx.op);
        let encoded_model = crate::channel::oauth::percent_encode(ctx.upstream_model_id);
        let path = if invoke || count_tokens {
            if ctx.upstream_model_id.trim().is_empty() {
                return Err(ChannelError::Build(
                    "Bedrock Runtime requires an upstream model id".into(),
                ));
            }
            if count_tokens {
                format!("/model/{encoded_model}/count-tokens")
            } else if ctx.stream {
                format!("/model/{encoded_model}/invoke-with-response-stream")
            } else {
                format!("/model/{encoded_model}/invoke")
            }
        } else {
            ctx.path.to_owned()
        };
        let api_key = common::resolve_api_key(&ctx)?;
        let query = allow_query(ctx.query, FORWARD_QUERY);
        let uri = bedrock::resolve_uri(
            &ctx,
            Service::Runtime,
            &path,
            query.as_deref(),
            &encoded_model,
        )?;
        let mut headers = allow_headers(ctx.headers, FORWARD_HEADERS);
        if invoke && ctx.stream {
            headers.insert(
                http::header::ACCEPT,
                http::HeaderValue::from_static("application/vnd.amazon.eventstream"),
            );
            headers.insert(
                http::HeaderName::from_static("x-amzn-bedrock-accept"),
                http::HeaderValue::from_static("application/json"),
            );
        }
        let mut req = build_request(ctx.method, uri, headers, ctx.body)?;
        common::inject_bearer(&mut req, &api_key)?;
        Ok(PreparedRequest::new(req))
    }

    fn shape_request(&self, body: Bytes, headers: &mut HeaderMap, ctx: &ShapeCtx) -> Bytes {
        shape::request(body, headers, ctx)
    }

    fn shape_response(&self, body: Bytes, ctx: &ShapeCtx) -> Bytes {
        shape::response(body, ctx)
    }

    fn stream_decoder(&self) -> Option<Box<dyn crate::channel::ChannelStreamDecoder>> {
        Some(Box::new(stream::BedrockRuntimeStreamDecoder::new()))
    }
}

#[cfg(test)]
mod tests;
