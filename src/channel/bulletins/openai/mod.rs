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
            // Content generation defaults to the Responses WebSocket transport.
            xform(
                GenerateContent,
                cg(OpenAiResponses),
                StreamGenerateContent,
                cg(OpenAiResponsesWebSocket),
            ),
            xform(
                GenerateContent,
                cg(OpenAiChatCompletions),
                StreamGenerateContent,
                cg(OpenAiResponsesWebSocket),
            ),
            xform(
                GenerateContent,
                cg(ClaudeMessages),
                StreamGenerateContent,
                cg(OpenAiResponsesWebSocket),
            ),
            xform(
                GenerateContent,
                cg(GeminiGenerateContent),
                StreamGenerateContent,
                cg(OpenAiResponsesWebSocket),
            ),
            xform(
                StreamGenerateContent,
                cg(OpenAiResponses),
                StreamGenerateContent,
                cg(OpenAiResponsesWebSocket),
            ),
            xform(
                StreamGenerateContent,
                cg(OpenAiChatCompletions),
                StreamGenerateContent,
                cg(OpenAiResponsesWebSocket),
            ),
            xform(
                StreamGenerateContent,
                cg(ClaudeMessages),
                StreamGenerateContent,
                cg(OpenAiResponsesWebSocket),
            ),
            xform(
                StreamGenerateContent,
                cg(GeminiGenerateContent),
                StreamGenerateContent,
                cg(OpenAiResponsesWebSocket),
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
    fn content_defaults_target_responses_websocket() {
        for (operation, kind) in [
            (Operation::GenerateContent, Kind::OpenAiResponses),
            (Operation::GenerateContent, Kind::OpenAiChatCompletions),
            (Operation::GenerateContent, Kind::ClaudeMessages),
            (Operation::GenerateContent, Kind::GeminiGenerateContent),
            (Operation::StreamGenerateContent, Kind::OpenAiResponses),
            (
                Operation::StreamGenerateContent,
                Kind::OpenAiChatCompletions,
            ),
            (Operation::StreamGenerateContent, Kind::ClaudeMessages),
            (
                Operation::StreamGenerateContent,
                Kind::GeminiGenerateContent,
            ),
        ] {
            let RoutingDecision::TransformTo(target) = route(operation, kind) else {
                panic!("route should transform to websocket");
            };
            assert_eq!(target.operation, Operation::StreamGenerateContent);
            assert_eq!(
                target.kind,
                OperationKind::ContentGeneration(Kind::OpenAiResponsesWebSocket)
            );
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
