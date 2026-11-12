#![allow(dead_code)]

use gproxy_client_sdk::*;

use serde_json::{Value, json};

pub fn request(dialect: Dialect, messages: Vec<Message>) -> GenerateRequest {
    let mut request = GenerateRequest::new(
        Model {
            id: "provider/model alias".into(),
            dialect,
        },
        messages,
    );
    request.settings.max_output_tokens = Some(1024);
    request
}

pub fn persisted(result: GenerateResult) -> Vec<Message> {
    let value = serde_json::to_value(Conversation::new(vec![result.into_message()])).unwrap();
    assert_eq!(value["schema_version"], "1");
    serde_json::from_value::<Conversation>(value)
        .unwrap()
        .into_messages()
}

#[cfg(feature = "claude")]
pub fn claude_events(native: &Value) -> Vec<Value> {
    let mut start = native.clone();
    start["content"] = json!([]);
    start["stop_reason"] = Value::Null;
    start["usage"]["output_tokens"] = 0.into();
    let mut events = vec![json!({"type":"message_start","message":start})];
    for (index, block) in native["content"].as_array().unwrap().iter().enumerate() {
        let mut start = block.clone();
        match block["type"].as_str().unwrap() {
            "text" => start["text"] = "".into(),
            "thinking" => {
                start["thinking"] = "".into();
                start["signature"] = "".into();
            }
            "tool_use" => start["input"] = json!({}),
            _ => {}
        }
        events.push(json!({"type":"content_block_start","index":index,"content_block":start}));
        let deltas = match block["type"].as_str().unwrap() {
            "text" => vec![json!({"type":"text_delta","text":block["text"]})],
            "thinking" => vec![
                json!({"type":"thinking_delta","thinking":block["thinking"]}),
                json!({"type":"signature_delta","signature":block["signature"]}),
            ],
            "tool_use" => {
                vec![json!({"type":"input_json_delta","partial_json":block["input"].to_string()})]
            }
            _ => vec![],
        };
        for delta in deltas {
            events.push(json!({"type":"content_block_delta","index":index,"delta":delta}));
        }
        events.push(json!({"type":"content_block_stop","index":index}));
    }
    events.push(json!({"type":"message_delta","delta":{"stop_reason":native["stop_reason"]},"usage":native["usage"]}));
    events.push(json!({"type":"message_stop"}));
    events
}

#[cfg(feature = "openai")]
pub fn openai_events(native: &Value) -> Vec<Value> {
    let mut events = vec![];
    for (index, item) in native["output"].as_array().unwrap().iter().enumerate() {
        events.push(json!({"type":"response.output_item.added","output_index":index,"sequence_number":events.len(),"item":item}));
        match item["type"].as_str().unwrap() {
            "message" => {
                for (part_index, part) in item["content"].as_array().unwrap().iter().enumerate() {
                    events.push(json!({
                        "type":"response.output_text.delta",
                        "item_id":item[
                            "id"
                        ],
                        "output_index":index,
                        "content_index":part_index,
                        "delta":part[
                            "text"
                        ],
                        "sequence_number":events.len(),
                        "logprobs":[

                        ]
                    }));
                }
            }
            "reasoning" => events.push(json!({
                "type":"response.reasoning_summary_text.delta",
                "item_id":item[
                    "id"
                ],
                "output_index":index,
                "summary_index":0,
                "delta":"onetwo",
                "sequence_number":events.len()
            })),
            "function_call" => events.push(json!({
                "type":"response.function_call_arguments.delta",
                "item_id":item[
                    "id"
                ],
                "output_index":index,
                "delta":item[
                    "arguments"
                ],
                "sequence_number":events.len()
            })),
            _ => {}
        }
        events.push(json!({"type":"response.output_item.done","output_index":index,"sequence_number":events.len(),"item":item}));
    }
    events.push(
        json!({"type":"response.completed","sequence_number":events.len(),"response":native}),
    );
    events
}

#[cfg(any(feature = "claude", feature = "openai", feature = "gemini"))]
pub fn decode(dialect: Dialect, values: Vec<Value>) -> Vec<Event> {
    let mut decoder = stream::StreamDecoder::new(dialect);
    let mut events = values
        .into_iter()
        .flat_map(|v| decoder.push_json(&v.to_string()).unwrap())
        .collect::<Vec<_>>();
    events.extend(decoder.finish().unwrap());
    events
}

pub fn aggregate(events: Vec<Event>) -> GenerateResult {
    futures_executor::block_on(gproxy_client_sdk::aggregate(futures_util::stream::iter(
        events,
    )))
    .unwrap()
}

pub fn extensions(options: &mut ProviderOptions, vendor: Vendor) -> &mut protocol::Rest {
    match vendor {
        Vendor::Claude => &mut options.claude.rest,
        Vendor::Openai => &mut options.openai.rest,
        Vendor::Gemini => &mut options.gemini.rest,
        Vendor::Unknown(_) => panic!("this fixture helper only sets known vendor options"),
    }
}

/// Normalize only fields whose vendor reference explicitly permits both a
/// missing key and null. Required nullable citation titles stay present.
pub fn normalize_claude(value: &mut Value) {
    match value {
        Value::Array(values) => {
            for value in values {
                normalize_claude(value);
            }
        }
        Value::Object(object) => {
            for key in ["citations", "toolset_name", "page_age"] {
                if object.get(key) == Some(&Value::Null) {
                    object.remove(key);
                }
            }
            if object.get("type").and_then(Value::as_str).is_some_and(|s| {
                matches!(
                    s,
                    "char_location" | "page_location" | "content_block_location"
                )
            }) {
                object.entry("document_title").or_insert(Value::Null);
            }
            for value in object.values_mut() {
                normalize_claude(value);
            }
        }
        _ => {}
    }
}
mod native;
pub mod script;
pub use native::*;
#[cfg(all(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
pub mod content;
#[cfg(all(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini",
    not(target_arch = "wasm32")
))]
pub mod generation;
