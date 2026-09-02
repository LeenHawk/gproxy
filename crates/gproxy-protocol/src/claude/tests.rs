use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use super::*;

#[test]
fn messages_and_stream_unions_roundtrip_unknown_fields() {
    let tool_wire = json!([
        {"type":"bash_20250124","name":"bash"},
        {"type":"text_editor_20250728","name":"str_replace_based_edit_tool"},
        {"type":"future_tool","name":"future"}
    ]);
    let tools: Vec<Tool> = serde_json::from_value(tool_wire.clone()).expect("decode tools");
    assert!(matches!(tools[0], Tool::Command(_)));
    assert!(matches!(tools[1], Tool::TextEditor(_)));
    assert!(matches!(tools[2], Tool::Unknown(_)));
    assert_eq!(
        serde_json::to_value(tools).expect("encode tools"),
        tool_wire
    );

    roundtrip::<CreateMessageRequestBody>(json!({
        "model":"claude-future-1",
        "messages":[{
            "role":"user",
            "content":[
                {"type":"text","text":"hello","future_text":true},
                {"type":"hologram","payload":{"x":1}}
            ],
            "future_message":"kept"
        }],
        "max_tokens":64,
        "future_request":{"enabled":true}
    }));

    let delta = roundtrip::<StreamEvent>(json!({
        "type":"content_block_delta",
        "index":0,
        "delta":{"type":"future_delta","payload":[1,2,3]},
        "trace_id":"trace_1"
    }));
    let StreamEvent::Known(event) = delta else {
        panic!("known stream event must remain typed");
    };
    let KnownStreamEvent::ContentBlockDelta { delta, .. } = *event else {
        panic!("expected content block delta");
    };
    let EventDelta::Unknown(object) = *delta else {
        panic!("future delta must remain a typed object");
    };
    assert_eq!(object.type_, "future_delta");
    let future = roundtrip::<StreamEvent>(json!({
        "type":"future_event",
        "nested":{"value":7}
    }));
    let StreamEvent::Unknown(object) = future else {
        panic!("future stream event must remain a typed object");
    };
    assert_eq!(object.type_, "future_event");
    assert_eq!(object.rest["nested"]["value"], 7);
    roundtrip::<ContextManagementResponse>(json!({
        "applied_edits":[{
            "type":"clear_tool_uses_20250919",
            "cleared_input_tokens":12,
            "cleared_tool_uses":2,
            "future_edit":{"source":"server"}
        }]
    }));
    roundtrip::<Diagnostics>(json!({
        "cache_miss_reason":{
            "type":"model_changed",
            "cache_missed_input_tokens":9,
            "future_diagnostic":true
        }
    }));
    roundtrip::<ContextManagementConfig>(json!({
        "edits":[{
            "type":"clear_thinking_20251015",
            "keep":{"type":"all","future_keep":true},
            "future_edit":"kept"
        }]
    }));
    roundtrip::<Diagnostics>(json!({
        "cache_miss_reason":{
            "type":"previous_message_not_found",
            "future_diagnostic":"kept"
        }
    }));
    roundtrip::<Diagnostics>(json!({
        "cache_miss_reason":{
            "type":"unavailable",
            "future_diagnostic":"kept"
        }
    }));
    assert!(serde_json::from_value::<BoolOrStringArray>(json!(1)).is_err());
    assert!(serde_json::from_value::<ContextTrigger>(json!("input_tokens")).is_err());
    assert!(serde_json::from_value::<ThinkingKeep>(json!(7)).is_err());

    let fallback = json!({
        "type":"fallback",
        "from":{"model":"claude-sonnet-5"},
        "to":{"model":"claude-opus-5"}
    });
    let fallback_without_trigger = roundtrip::<ContentBlockParam>(fallback.clone());
    assert!(matches!(
        fallback_without_trigger,
        ContentBlockParam::Fallback(FallbackBlockParam { trigger: None, .. })
    ));
    let mut null_trigger = fallback.clone();
    null_trigger["trigger"] = Value::Null;
    let fallback_with_null = roundtrip::<ContentBlockParam>(null_trigger);
    assert!(matches!(
        fallback_with_null,
        ContentBlockParam::Fallback(FallbackBlockParam {
            trigger: Some(Value::Null),
            ..
        })
    ));
    let mut nested_trigger = fallback.clone();
    nested_trigger["trigger"] = json!({"future_reason":{"nested":true}});
    roundtrip::<ContentBlockParam>(nested_trigger);
    let mut response = fallback.clone();
    response["trigger"] = json!({"type":"refusal","category":"cyber","future_trigger":true});
    response["future_fallback"] = json!("kept");
    let response_fallback = roundtrip::<ResponseContentBlock>(response);
    assert!(matches!(
        response_fallback,
        ResponseContentBlock::Fallback(ResponseFallbackBlock { .. })
    ));
    assert!(serde_json::from_value::<ResponseFallbackBlock>(fallback.clone()).is_err());
    let mut untyped_trigger = fallback;
    untyped_trigger["trigger"] = json!({"future_reason":true});
    assert!(serde_json::from_value::<ResponseFallbackBlock>(untyped_trigger).is_err());

    let result = ResponseWebSearchResultBlock {
        encrypted_content: "opaque".into(),
        page_age: None,
        title: "Result".into(),
        type_: WebSearchResultBlockType::WebSearchResult,
        url: "https://example.test".into(),
        rest: Default::default(),
    };
    assert!(
        serde_json::to_value(result)
            .expect("encode web search result")
            .get("page_age")
            .is_none()
    );
}

