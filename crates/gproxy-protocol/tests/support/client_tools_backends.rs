use crate::client_tools_base as base;
pub fn followup(output: &Value) -> Value {
    let mut request = base::followup(output);
    request["max_output_tokens"] = json!(64);
    request
}
use serde_json::{Value, json};

pub fn request(stream: bool) -> Value {
    let mut request = base::request(stream);
    request["max_output_tokens"] = json!(64);
    request
}
pub fn claude(names: &[String]) -> Value {
    let chat = base::response(names);
    let blocks: Vec<_> = chat["choices"][0]["message"]["tool_calls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|call| {
            let input: Value =
                serde_json::from_str(call["function"]["arguments"].as_str().unwrap()).unwrap();
            json!({"type":"tool_use","id":call["id"],"name":call["function"]["name"],"input":input})
        })
        .collect();
    json!({"type":"message","id":"msg.native","model":"selected","role":"assistant","content":blocks,"stop_reason":"tool_use","stop_sequence":null,"usage":{"input_tokens":4,"output_tokens":8,"cache_creation_input_tokens":0,"cache_read_input_tokens":0,"output_tokens_details":{"thinking_tokens":0}}})
}
pub fn gemini(names: &[String], signed: bool) -> Value {
    let chat = base::response(names);
    let parts: Vec<_> = chat["choices"][0]["message"]["tool_calls"].as_array().unwrap().iter().map(|call| {
        let args: Value = serde_json::from_str(call["function"]["arguments"].as_str().unwrap()).unwrap();
        let mut part = json!({"functionCall":{"id":call["id"],"name":call["function"]["name"],"args":args}});
        if signed { part["thoughtSignature"] = json!("c2lnbmF0dXJl"); }
        part
    }).collect();
    json!({"responseId":"gemini.native","modelVersion":"selected","candidates":[{"index":0,"content":{"role":"model","parts":parts},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":4,"candidatesTokenCount":8,"totalTokenCount":12,"cachedContentTokenCount":0,"thoughtsTokenCount":0,"toolUsePromptTokenCount":0}})
}
