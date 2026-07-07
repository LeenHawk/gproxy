//! OpenAI channel — `Authorization: Bearer`, default `https://api.openai.com`.

mod auth;

use crate::channel::bulletins::common::{self, ApiKeyDefaults};
use crate::channel::{Channel, ChannelError, PrepareCtx, PreparedRequest};
use crate::protocol::Provider;

const DEFAULTS: ApiKeyDefaults = ApiKeyDefaults {
    default_base_url: Some("https://api.openai.com"),
    forward_headers: &["openai-beta", "openai-organization", "openai-project"],
    forward_query: &[],
};

pub struct OpenAiChannel;

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Channel for OpenAiChannel {
    fn id(&self) -> &'static str {
        "openai"
    }

    fn provider_family(&self) -> Provider {
        Provider::OpenAi
    }

    fn routing_table(&self) -> crate::channel::routes::RouteList {
        use crate::channel::routes::{cg, local, pass, pv, xform};
        use crate::protocol::{ContentGenerationKind::*, Operation::*, Provider as P};
        vec![
            pass(ListModels, pv(P::OpenAi)),
            xform(ListModels, pv(P::Claude), ListModels, pv(P::OpenAi)),
            xform(ListModels, pv(P::Gemini), ListModels, pv(P::OpenAi)),
            pass(GetModel, pv(P::OpenAi)),
            xform(GetModel, pv(P::Claude), GetModel, pv(P::OpenAi)),
            xform(GetModel, pv(P::Gemini), GetModel, pv(P::OpenAi)),
            // §6.3: openai-family count_tokens is served locally.
            local(CountTokens, pv(P::OpenAi)),
            local(CountTokens, pv(P::Claude)),
            local(CountTokens, pv(P::Gemini)),
            // Both OpenAI HTTP wire formats are native; websocket is only used
            // when the downstream request itself is Responses WebSocket.
            pass(GenerateContent, cg(OpenAiResponses)),
            xform(
                GenerateContent,
                cg(OpenAiResponsesWebSocket),
                StreamGenerateContent,
                cg(OpenAiResponsesWebSocket),
            ),
            pass(GenerateContent, cg(OpenAiChatCompletions)),
            xform(
                GenerateContent,
                cg(ClaudeMessages),
                GenerateContent,
                cg(OpenAiChatCompletions),
            ),
            xform(
                GenerateContent,
                cg(GeminiGenerateContent),
                GenerateContent,
                cg(OpenAiChatCompletions),
            ),
            pass(StreamGenerateContent, cg(OpenAiResponses)),
            pass(StreamGenerateContent, cg(OpenAiResponsesWebSocket)),
            pass(StreamGenerateContent, cg(OpenAiChatCompletions)),
            xform(
                StreamGenerateContent,
                cg(ClaudeMessages),
                StreamGenerateContent,
                cg(OpenAiChatCompletions),
            ),
            xform(
                StreamGenerateContent,
                cg(GeminiGenerateContent),
                StreamGenerateContent,
                cg(OpenAiChatCompletions),
            ),
            pass(CreateImage, pv(P::OpenAi)),
            pass(EditImage, pv(P::OpenAi)),
            pass(CreateEmbedding, pv(P::OpenAi)),
            xform(
                CreateEmbedding,
                pv(P::Gemini),
                CreateEmbedding,
                pv(P::OpenAi),
            ),
            pass(CompactContent, pv(P::OpenAi)),
        ]
    }

    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        let websocket = crate::channel::responses_websocket::is_target(&ctx.method, ctx.path);
        let (mut req, key) = common::build_request(ctx, &DEFAULTS)?;
        auth::apply(&mut req, &key)?;
        if websocket {
            crate::channel::responses_websocket::apply_beta(req.headers_mut());
            *req.uri_mut() = crate::channel::responses_websocket::websocket_uri(req.uri())?;
            return crate::channel::responses_websocket::prepare(req);
        }
        Ok(PreparedRequest::new(req))
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use http::{HeaderMap, Method};
    use serde_json::json;

    use super::*;
    use crate::channel::routes::cg;
    use crate::protocol::{ContentGenerationKind as Kind, Operation, OperationKind};
    use crate::transform::routing::RoutingDecision;

    fn route(operation: Operation, kind: Kind) -> RoutingDecision {
        OpenAiChannel
            .routing_table()
            .into_iter()
            .find(|(source, _)| source.operation == operation && source.kind == cg(kind))
            .map(|(_, decision)| decision)
            .expect("missing route")
    }

    #[test]
    fn content_defaults_use_http_openai_routes_except_websocket_source() {
        for (operation, kind) in [
            (Operation::GenerateContent, Kind::OpenAiResponses),
            (Operation::GenerateContent, Kind::OpenAiChatCompletions),
            (Operation::StreamGenerateContent, Kind::OpenAiResponses),
            (
                Operation::StreamGenerateContent,
                Kind::OpenAiChatCompletions,
            ),
            (
                Operation::StreamGenerateContent,
                Kind::OpenAiResponsesWebSocket,
            ),
        ] {
            assert_eq!(route(operation, kind), RoutingDecision::Passthrough);
        }

        for (operation, kind, target_operation) in [
            (
                Operation::GenerateContent,
                Kind::OpenAiResponsesWebSocket,
                Operation::StreamGenerateContent,
            ),
            (
                Operation::GenerateContent,
                Kind::ClaudeMessages,
                Operation::GenerateContent,
            ),
            (
                Operation::GenerateContent,
                Kind::GeminiGenerateContent,
                Operation::GenerateContent,
            ),
            (
                Operation::StreamGenerateContent,
                Kind::ClaudeMessages,
                Operation::StreamGenerateContent,
            ),
            (
                Operation::StreamGenerateContent,
                Kind::GeminiGenerateContent,
                Operation::StreamGenerateContent,
            ),
        ] {
            let RoutingDecision::TransformTo(target) = route(operation, kind) else {
                panic!("route should transform");
            };
            assert_eq!(target.operation, target_operation);
            let target_kind = if kind == Kind::OpenAiResponsesWebSocket {
                Kind::OpenAiResponsesWebSocket
            } else {
                Kind::OpenAiChatCompletions
            };
            assert_eq!(target.kind, OperationKind::ContentGeneration(target_kind));
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn prepare_responses_websocket_returns_custom_stream() {
        let secret = json!({ "api_key": "sk-test" });
        let settings = json!({});
        let headers = HeaderMap::new();
        let ctx = PrepareCtx {
            secret: &secret,
            provider_settings: &settings,
            upstream_model_id: "gpt-test",
            method: Method::GET,
            path: "/v1/responses",
            query: None,
            headers: &headers,
            body: Bytes::from_static(
                br#"{"type":"response.create","model":"gpt-test","stream":true}"#,
            ),
        };

        assert!(matches!(
            OpenAiChannel.prepare(ctx).unwrap(),
            PreparedRequest::CustomStream(_)
        ));
    }
}
