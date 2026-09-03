use bytes::Bytes;
use gproxy_protocol::{ContentGenerationKind as Kind, Operation};
use serde_json::json;

use super::{content, convert_request};
use crate::{TransformError, request};

#[test]
fn cross_protocol_requests_do_not_leak_foreign_extension_fields() {
    let responses = content(Operation::GenerateContent, Kind::OpenAiResponses);
    let gemini = content(Operation::GenerateContent, Kind::GeminiGenerateContent);
    let to_gemini = convert_request(
        responses,
        gemini,
        json!({
            "model":"route",
            "client_metadata":{"client":"codex"},
            "prompt_cache_key":"thread-not-a-gemini-cache",
            "input":[
                {"type":"message","role":"system","content":[{
                    "type":"input_text","text":"system policy"
                }]},
                {"type":"message","id":"msg_1","role":"user","content":[{
                    "type":"input_text","text":"hello"
                }]}
            ],
            "tools":[
                {
                    "type":"function","name":"lookup","description":"lookup",
                    "parameters":{"type":"object"},"external_web_access":true
                },
                {"type":"web_search_preview","external_web_access":true}
            ]
        }),
    );
    let wire = to_gemini.to_string();
    assert!(!wire.contains("client_metadata"));
    assert!(!wire.contains("openai_item_id"));
    assert!(!wire.contains("external_web_access"));
    assert!(to_gemini.get("cachedContent").is_none());
    assert_eq!(to_gemini["systemInstruction"]["role"], "user");
    assert_eq!(
        to_gemini["toolConfig"]["includeServerSideToolInvocations"],
        true
    );
    assert!(
        !to_gemini["contents"]
            .as_array()
            .unwrap()
            .iter()
            .any(|content| content["role"] == "system")
    );

    let to_responses = convert_request(
        gemini,
        responses,
        json!({
            "systemInstruction":{"role":"user","parts":[{"text":"system policy"}]},
            "contents":[
                {"role":"model","parts":[{"functionCall":{
                    "id":"call_1","name":"lookup","args":{"path":"task.txt"}
                },"thoughtSignature":"opaque"}]},
                {"role":"user","parts":[{"functionResponse":{
                    "id":"call_1","name":"lookup","response":{"output":"ok"}
                }}]}
            ]
        }),
    );
    let wire = to_responses.to_string();
    assert!(!wire.contains("functionCall"));
    assert!(!wire.contains("functionResponse"));
    assert!(!wire.contains("thought_signature"));
    assert_eq!(to_responses["input"][0]["type"], "reasoning");
    assert_eq!(to_responses["input"][1]["type"], "function_call");
    assert_eq!(to_responses["input"][2]["type"], "function_call_output");
    assert_eq!(to_responses["instructions"], "system policy");
}

#[test]
fn every_content_pair_drops_unknown_fields_at_each_object_level() {
    let sources = [
        (
            Kind::OpenAiChat,
            json!({
                "model":"route","future_marker":"root",
                "messages":[{"role":"user","future_marker":"message","content":[
                    {"type":"text","text":"hello","future_marker":"nested"}
                ]}]
            }),
        ),
        (
            Kind::OpenAiResponses,
            json!({
                "model":"route","future_marker":"root",
                "input":[{"type":"message","role":"user","future_marker":"message","content":[
                    {"type":"input_text","text":"hello","future_marker":"nested"}
                ]}]
            }),
        ),
        (
            Kind::ClaudeMessages,
            json!({
                "model":"route","max_tokens":32,"future_marker":"root",
                "messages":[{"role":"user","future_marker":"message","content":[
                    {"type":"text","text":"hello","future_marker":"nested"}
                ]}]
            }),
        ),
        (
            Kind::GeminiGenerateContent,
            json!({
                "model":"models/route","future_marker":"root",
                "contents":[{"role":"user","future_marker":"message","parts":[
                    {"text":"hello","future_marker":"nested"}
                ]}]
            }),
        ),
    ];
    for (source_kind, input) in sources {
        let source = content(Operation::GenerateContent, source_kind);
        for target_kind in [
            Kind::OpenAiChat,
            Kind::OpenAiResponses,
            Kind::ClaudeMessages,
            Kind::GeminiGenerateContent,
        ] {
            if source_kind == target_kind {
                continue;
            }
            let output = convert_request(
                source,
                content(Operation::GenerateContent, target_kind),
                input.clone(),
            );
            assert!(
                !output.to_string().contains("future_marker"),
                "{source_kind:?} -> {target_kind:?} leaked an extension: {output}"
            );
        }
    }
}

#[test]
fn unknown_required_shapes_fail_after_optional_items_are_filtered() {
    let responses = content(Operation::GenerateContent, Kind::OpenAiResponses);
    let chat = content(Operation::GenerateContent, Kind::OpenAiChat);
    let only_unknown = request(
        responses,
        chat,
        Bytes::from_static(br#"{"model":"route","input":[{"type":"future_item"}]}"#),
        "upstream-model",
        false,
    );
    assert!(matches!(
        only_unknown,
        Err(TransformError::Unsupported { .. })
    ));

    let gemini = content(Operation::GenerateContent, Kind::GeminiGenerateContent);
    let claude = content(Operation::GenerateContent, Kind::ClaudeMessages);
    let unknown_role = request(
        gemini,
        claude,
        Bytes::from_static(br#"{"contents":[{"role":"future","parts":[{"text":"hi"}]}]}"#),
        "upstream-model",
        false,
    );
    assert!(matches!(
        unknown_role,
        Err(TransformError::Unsupported { .. })
    ));
}
