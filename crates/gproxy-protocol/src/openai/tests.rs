use serde_json::{Value, json};

use super::compact::CompactResponseRequestBody;
use super::generate_content::chat::{ChatCompletionChunk, ChatCompletionRequest};
use super::generate_content::responses::{
    ResponseCreateRequest, ResponseItem, ResponseNamespaceTool, ResponseObject,
    ResponseStreamEvent, ResponseTool, ResponseWebSocketRequest,
};
use super::memories::{MemorySummarizeRequest, MemorySummarizeResponse};
use super::models::{ListModelsRequest, RetrieveModelRequest};

mod item_actions;
mod item_content;
mod tool_runtime;

fn round_trip<T>(value: Value) -> T
where
    T: serde::de::DeserializeOwned + serde::Serialize,
{
    let parsed = serde_json::from_value::<T>(value.clone()).expect("decode OpenAI wire value");
    assert_eq!(
        serde_json::to_value(&parsed).expect("encode OpenAI wire value"),
        value
    );
    parsed
}

#[test]
fn requests_preserve_rest_and_unknown_union_members() {
    let chat = json!({
        "model": "gpt-future",
        "messages": [
            {
                "role": "user",
                "content": [{"type":"text", "text":"hi", "part_future":7}],
                "message_future": {"enabled":true}
            },
            {"role":"future_role", "payload":{"nested":[1,2,3]}}
        ],
        "request_future": "kept"
    });
    let parsed = round_trip::<ChatCompletionRequest>(chat);
    assert_eq!(parsed.rest["request_future"], "kept");
    assert!(matches!(
        parsed.messages[1],
        super::generate_content::chat::ChatCompletionMessageParam::Unknown(_)
    ));

    let responses = json!({
        "model":"gpt-future",
        "input":[{"type":"future_item", "payload":{"x":1}}],
        "tools":[
            {
                "type":"namespace",
                "description":"CRM tools",
                "name":"crm",
                "tools":[{
                    "type":"function",
                    "name":"lookup",
                    "parameters":{"type":"object"},
                    "namespace_future":true
                }],
                "tool_future":["a","b"]
            },
            {
                "type":"function",
                "name":"ping",
                "parameters":null,
                "strict":null,
                "output_schema":{"type":"string"}
            }
        ],
        "response_request_future":42
    });
    let parsed = round_trip::<ResponseCreateRequest>(responses);
    assert_eq!(parsed.rest["response_request_future"], 42);
    let input = parsed.input.expect("response input");
    let super::generate_content::responses::ResponseInput::Items(items) = input else {
        panic!("expected item input");
    };
    assert!(matches!(items[0], ResponseItem::Unknown(_)));
    let ResponseTool::Namespace { tools, rest, .. } = &parsed.tools.as_ref().expect("tools")[0]
    else {
        panic!("expected namespace tool");
    };
    assert_eq!(rest["tool_future"], json!(["a", "b"]));
    let ResponseNamespaceTool::Function { rest, .. } = &tools[0] else {
        panic!("expected namespace function");
    };
    assert_eq!(rest["namespace_future"], true);
    assert!(matches!(
        parsed.tools.as_ref().expect("tools")[1],
        ResponseTool::Function {
            parameters: super::generate_content::responses::ResponseFunctionParameters::Null,
            strict: super::generate_content::responses::ResponseFunctionStrict::Null,
            ..
        }
    ));

    round_trip::<CompactResponseRequestBody>(json!({
        "model":"gpt-future",
        "compact_future":{"mode":"new"}
    }));
    round_trip::<ListModelsRequest>(json!({"future_query":"kept"}));
    round_trip::<RetrieveModelRequest>(json!({
        "model":"gpt-future",
        "future_path_metadata":true
    }));

    let memory = round_trip::<MemorySummarizeRequest>(json!({
        "model":"gpt-future",
        "traces":[{
            "id":"trace-1",
            "metadata":{"source_path":"/tmp/trace.jsonl", "metadata_future":1},
            "items":[{"type":"future_item", "payload":{"x":1}}],
            "trace_future":true
        }],
        "request_future":{"mode":"new"}
    }));
    assert!(memory.reasoning.is_none());
}

#[test]
fn responses_preserve_unknown_fields_and_items() {
    let response = json!({
        "id":"resp_1",
        "created_at":1,
        "object":"response",
        "output":[{"type":"future_output", "raw":{"deep":true}}],
        "response_future":{"region":"moon"}
    });
    let parsed = round_trip::<ResponseObject>(response);
    assert_eq!(parsed.rest["response_future"]["region"], "moon");
    assert!(matches!(parsed.output[0], ResponseItem::Unknown(_)));

    round_trip::<MemorySummarizeResponse>(json!({
        "output":[{
            "trace_summary":"raw summary",
            "memory_summary":"memory summary",
            "summary_future":true
        }],
        "response_future":{"region":"moon"}
    }));
}

#[test]
fn chat_and_responses_stream_events_round_trip_future_data() {
    round_trip::<ChatCompletionChunk>(json!({
        "id":"chatcmpl_1",
        "choices":[{
            "index":0,
            "delta":{"content":"hi", "delta_future":true},
            "finish_reason":null,
            "choice_future":9
        }],
        "created":1,
        "model":"gpt-future",
        "object":"chat.completion.chunk",
        "chunk_future":{"trace":"x"}
    }));

    let known = json!({
        "type":"response.output_text.delta",
        "content_index":0,
        "delta":"hello",
        "item_id":"msg_1",
        "output_index":0,
        "event_future":{"trace_id":"t1"}
    });
    let parsed = round_trip::<ResponseStreamEvent>(known);
    assert!(matches!(parsed, ResponseStreamEvent::Known(_)));

    let unknown = json!({
        "type":"response.future_event",
        "sequence_number":7,
        "raw":{"nested":[{"x":1}]}
    });
    let parsed = round_trip::<ResponseStreamEvent>(unknown.clone());
    let ResponseStreamEvent::Unknown(raw) = parsed else {
        panic!("future event must remain raw");
    };
    assert_eq!(raw, unknown);

    round_trip::<ResponseWebSocketRequest>(json!({
        "type":"response.create",
        "model":"gpt-future",
        "input":"hello"
    }));
    let future = round_trip::<ResponseWebSocketRequest>(json!({
        "type":"response.future",
        "payload":{"x":1}
    }));
    assert!(matches!(future, ResponseWebSocketRequest::Unknown(_)));
}
