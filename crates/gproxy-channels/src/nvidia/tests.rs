use bytes::Bytes;
use gproxy_channel_api::{Channel, ChannelSupport, PrepareCtx, StreamCtx, StreamEnd};
use gproxy_protocol::{
    ContentGenerationKind as Kind, Operation, OperationKey, StreamFraming, WireFamily,
};
use http::{HeaderMap, Method};
use serde_json::{Value, json};

use super::NvidiaChannel;

const fn family(operation: Operation) -> OperationKey {
    OperationKey::family(operation, WireFamily::OpenAi)
}

const fn content(operation: Operation, kind: Kind) -> OperationKey {
    OperationKey::content(operation, kind)
}

#[test]
fn declares_truthful_operations() {
    let expected = [
        ChannelSupport::passthrough(family(Operation::ListModels)),
        ChannelSupport::passthrough(family(Operation::GetModel)),
        ChannelSupport::transform(
            OperationKey::family(Operation::ListModels, WireFamily::Claude),
            family(Operation::ListModels),
        ),
        ChannelSupport::transform(
            OperationKey::family(Operation::GetModel, WireFamily::Claude),
            family(Operation::GetModel),
        ),
        ChannelSupport::passthrough(content(Operation::GenerateContent, Kind::OpenAiChat)),
        ChannelSupport::transform(
            content(Operation::GenerateContent, Kind::OpenAiResponses),
            content(Operation::GenerateContent, Kind::OpenAiChat),
        ),
        ChannelSupport::transform(
            content(Operation::GenerateContent, Kind::ClaudeMessages),
            content(Operation::GenerateContent, Kind::OpenAiChat),
        ),
        ChannelSupport::transform(
            content(Operation::GenerateContent, Kind::GeminiGenerateContent),
            content(Operation::GenerateContent, Kind::OpenAiChat),
        ),
        ChannelSupport::passthrough(content(Operation::StreamGenerateContent, Kind::OpenAiChat)),
        ChannelSupport::transform(
            content(Operation::StreamGenerateContent, Kind::OpenAiResponses),
            content(Operation::StreamGenerateContent, Kind::OpenAiChat),
        ),
        ChannelSupport::transform(
            content(Operation::StreamGenerateContent, Kind::ClaudeMessages),
            content(Operation::StreamGenerateContent, Kind::OpenAiChat),
        ),
        ChannelSupport::transform(
            content(
                Operation::StreamGenerateContent,
                Kind::GeminiGenerateContent,
            ),
            content(Operation::StreamGenerateContent, Kind::OpenAiChat),
        ),
        ChannelSupport::passthrough(family(Operation::CreateEmbedding)),
    ];
    assert_eq!(NvidiaChannel.descriptor().supports, expected);
}

#[test]
fn resolves_default_and_exact_override() {
    let headers = HeaderMap::new();
    let secret = json!({"api_key":"nv-key"});
    let default_settings = json!({});
    let default = NvidiaChannel
        .prepare(PrepareCtx {
            key: family(Operation::CreateEmbedding),
            stream: false,
            method: &Method::POST,
            path: "/v1/embeddings",
            query: None,
            headers: &headers,
            body: &Bytes::from_static(br#"{"model":"client","input":"hi"}"#),
            upstream_model: "nvidia/nv-embed-v1",
            provider_settings: &default_settings,
            secret: &secret,
        })
        .unwrap();
    assert_eq!(
        default.request.uri(),
        "https://integrate.api.nvidia.com/v1/embeddings"
    );

    let settings = json!({
        "base_url":"https://ignored.example",
        "endpoints":{"openai_get_model":"https://override.example/models/{model}"}
    });
    let override_request = NvidiaChannel
        .prepare(PrepareCtx {
            key: family(Operation::GetModel),
            stream: false,
            method: &Method::GET,
            path: "/v1/models/client-model",
            query: None,
            headers: &headers,
            body: &Bytes::new(),
            upstream_model: "meta/llama 3",
            provider_settings: &settings,
            secret: &secret,
        })
        .unwrap();
    assert_eq!(
        override_request.request.uri(),
        "https://override.example/models/meta%2Fllama%203"
    );
    assert_eq!(
        override_request.request.headers()["authorization"],
        "Bearer nv-key"
    );
}

#[test]
fn rewrites_model_and_observes_stream_usage() {
    let headers = HeaderMap::new();
    let settings = json!({});
    let secret = json!({"api_key":"nv-key"});
    let key = content(Operation::StreamGenerateContent, Kind::OpenAiChat);
    let prepared = NvidiaChannel
        .prepare(PrepareCtx {
            key,
            stream: true,
            method: &Method::POST,
            path: "/v1/chat/completions",
            query: None,
            headers: &headers,
            body: &Bytes::from_static(br#"{"model":"client","messages":[],"stream":true}"#),
            upstream_model: "meta/llama-3.1-70b-instruct",
            provider_settings: &settings,
            secret: &secret,
        })
        .unwrap();
    let body: Value = serde_json::from_slice(prepared.request.body()).unwrap();
    assert_eq!(body["model"], "meta/llama-3.1-70b-instruct");
    assert_eq!(body["stream_options"]["include_usage"], true);

    let request_body = prepared.request.body().clone();
    let response_headers = HeaderMap::new();
    let mut decoder = NvidiaChannel
        .stream_decoder(StreamCtx {
            key,
            framing: StreamFraming::Sse,
            request_body: &request_body,
            response_headers: &response_headers,
        })
        .unwrap();
    let chunk = Bytes::from_static(
        b"data: {\"choices\":[],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":4,\"total_tokens\":11}}\n\n",
    );
    assert_eq!(decoder.push(chunk).unwrap().len(), 1);
    let usage = decoder.finish(StreamEnd::Complete).unwrap().usage.unwrap();
    assert_eq!((usage.input_tokens, usage.output_tokens), (7, 4));
}
