use bytes::Bytes;
use http::{HeaderMap, Method};
use serde_json::json;

use super::*;
use crate::protocol::{ContentGenerationKind as Kind, Operation, OperationKind, Provider};
use crate::routing::RoutingDecision;

#[test]
fn prepares_china_platform_request_with_bearer_auth() {
    let secret = json!({ "api_key": "sk-kimi-test" });
    let settings = json!({});
    let headers = HeaderMap::new();
    let request = KimiApiChannel
        .prepare(PrepareCtx {
            secret: &secret,
            provider_settings: &settings,
            op: crate::protocol::OperationKey::content_generation(
                Operation::GenerateContent,
                Kind::OpenAiChatCompletions,
            ),
            stream: false,
            upstream_model_id: "kimi-k3",
            method: Method::POST,
            path: "/v1/chat/completions",
            query: None,
            headers: &headers,
            body: Bytes::from_static(b"{}"),
        })
        .unwrap()
        .into_http()
        .unwrap();

    assert_eq!(request.uri(), "https://api.moonshot.cn/v1/chat/completions");
    assert_eq!(request.headers()["authorization"], "Bearer sk-kimi-test");
}

#[test]
fn supports_global_platform_base_url_override() {
    let secret = json!({ "api_key": "sk-kimi-test" });
    let settings = json!({ "base_url": "https://api.moonshot.ai" });
    let headers = HeaderMap::new();
    let request = KimiApiChannel
        .prepare(PrepareCtx {
            secret: &secret,
            provider_settings: &settings,
            op: crate::protocol::OperationKey::provider(Operation::ListModels, Provider::OpenAi),
            stream: false,
            upstream_model_id: "",
            method: Method::GET,
            path: "/v1/models",
            query: None,
            headers: &headers,
            body: Bytes::new(),
        })
        .unwrap()
        .into_http()
        .unwrap();

    assert_eq!(request.uri(), "https://api.moonshot.ai/v1/models");
}

#[test]
fn routes_all_available_openai_surfaces_as_passthrough() {
    let routes = KimiApiChannel.routing_table();
    let decision = |operation, kind| {
        routes
            .iter()
            .find(|(source, _)| source.operation() == operation && source.kind() == kind)
            .map(|(_, decision)| *decision)
            .expect("missing Kimi API route")
    };

    assert_eq!(
        decision(
            Operation::ListModels,
            OperationKind::Provider(Provider::OpenAi)
        ),
        RoutingDecision::Passthrough
    );
    assert_eq!(
        decision(
            Operation::GenerateContent,
            OperationKind::ContentGeneration(Kind::OpenAiChatCompletions),
        ),
        RoutingDecision::Passthrough
    );
    assert_eq!(
        decision(
            Operation::GenerateContent,
            OperationKind::ContentGeneration(Kind::OpenAiResponses),
        ),
        RoutingDecision::Passthrough
    );
    assert_eq!(
        decision(
            Operation::GetModel,
            OperationKind::Provider(Provider::OpenAi)
        ),
        RoutingDecision::Passthrough
    );
    for operation in [Operation::CreateEmbedding, Operation::CreateImage] {
        assert_eq!(
            decision(operation, OperationKind::Provider(Provider::OpenAi)),
            RoutingDecision::Passthrough
        );
    }
    assert_eq!(
        decision(
            Operation::CountTokens,
            OperationKind::Provider(Provider::OpenAi)
        ),
        RoutingDecision::Local
    );
}
