use bytes::Bytes;
use gproxy_protocol::{ContentGenerationKind as Kind, Operation, OperationKey, WireFamily};
use serde_json::{Value, json};

use crate::{can_transform, request, response, response_stream_framed};

fn family(operation: Operation, family: WireFamily) -> OperationKey {
    OperationKey::family(operation, family)
}

fn content(operation: Operation, kind: Kind) -> OperationKey {
    OperationKey::content(operation, kind)
}

fn convert_request(source: OperationKey, target: OperationKey, value: Value) -> Value {
    serde_json::from_slice(
        &request(
            source,
            target,
            Bytes::from(serde_json::to_vec(&value).unwrap()),
            "upstream-model",
            false,
        )
        .unwrap(),
    )
    .unwrap()
}

fn convert_response(source: OperationKey, target: OperationKey, value: Value) -> Value {
    serde_json::from_slice(
        &response(
            source,
            target,
            Bytes::from(serde_json::to_vec(&value).unwrap()),
        )
        .unwrap(),
    )
    .unwrap()
}

#[test]
fn v2_transform_surface_has_a_v3_disposition() {
    let providers = [WireFamily::OpenAi, WireFamily::Claude, WireFamily::Gemini];
    for operation in [
        Operation::ListModels,
        Operation::GetModel,
        Operation::CountTokens,
    ] {
        for source in providers {
            for target in providers {
                if source != target {
                    assert!(can_transform(
                        family(operation, source),
                        family(operation, target)
                    ));
                }
            }
        }
    }
    for source in [
        Kind::OpenAiChat,
        Kind::OpenAiResponses,
        Kind::ClaudeMessages,
        Kind::GeminiGenerateContent,
    ] {
        for target in [
            Kind::OpenAiChat,
            Kind::OpenAiResponses,
            Kind::ClaudeMessages,
            Kind::GeminiGenerateContent,
        ] {
            if source != target {
                assert!(can_transform(
                    content(Operation::GenerateContent, source),
                    content(Operation::GenerateContent, target)
                ));
            }
        }
        assert!(can_transform(
            content(Operation::GenerateContent, source),
            content(
                Operation::StreamGenerateContent,
                Kind::OpenAiResponsesWebSocket
            ),
        ));
        assert!(can_transform(
            content(
                Operation::StreamGenerateContent,
                Kind::OpenAiResponsesWebSocket,
            ),
            content(Operation::StreamGenerateContent, source),
        ));
    }
    for target in [
        Kind::OpenAiChat,
        Kind::OpenAiResponses,
        Kind::ClaudeMessages,
        Kind::GeminiGenerateContent,
    ] {
        assert!(can_transform(
            family(Operation::CompactContent, WireFamily::OpenAi),
            content(Operation::GenerateContent, target),
        ));
        assert!(can_transform(
            content(Operation::GenerateContent, target),
            family(Operation::CompactContent, WireFamily::OpenAi),
        ));
        assert!(can_transform(
            family(Operation::CompactContent, WireFamily::OpenAi),
            content(Operation::StreamGenerateContent, target),
        ));
        assert!(can_transform(
            content(Operation::StreamGenerateContent, target),
            family(Operation::CompactContent, WireFamily::OpenAi),
        ));
    }
    assert!(can_transform(
        family(Operation::CreateEmbedding, WireFamily::OpenAi),
        family(Operation::BatchCreateEmbedding, WireFamily::Gemini),
    ));
    assert!(can_transform(
        family(Operation::BatchCreateEmbedding, WireFamily::Gemini),
        family(Operation::CreateEmbedding, WireFamily::OpenAi),
    ));
    for operation in [Operation::CreateVideo, Operation::RetrieveVideo] {
        assert!(can_transform(
            family(operation, WireFamily::OpenAi),
            family(operation, WireFamily::Gemini),
        ));
        assert!(can_transform(
            family(operation, WireFamily::Gemini),
            family(operation, WireFamily::OpenAi),
        ));
    }
}

#[test]
fn models_count_and_embedding_pairs_preserve_semantic_values() {
    let openai_models = family(Operation::ListModels, WireFamily::OpenAi);
    let gemini_models = family(Operation::ListModels, WireFamily::Gemini);
    let models = convert_response(
        openai_models,
        gemini_models,
        json!({"models":[{"name":"models/gemini-test","displayName":"Gemini","description":"Test model","inputTokenLimit":32,"outputTokenLimit":8,"supportedGenerationMethods":["generateContent"]}]}),
    );
    assert_eq!(models["data"][0]["id"], "gemini-test");
    assert_eq!(models["data"][0]["context_window"], 32);
    assert_eq!(models["data"][0]["description"], "Test model");
    assert_eq!(
        models["data"][0]["generation_methods"][0],
        "generateContent"
    );

    let openai_count = family(Operation::CountTokens, WireFamily::OpenAi);
    let gemini_count = family(Operation::CountTokens, WireFamily::Gemini);
    let count = convert_request(
        openai_count,
        gemini_count,
        json!({"model":"gpt","input":"hello"}),
    );
    assert_eq!(
        count["generateContentRequest"]["contents"][0]["parts"][0]["text"],
        "hello"
    );
    let count_response = convert_response(openai_count, gemini_count, json!({"totalTokens":7}));
    assert_eq!(count_response["input_tokens"], 7);

    let openai_embedding = family(Operation::CreateEmbedding, WireFamily::OpenAi);
    let gemini_batch = family(Operation::BatchCreateEmbedding, WireFamily::Gemini);
    let embedding = convert_request(
        openai_embedding,
        gemini_batch,
        json!({"model":"embed","input":["a","b"],"dimensions":16}),
    );
    assert_eq!(embedding["requests"].as_array().unwrap().len(), 2);
    assert_eq!(embedding["requests"][1]["content"]["parts"][0]["text"], "b");
}

