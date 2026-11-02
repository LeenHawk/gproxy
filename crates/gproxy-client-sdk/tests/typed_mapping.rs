mod support;
use gproxy_client_sdk::*;
use serde_json::{Value, json};

#[cfg(all(feature = "claude", feature = "openai", feature = "gemini"))]
#[test]
fn v1_metadata_templates_remain_typed_after_the_content_revision() {
    let persisted = json!({
        "schema_version":"1",
        "messages":[{
            "role":"assistant",
            "content":[
                {
                    "type":"reasoning",
                    "text":"think",
                    "provider_metadata":{
                        "claude":{"native":{"type":"thinking","signature":"claude-sig","future":true}},
                        "openai":{"native":{
                            "type":"reasoning",
                            "id":"rs_saved",
                            "summary":[{"type":"summary_text","text":"think"}],
                            "encrypted_content":"cipher"
                        }},
                        "gemini":{"native":{"thought":true,"thoughtSignature":"gemini-sig"}}
                    }
                },
                {
                    "type":"text",
                    "text":"answer",
                    "provider_metadata":{
                        "claude":{"native":{"type":"text","future_text":true}},
                        "openai":{
                            "native":{"type":"output_text","annotations":[],"logprobs":[]},
                            "item":{"type":"message","id":"msg_saved","role":"assistant","status":"completed"},
                            "group":1
                        },
                        "gemini":{"native":{"thoughtSignature":"text-sig"}}
                    }
                },
                {
                    "type":"tool_call",
                    "id":"saved_call",
                    "name":"lookup",
                    "provider_executed":false,
                    "arguments":"{ \"x\": 1 }",
                    "provider_metadata":{
                        "claude":{"native":{"type":"tool_use","extra":true}},
                        "openai":{"native":{"type":"function_call","id":"fc_saved","status":"completed"}},
                        "gemini":{"native":{
                            "functionCall":{"id":"saved_call","args":{},"extra":true},
                            "thoughtSignature":"call-sig"
                        }}
                    }
                },
                {
                    "type":"image",
                    "source":{"type":"inline","bytes":[104,101,108,108,111]},
                    "media_type":"image/png","detail":"auto",
                    "provider_metadata":{"gemini":{"native":{
                        "inlineData":{"mimeType":"image/png","displayName":"saved image","extra":true},
                        "thoughtSignature":"image-sig"
                    }}}
                }
            ]
        }]
    });
    let conversation: Conversation = serde_json::from_value(persisted.clone()).unwrap();
    assert_eq!(serde_json::to_value(&conversation).unwrap(), persisted);
    let messages = conversation.into_messages();
    let claude =
        dialects::render(&support::request(Dialect::Claude, messages.clone()), false).unwrap();
    assert_eq!(
        claude["messages"][0]["content"][0]["signature"],
        "claude-sig"
    );
    let openai =
        dialects::render(&support::request(Dialect::Openai, messages.clone()), false).unwrap();
    assert_eq!(openai["input"][0]["encrypted_content"], "cipher");
    assert_eq!(openai["input"][1]["id"], "msg_saved");
    assert_eq!(openai["input"][2]["arguments"], "{ \"x\": 1 }");
    let gemini = dialects::render(&support::request(Dialect::Gemini, messages), false).unwrap();
    assert_eq!(
        gemini["contents"][0]["parts"][3]["inlineData"]["data"],
        "aGVsbG8="
    );
    assert_eq!(
        gemini["contents"][0]["parts"][3]["inlineData"]["displayName"],
        "saved image"
    );
}

#[cfg(feature = "claude")]
#[test]
fn claude_tool_result_optional_fields_are_replayed_losslessly() {
    for value in [
        json!({"type":"tool_result","tool_use_id":"call"}),
        json!({"type":"tool_result","tool_use_id":"call","content":"answer","is_error":false,"extra":true}),
        json!({"type":"tool_result","tool_use_id":"call","content":[{"type":"text","text":"answer","extra":true}],"is_error":true}),
    ] {
        let parsed =
            dialects::claude::parse_tool_result(serde_json::from_value(value.clone()).unwrap())
                .unwrap();
        let request = support::request(
            Dialect::Claude,
            vec![Message {
                role: Role::Tool,
                content: vec![parsed],
            }],
        );
        assert_eq!(
            dialects::render(&request, false).unwrap()["messages"][0]["content"][0],
            value
        );
    }
}

