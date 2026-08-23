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

    roundtrip::<StreamEvent>(json!({
        "type":"content_block_delta",
        "index":0,
        "delta":{"type":"future_delta","payload":[1,2,3]},
        "trace_id":"trace_1"
    }));
    roundtrip::<StreamEvent>(json!({
        "type":"future_event",
        "nested":{"value":7}
    }));
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

fn roundtrip<T>(wire: Value)
where
    T: DeserializeOwned + Serialize,
{
    let decoded: T = serde_json::from_value(wire.clone()).expect("decode wire");
    assert_eq!(serde_json::to_value(decoded).expect("encode wire"), wire);
}
