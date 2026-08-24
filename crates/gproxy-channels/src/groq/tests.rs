use bytes::Bytes;
use gproxy_channel_api::{Channel, ChannelSupport, PrepareCtx, StreamCtx, StreamEnd};
use gproxy_protocol::{
    ContentGenerationKind as Kind, Operation, OperationKey, StreamFraming, WireFamily,
};
use http::{HeaderMap, Method};
use serde_json::{Value, json};

use super::GroqChannel;

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
        ChannelSupport::passthrough(content(Operation::GenerateContent, Kind::OpenAiResponses)),
        ChannelSupport::transform(
            content(Operation::GenerateContent, Kind::ClaudeMessages),
            content(Operation::GenerateContent, Kind::OpenAiChat),
        ),
        ChannelSupport::transform(
            content(Operation::GenerateContent, Kind::GeminiGenerateContent),
            content(Operation::GenerateContent, Kind::OpenAiChat),
        ),
        ChannelSupport::passthrough(content(Operation::StreamGenerateContent, Kind::OpenAiChat)),
        ChannelSupport::passthrough(content(
            Operation::StreamGenerateContent,
            Kind::OpenAiResponses,
        )),
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
    ];
    assert_eq!(GroqChannel.descriptor().supports, expected);
}

#[test]
fn resolves_default_and_exact_override() {
    let headers = HeaderMap::new();
    let secret = json!({"api_key":"groq-key"});
    let default_settings = json!({});
    let default = GroqChannel
        .prepare(PrepareCtx {
            key: family(Operation::ListModels),
            stream: false,
            method: &Method::GET,
            path: "/v1/models",
            query: None,
            headers: &headers,
            body: &Bytes::new(),
            upstream_model: "",
            provider_settings: &default_settings,
            secret: &secret,
        })
        .unwrap();
    assert_eq!(
        default.request.uri(),
        "https://api.groq.com/openai/v1/models"
    );

    let settings = json!({
        "base_url":"https://ignored.example",
        "endpoints":{"openai_get_model":"https://override.example/models/{model}"}
    });
    let override_request = GroqChannel
        .prepare(PrepareCtx {
            key: family(Operation::GetModel),
            stream: false,
            method: &Method::GET,
            path: "/v1/models/client-model",
            query: None,
            headers: &headers,
            body: &Bytes::new(),
            upstream_model: "owner/model 1",
            provider_settings: &settings,
            secret: &secret,
        })
        .unwrap();
    assert_eq!(
        override_request.request.uri(),
        "https://override.example/models/owner%2Fmodel%201"
    );
    assert_eq!(
        override_request.request.headers()["authorization"],
        "Bearer groq-key"
    );
}

#[test]
fn rewrites_model_and_observes_stream_usage() {
    let headers = HeaderMap::new();
    let settings = json!({});
    let secret = json!({"api_key":"groq-key"});
    let key = content(Operation::StreamGenerateContent, Kind::OpenAiChat);
    let prepared = GroqChannel
        .prepare(PrepareCtx {
            key,
            stream: true,
            method: &Method::POST,
            path: "/v1/chat/completions",
            query: None,
            headers: &headers,
            body: &Bytes::from_static(br#"{"model":"client","messages":[],"stream":true}"#),
            upstream_model: "llama-3.3-70b-versatile",
            provider_settings: &settings,
            secret: &secret,
        })
        .unwrap();
    let body: Value = serde_json::from_slice(prepared.request.body()).unwrap();
    assert_eq!(body["model"], "llama-3.3-70b-versatile");
    assert_eq!(body["stream_options"]["include_usage"], true);

    let request_body = prepared.request.body().clone();
    let response_headers = HeaderMap::new();
    let mut decoder = GroqChannel
        .stream_decoder(StreamCtx {
            key,
            framing: StreamFraming::Sse,
            request_body: &request_body,
            response_headers: &response_headers,
        })
        .unwrap();
    let chunk = Bytes::from_static(
        b"data: {\"choices\":[],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":2,\"total_tokens\":5}}\n\n",
    );
    assert_eq!(decoder.push(chunk).unwrap().len(), 1);
    let usage = decoder.finish(StreamEnd::Complete).unwrap().usage.unwrap();
    assert_eq!((usage.input_tokens, usage.output_tokens), (3, 2));
}