#[cfg(feature = "openai")]
#[test]
fn responses_tool_result_names_and_optional_call_ids_are_replayed_losslessly() {
    for mut output in [
        json!({"type":"function_call_output","id":"result","call_id":"call_1","output":"answer","status":"completed"}),
        json!({"type":"function_call_output","id":"result","name":"weather","output":"answer","status":"completed"}),
    ] {
        output["created_by"] = json!("assistant");
        output["caller"] = Value::Null;
        output["future"] = json!({"keep":true});
        let parsed =
            dialects::openai::parse_tool_result(serde_json::from_value(output.clone()).unwrap())
                .unwrap();
        let request = support::request(
            Dialect::Openai,
            vec![Message {
                role: Role::Tool,
                content: vec![parsed],
            }],
        );
        assert_eq!(
            dialects::render(&request, false).unwrap()["input"][0],
            output
        );
    }
}

#[cfg(all(feature = "claude", feature = "gemini"))]
#[test]
fn claude_tool_result_name_is_resolved_from_an_earlier_call_when_continuing_on_gemini() {
    let reply =
        dialects::claude::parse(serde_json::from_value(support::claude()).unwrap()).unwrap();
    let native: protocol::claude::content::ToolResultBlock = serde_json::from_value(json!({
        "type":"tool_result",
        "tool_use_id":"tool_1",
        "content":"sunny"
    }))
    .unwrap();
    let result = dialects::claude::parse_tool_result(native).unwrap();
    assert!(matches!(&result.content,Content::ToolResult(ToolResult {name,..}) if name.is_empty()));
    let request = support::request(
        Dialect::Gemini,
        vec![
            reply.into_message(),
            Message {
                role: Role::Tool,
                content: vec![result],
            },
        ],
    );
    let native = dialects::gemini::render(&request).unwrap();
    let response = native.contents[1].parts.as_ref().unwrap()[0]
        .function_response
        .as_ref()
        .unwrap();
    assert_eq!(response.name, "weather");
    assert_eq!(response.id.as_deref(), Some("tool_1"));
}

#[cfg(all(feature = "openai", feature = "gemini"))]
#[test]
fn responses_tool_result_name_is_resolved_from_earlier_call_id() {
    let mut native = support::openai();
    native["output"].as_array_mut().unwrap().push(json!({
        "type":"function_call_output",
        "id":"result_1",
        "call_id":"call_1",
        "output":"sunny",
        "status":"completed"
    }));
    let result = dialects::openai::parse(serde_json::from_value(native).unwrap()).unwrap();
    assert!(
        matches!(&result.content.last().unwrap().content,Content::ToolResult(ToolResult {name,..}) if name.is_empty())
    );
    let rendered = dialects::gemini::render(&support::request(
        Dialect::Gemini,
        vec![result.into_message()],
    ))
    .unwrap();
    let parts = rendered.contents[0].parts.as_ref().unwrap();
    let response = parts
        .iter()
        .find_map(|part| part.function_response.as_ref())
        .unwrap();
    assert_eq!(response.name, "weather");
}

#[cfg(feature = "gemini")]
#[test]
fn nameless_results_without_an_earlier_call_return_invalid_data() {
    let block = Block::new(Content::ToolResult(ToolResult {
        id: "missing".into(),
        name: String::new(),
        output: ToolOutput::Json(json!("result")),
        provider_executed: false,
        is_error: false,
    }));
    let request = support::request(
        Dialect::Gemini,
        vec![Message {
            role: Role::Tool,
            content: vec![block],
        }],
    );
    assert!(matches!(
        dialects::gemini::render(&request),
        Err(Error::InvalidData { .. })
    ));
}