#[test]
fn models_count_files_and_skill_versions_keep_extension_data() {
    roundtrip::<ModelInfo>(json!({
        "id":"claude-next","type":"model","display_name":"Claude Next",
        "created_at":"2026-08-24T00:00:00Z","max_input_tokens":1000000,
        "max_tokens":300000,"allowed_fallback_models":[],
        "capabilities":{
            "batch":{"supported":true},"citations":{"supported":true},
            "code_execution":{"supported":true},
            "context_management":{"supported":true},
            "effort":{"supported":true},"image_input":{"supported":true},
            "pdf_input":{"supported":true},"structured_outputs":{"supported":true},
            "thinking":{"supported":true,"types":{}},
            "future_capability":{"mode":"x"}
        },
        "future_model":42
    }));
    roundtrip::<CountTokensResponseBody>(json!({
        "input_tokens":12,"context_management":{"original_input_tokens":20},
        "future_count":"kept"
    }));
    roundtrip::<FileMetadata>(json!({
        "id":"file_1","created_at":"2026-08-24T00:00:00Z",
        "filename":"a.txt","mime_type":"text/plain","size_bytes":3,
        "type":"file","downloadable":true,
        "checksum":"abc"
    }));
    roundtrip::<SkillVersion>(json!({
        "id":"skillver_1","created_at":"2026-08-24T00:00:00Z",
        "description":"demo","directory":"demo","name":"Demo","skill_id":"skill_1",
        "type":"skill_version","version":"1","manifest":{"entry":"SKILL.md"}
    }));
}

#[test]
fn fable_5_1_message_controls_and_transformations_roundtrip() {
    let request = roundtrip::<CreateMessageRequestBody>(json!({
        "model":"claude-fable-5-1",
        "max_tokens":1024,
        "thinking":{
            "type":"adaptive",
            "display":"updates",
            "block_binding":{"prefix_mismatch_behavior":"drop_block"}
        },
        "messages":[
            {"role":"system","content":[],"output_config":{"effort":"low"}},
            {"role":"user","content":"hello"},
            {"role":"system","content":"one turn","clear_at":"next_user_message"}
        ]
    }));
    assert!(matches!(
        request.model,
        ClaudeModel::Known(ClaudeModelKnown::ClaudeFable51)
    ));

    let response = roundtrip::<CreateMessageResponseBody>(json!({
        "id":"msg_1","type":"message","role":"assistant","content":[],
        "model":"claude-fable-5-1","stop_reason":"end_turn","stop_sequence":null,
        "usage":{"input_tokens":3,"output_tokens":1},
        "input_transformations":[{
            "type":"thinking_dropped","path":"messages.1.content.0",
            "reason":"prefix_binding_mismatch"
        }]
    }));
    assert!(matches!(
        response.input_transformations.as_deref(),
        Some([InputTransformation::ThinkingDropped(_)])
    ));

    roundtrip::<StreamEvent>(json!({
        "type":"message_delta",
        "delta":{"stop_reason":"end_turn"},
        "usage":{"output_tokens":1},
        "input_transformations":[{
            "type":"thinking_dropped","path":"messages.1.content.0",
            "reason":"model_binding_mismatch"
        }]
    }));
}

fn roundtrip<T>(wire: Value) -> T
where
    T: DeserializeOwned + Serialize,
{
    let decoded: T = serde_json::from_value(wire.clone()).expect("decode wire");
    assert_eq!(serde_json::to_value(&decoded).expect("encode wire"), wire);
    decoded
}
