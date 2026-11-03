mod support;
use gproxy_client_sdk::*;
use serde_json::{Value, json};

#[cfg(feature = "openai")]
#[test]
fn responses_same_vendor_preserves_item_boundaries_encryption_and_lexical_arguments() {
    let native = support::openai();
    let result = dialects::openai::parse(serde_json::from_value(native.clone()).unwrap()).unwrap();
    let request = support::request(Dialect::Openai, support::persisted(result));
    let rendered = dialects::render(&request, false).unwrap().request;
    assert_eq!(rendered["input"], native["output"]);
}

#[cfg(feature = "openai")]
#[test]
fn responses_refusal_is_portable_text_with_native_refusal_metadata() {
    let mut native = support::openai();
    native["output"][1]["content"] =
        json!([{"type":"refusal","refusal":"blocked","futureRefusal":true}]);
    let result = dialects::openai::parse(serde_json::from_value(native.clone()).unwrap()).unwrap();
    assert_eq!(
        result.finish_reason.unified,
        FinishReasonKind::ContentBlocked
    );
    assert_eq!(
        dialects::render(
            &support::request(Dialect::Openai, support::persisted(result)),
            false
        )
        .unwrap()
        .request["input"],
        native["output"]
    );
}

#[cfg(feature = "openai")]
#[test]
fn responses_stream_aggregates_to_the_buffered_result_and_can_be_replayed() {
    let native = support::openai();
    let events = support::decode(Dialect::Openai, support::openai_events(&native));
    assert!(events.iter().any(|e| matches!(e, Event::TextDelta { .. })));
    let result = support::aggregate(events);
    assert_eq!(
        result,
        dialects::openai::parse(serde_json::from_value(native.clone()).unwrap()).unwrap()
    );
    assert_eq!(
        dialects::render(
            &support::request(Dialect::Openai, support::persisted(result)),
            false
        )
        .unwrap()
        .request["input"],
        native["output"]
    );
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
            dialects::render(&request, false).unwrap().request["input"][0],
            output
        );
    }
}
#[cfg(all(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
#[test]
fn responses_generated_images_and_builtin_tools_roundtrip_as_portable_blocks() {
    let mut native = support::openai();
    native["output"] = json!([
        {
            "type": "image_generation_call",
            "id": "ig_1",
            "status": "completed",
            "result": "AQID",
            "output_format": "png",
            "revised_prompt": "a landscape",
            "extra": 1
        },
        {
            "type": "web_search_call",
            "id": "web_1",
            "status": "completed",
            "action": {
                "type": "search",
                "query": "weather",
                "extra": 2
            },
            "extra": 3
        }
    ]);
    let parsed = dialects::openai::parse(serde_json::from_value(native.clone()).unwrap()).unwrap();
    assert!(matches!(parsed.content[0].content, Content::Image(_)));
    assert!(matches!(
        parsed.content[1].content,
        Content::ToolCall(ToolCall {
            provider_executed: true,
            ..
        })
    ));
    let request = support::request(Dialect::Openai, support::persisted(parsed));
    assert_eq!(
        dialects::render(&request, false).unwrap().request["input"],
        native["output"]
    );
}

#[cfg(feature = "openai")]
#[test]
fn responses_assistant_text_without_native_envelope_uses_the_easy_message_form() {
    // Live upstream rejects `input_text` parts on assistant messages; history from
    // another vendor or built by the application has no output-message envelope.
    let request = support::request(
        Dialect::Openai,
        vec![
            Message {
                role: Role::User,
                content: vec![Block::text("What is 17*23?")],
            },
            Message {
                role: Role::Assistant,
                content: vec![Block::text("391")],
            },
            Message {
                role: Role::User,
                content: vec![Block::text("Add 9.")],
            },
        ],
    );
    let rendered = dialects::render(&request, false).unwrap().request;
    assert_eq!(
        rendered["input"][1],
        json!({"type":"message","role":"assistant","content":"391"})
    );
    assert_eq!(rendered["input"][0]["content"][0]["type"], "input_text");
    assert_eq!(rendered["input"][2]["content"][0]["type"], "input_text");
}