#[test]
fn compact_image_and_video_pairs_reach_their_native_shapes() {
    let compact = family(Operation::CompactContent, WireFamily::OpenAi);
    let responses = content(Operation::GenerateContent, Kind::OpenAiResponses);
    let compact_request = convert_request(
        compact,
        responses,
        json!({"model":"gpt","input":"long context","instructions":"keep facts"}),
    );
    assert_eq!(compact_request["input"], "long context");
    assert_eq!(compact_request["instructions"], "keep facts");

    let image = family(Operation::CreateImage, WireFamily::OpenAi);
    let gemini_stream = content(
        Operation::StreamGenerateContent,
        Kind::GeminiGenerateContent,
    );
    let image_request = convert_request(
        image,
        gemini_stream,
        json!({"model":"image","prompt":"draw a fox","n":2}),
    );
    assert_eq!(
        image_request["contents"][0]["parts"][0]["text"],
        "draw a fox"
    );
    assert_eq!(image_request["generationConfig"]["candidateCount"], 2);
    let edit = convert_request(
        family(Operation::EditImage, WireFamily::OpenAi),
        gemini_stream,
        json!({
            "model":"image","prompt":"edit the fox",
            "images":["data:image/png;base64,aW1n"]
        }),
    );
    assert_eq!(
        edit["contents"][0]["parts"][1]["inlineData"]["data"],
        "aW1n"
    );
    let image_response = convert_response(
        image,
        gemini_stream,
        json!({"candidates":[{"content":{"parts":[{"inlineData":{"mimeType":"image/png","data":"aW1n"}}]}}]}),
    );
    assert_eq!(image_response["data"][0]["b64_json"], "aW1n");

    let openai_video = family(Operation::CreateVideo, WireFamily::OpenAi);
    let gemini_video = family(Operation::CreateVideo, WireFamily::Gemini);
    let video_request = convert_request(
        openai_video,
        gemini_video,
        json!({"prompt":"flyover","model":"sora-2","seconds":"8","size":"720x1280"}),
    );
    assert_eq!(video_request["instances"][0]["prompt"], "flyover");
    assert_eq!(video_request["parameters"]["aspectRatio"], "9:16");
}

#[test]
fn gemini_image_stream_becomes_openai_image_events() {
    let source = family(Operation::CreateImage, WireFamily::OpenAi);
    let target = content(
        Operation::StreamGenerateContent,
        Kind::GeminiGenerateContent,
    );
    let mut stream = response_stream_framed(
        source,
        target,
        gproxy_protocol::StreamFraming::Sse,
        gproxy_protocol::StreamFraming::JsonArray,
    )
    .unwrap();
    let wire = br#"[{"candidates":[{"content":{"parts":[{"inlineData":{"mimeType":"image/png","data":"aW1n"}}]},"finishReason":"STOP"}]}]"#;
    let mut output = Vec::new();
    for frame in stream.push(Bytes::copy_from_slice(wire)).unwrap() {
        output.extend_from_slice(&frame);
    }
    for frame in stream.finish().unwrap() {
        output.extend_from_slice(&frame);
    }
    let text = String::from_utf8(output).unwrap();
    assert!(text.contains("image_generation.partial_image"));
    assert!(text.contains("image_generation.completed"));
}

#[test]
fn responses_websocket_composes_with_existing_semantic_pairs() {
    let chat = content(Operation::GenerateContent, Kind::OpenAiChat);
    let websocket = content(
        Operation::StreamGenerateContent,
        Kind::OpenAiResponsesWebSocket,
    );
    let frame = convert_request(
        chat,
        websocket,
        json!({"model":"gpt","messages":[{"role":"user","content":"hello"}]}),
    );
    assert_eq!(frame["type"], "response.create");
    assert_eq!(frame["input"][0]["role"], "user");

    let claude = content(Operation::StreamGenerateContent, Kind::ClaudeMessages);
    let request = convert_request(
        websocket,
        claude,
        json!({"type":"response.create","model":"gpt","input":"hello"}),
    );
    assert_eq!(request["messages"][0]["role"], "user");
    assert_eq!(request["messages"][0]["content"][0]["text"], "hello");
}
