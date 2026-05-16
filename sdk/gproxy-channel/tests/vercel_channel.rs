#![cfg(feature = "vercel")]

use gproxy_channel::channel::Channel;
use gproxy_channel::channels::vercel::{VercelChannel, VercelCredential, VercelSettings};
use gproxy_channel::request::PreparedRequest;
use gproxy_channel::response::ResponseClassification;
use gproxy_channel::routing::RouteKey;
use gproxy_protocol::kinds::{OperationFamily, ProtocolKind};

fn prepared_request(operation: OperationFamily, protocol: ProtocolKind) -> PreparedRequest {
    PreparedRequest {
        method: http::Method::POST,
        route: RouteKey::new(operation, protocol),
        model: Some("anthropic/claude-sonnet-4".to_string()),
        query: None,
        body: br#"{"model":"anthropic/claude-sonnet-4","messages":[]}"#.to_vec(),
        headers: http::HeaderMap::new(),
    }
}

#[test]
fn vercel_defaults_to_ai_gateway_base_url_and_bearer_auth() {
    let settings = VercelSettings::default();
    let credential = VercelCredential {
        api_key: "test-vercel-key".to_string(),
    };
    let request = prepared_request(
        OperationFamily::GenerateContent,
        ProtocolKind::OpenAiChatCompletion,
    );

    let upstream = VercelChannel
        .prepare_request(&credential, &settings, &request)
        .expect("prepare request");

    assert_eq!(
        upstream.uri().to_string(),
        "https://ai-gateway.vercel.sh/v1/chat/completions"
    );
    assert_eq!(
        upstream
            .headers()
            .get(http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok()),
        Some("Bearer test-vercel-key")
    );
}

#[test]
fn vercel_maps_models_and_embeddings_to_documented_openai_compatible_paths() {
    let settings = VercelSettings::default();
    let credential = VercelCredential {
        api_key: "test-vercel-key".to_string(),
    };

    let models = VercelChannel
        .prepare_request(
            &credential,
            &settings,
            &PreparedRequest {
                method: http::Method::GET,
                route: RouteKey::new(OperationFamily::ModelList, ProtocolKind::OpenAi),
                model: None,
                query: None,
                body: Vec::new(),
                headers: http::HeaderMap::new(),
            },
        )
        .expect("model list request");
    assert_eq!(
        models.uri().to_string(),
        "https://ai-gateway.vercel.sh/v1/models"
    );

    let embeddings = VercelChannel
        .prepare_request(
            &credential,
            &settings,
            &prepared_request(OperationFamily::Embedding, ProtocolKind::OpenAi),
        )
        .expect("embedding request");
    assert_eq!(
        embeddings.uri().to_string(),
        "https://ai-gateway.vercel.sh/v1/embeddings"
    );
}

#[test]
fn vercel_rejects_unsupported_openai_responses_and_count_token_routes() {
    let settings = VercelSettings::default();
    let credential = VercelCredential {
        api_key: "test-vercel-key".to_string(),
    };
    let responses_request = prepared_request(
        OperationFamily::GenerateContent,
        ProtocolKind::OpenAiResponse,
    );
    let count_request = prepared_request(OperationFamily::CountToken, ProtocolKind::OpenAi);

    assert!(
        VercelChannel
            .prepare_request(&credential, &settings, &responses_request)
            .is_err()
    );
    assert!(
        VercelChannel
            .prepare_request(&credential, &settings, &count_request)
            .is_err()
    );
}

#[test]
fn vercel_query_quota_uses_ai_gateway_usage_endpoint() {
    let settings = VercelSettings::default();
    let credential = VercelCredential {
        api_key: "test-vercel-key".to_string(),
    };

    let request = VercelChannel
        .prepare_quota_request(&credential, &settings)
        .expect("quota request")
        .expect("vercel supports quota requests");

    assert_eq!(
        request.uri().to_string(),
        "https://ai-gateway.vercel.sh/v1/credits"
    );
    assert_eq!(request.method(), http::Method::GET);
    assert_eq!(
        request
            .headers()
            .get(http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok()),
        Some("Bearer test-vercel-key")
    );
}

#[test]
fn vercel_classifies_auth_and_rate_limit_responses() {
    let headers = http::HeaderMap::new();

    assert!(matches!(
        VercelChannel.classify_response(401, &headers, b""),
        ResponseClassification::AuthDead
    ));
    assert!(matches!(
        VercelChannel.classify_response(403, &headers, b""),
        ResponseClassification::AuthDead
    ));
    assert!(matches!(
        VercelChannel.classify_response(429, &headers, b""),
        ResponseClassification::RateLimited { .. }
    ));
}