#[cfg(all(feature = "claude", feature = "openai", feature = "gemini"))]
#[test]
fn malformed_metadata_and_options_return_errors_without_panics() {
    for dialect in [Dialect::Claude, Dialect::Openai, Dialect::Gemini] {
        let mut broken = Block::text("hello");
        broken.provider_metadata.insert(
            dialect.vendor(),
            serde_json::from_value(json!({"native":"not an object"})).unwrap(),
        );
        let request = support::request(
            dialect,
            vec![Message {
                role: Role::Assistant,
                content: vec![broken],
            }],
        );
        assert!(matches!(
            dialects::render(&request, false),
            Err(Error::InvalidData { .. })
        ));
        for native in [
            json!("not an object"),
            json!({"type":123}),
            json!({"thoughtSignature":[]} ),
        ] {
            let mut block = Block::text("hello");
            block.provider_metadata.insert(
                dialect.vendor(),
                serde_json::from_value(json!({"native":native})).unwrap(),
            );
            let request = support::request(
                dialect,
                vec![Message {
                    role: Role::Assistant,
                    content: vec![block],
                }],
            );
            // Malformed typed fields are rejected. Unknown extensions remain
            // valid metadata; neither case may panic.
            let _ = dialects::render(&request, false);
        }
        let mut request = support::request(
            dialect,
            vec![Message {
                role: Role::User,
                content: vec![Block::text("hello")],
            }],
        );
        let key = match dialect {
            Dialect::Claude => "output_config",
            Dialect::Openai => "text",
            Dialect::Gemini => "generationConfig",
            #[cfg(feature = "chat")]
            Dialect::OpenaiChat => "audio",
        };
        let mut options = protocol::Rest::new();
        options.insert(key.into(), json!("invalid object"));
        request.provider_options.insert(dialect.vendor(), options);
        assert!(matches!(
            dialects::render(&request, false),
            Err(Error::InvalidData { .. })
        ));
    }
}

#[cfg(feature = "claude")]
#[test]
fn typed_claude_mapping_preserves_nullable_history_and_citation_fields() {
    let mut native = support::claude();
    native["content"][2]["citations"] = json!([{
        "type":"char_location",
        "cited_text":"quote",
        "document_index":0,
        "document_title":null,
        "file_id":"source_file",
        "start_char_index":0,
        "end_char_index":5,
        "futureCitation":{"keep":true}
    }]);
    native["content"][3]["caller"] = Value::Null;
    native["content"][3]["toolset_name"] = Value::Null;
    let parsed = dialects::claude::parse(serde_json::from_value(native.clone()).unwrap()).unwrap();
    let rendered = dialects::render(
        &support::request(Dialect::Claude, support::persisted(parsed)),
        false,
    )
    .unwrap();
    assert_eq!(rendered["messages"][0]["content"], native["content"]);
}

#[cfg(feature = "claude")]
#[test]
fn mismatched_stream_deltas_and_invalid_indices_are_invalid_data() {
    let mut decoder = stream::StreamDecoder::new(Dialect::Claude);
    decoder.push_json(&json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}).to_string()).unwrap();
    assert!(matches!(decoder.push_json(&json!({"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"wrong block"}}).to_string()),Err(Error::InvalidData {..})));
    let mut decoder = stream::StreamDecoder::new(Dialect::Claude);
    assert!(matches!(decoder.push_json(&json!({"type":"content_block_start","index":-1,"content_block":{"type":"text","text":""}}).to_string()),Err(Error::InvalidData {..})));
}

#[cfg(all(feature = "claude", feature = "openai", feature = "gemini"))]
#[test]
fn persisted_metadata_templates_remain_compatible_and_do_not_duplicate_payloads() {
    for (vendor, result) in [
        (
            Vendor::Claude,
            dialects::claude::parse(serde_json::from_value(support::claude()).unwrap()).unwrap(),
        ),
        (
            Vendor::Openai,
            dialects::openai::parse(serde_json::from_value(support::openai()).unwrap()).unwrap(),
        ),
        (
            Vendor::Gemini,
            dialects::gemini::parse(serde_json::from_value(support::gemini()).unwrap()).unwrap(),
        ),
    ] {
        for block in result.content {
            let native = &block.provider_metadata[&vendor]["native"];
            match block.content {
                Content::Text(Text { .. }) => {
                    assert!(native.get("text").is_none());
                    assert!(native.get("refusal").is_none());
                }
                Content::ToolCall(ToolCall { .. }) if vendor != Vendor::Gemini => {
                    assert!(native.get("arguments").is_none());
                    assert!(native.get("input").is_none());
                }
                Content::Image(Image {
                    source: Asset::Inline { .. },
                    ..
                }) if vendor == Vendor::Gemini => {
                    assert!(native["inlineData"].get("data").is_none())
                }
                _ => {}
            }
        }
    }
}
