use bytes::Bytes;
use gproxy_protocol::{ContentGenerationKind as Kind, Operation};
use serde_json::json;

use super::{content, convert_request, convert_response};
use crate::ResponseStream;

#[test]
fn gemini_tool_signature_survives_a_claude_client_round_trip() {
    let gemini = content(Operation::GenerateContent, Kind::GeminiGenerateContent);
    let claude = content(Operation::GenerateContent, Kind::ClaudeMessages);
    let response = convert_response(
        claude,
        gemini,
        json!({
            "responseId":"response-1","modelVersion":"gemini",
            "candidates":[{"content":{"role":"model","parts":[{
                "functionCall":{"id":"call-1","name":"Read","args":{"path":"task.txt"}},
                "thoughtSignature":"opaque-signature"
            }]},"finishReason":"STOP"}]
        }),
    );
    assert_eq!(response["content"][0]["type"], "redacted_thinking");
    assert_eq!(response["content"][0]["data"], "opaque-signature");
    assert_eq!(response["content"][1]["type"], "tool_use");

    let request = convert_request(
        claude,
        gemini,
        json!({
            "model":"route","max_tokens":64,
            "messages":[
                {"role":"assistant","content":response["content"]},
                {"role":"user","content":[{
                    "type":"tool_result","tool_use_id":"call-1","content":"result"
                }]}
            ]
        }),
    );
    let wire = request.to_string();
    assert!(wire.contains("opaque-signature"), "{wire}");
    assert!(wire.contains("\"name\":\"Read\""), "{wire}");
}

#[test]
fn gemini_tool_signature_replays_as_claude_redacted_thinking() {
    let gemini = content(Operation::GenerateContent, Kind::GeminiGenerateContent);
    let claude = content(Operation::GenerateContent, Kind::ClaudeMessages);
    let request = convert_request(
        gemini,
        claude,
        json!({
            "model":"route",
            "contents":[
                {"role":"model","parts":[{
                    "functionCall":{"id":"call-1","name":"Read","args":{"path":"task.txt"}},
                    "thoughtSignature":"opaque-signature"
                }]},
                {"role":"user","parts":[{
                    "functionResponse":{"id":"call-1","name":"Read","response":{"output":"result"}}
                }]}
            ]
        }),
    );
    assert_eq!(
        request["messages"][0]["content"][0]["type"],
        "redacted_thinking"
    );
    assert_eq!(request["messages"][0]["content"][1]["type"], "tool_use");
    assert!(request["messages"][0]["content"][1].get("caller").is_none());
}

#[test]
fn gemini_synthetic_signature_is_not_sent_to_claude() {
    let gemini = content(Operation::GenerateContent, Kind::GeminiGenerateContent);
    let claude = content(Operation::GenerateContent, Kind::ClaudeMessages);
    let request = convert_request(
        gemini,
        claude,
        json!({
            "model":"route",
            "contents":[
                {"role":"model","parts":[{
                    "functionCall":{"id":"call-1","name":"Read","args":{"path":"task.txt"}},
                    "thoughtSignature":"skip_thought_signature_validator"
                }]},
                {"role":"user","parts":[{
                    "functionResponse":{"id":"call-1","name":"Read","response":{"output":"result"}}
                }]}
            ]
        }),
    );
    assert_eq!(request["messages"][0]["content"][0]["type"], "tool_use");
    assert!(request["messages"][0]["content"][0].get("caller").is_none());
}

#[test]
fn claude_tool_signature_is_attached_to_the_gemini_function_call() {
    let gemini = content(Operation::GenerateContent, Kind::GeminiGenerateContent);
    let claude = content(Operation::GenerateContent, Kind::ClaudeMessages);
    let response = convert_response(
        gemini,
        claude,
        json!({
            "id":"msg-1","type":"message","role":"assistant","model":"claude",
            "content":[
                {"type":"thinking","thinking":"plan","signature":"opaque-signature"},
                {"type":"tool_use","id":"call-1","name":"Read","input":{"path":"task.txt"}}
            ],
            "stop_reason":"tool_use","stop_sequence":null,
            "usage":{"input_tokens":1,"output_tokens":2}
        }),
    );
    let parts = &response["candidates"][0]["content"]["parts"];
    assert_eq!(parts[0]["thoughtSignature"], "opaque-signature");
    assert_eq!(parts[1]["functionCall"]["name"], "Read");
    assert_eq!(parts[1]["thoughtSignature"], "opaque-signature");
}

#[test]
fn streamed_gemini_tool_signature_is_exposed_as_claude_thinking() {
    let mut stream = ResponseStream::new(
        content(Operation::StreamGenerateContent, Kind::ClaudeMessages),
        content(
            Operation::StreamGenerateContent,
            Kind::GeminiGenerateContent,
        ),
    )
    .unwrap();
    let output = stream
        .push(Bytes::from_static(
            br#"data: {"responseId":"response-1","modelVersion":"gemini","candidates":[{"content":{"role":"model","parts":[{"functionCall":{"id":"call-1","name":"Read","args":{"path":"task.txt"}},"thoughtSignature":"opaque-signature"}]},"finishReason":"STOP"}]}

"#,
        ))
        .unwrap();
    let wire = String::from_utf8(output.concat()).unwrap();
    assert!(wire.contains("\"type\":\"redacted_thinking\""));
    assert!(wire.contains("opaque-signature"));
    assert!(wire.contains("\"type\":\"tool_use\""));
}

#[test]
fn streamed_claude_signature_is_attached_to_the_gemini_function_call() {
    let mut stream = ResponseStream::new(
        content(
            Operation::StreamGenerateContent,
            Kind::GeminiGenerateContent,
        ),
        content(Operation::StreamGenerateContent, Kind::ClaudeMessages),
    )
    .unwrap();
    let wire = concat!(
        "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg-1\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":1,\"output_tokens\":0}}}\n\n",
        "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"thinking\",\"thinking\":\"\",\"signature\":\"\"}}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"plan\"}}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"signature_delta\",\"signature\":\"opaque-signature\"}}\n\n",
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
        "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"call-1\",\"name\":\"Read\",\"input\":{}}}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"path\\\":\\\"task.txt\\\"}\"}}\n\n",
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":1}\n\n",
        "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\",\"stop_sequence\":null},\"usage\":{\"output_tokens\":2}}\n\n",
        "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
    );
    let output = stream.push(Bytes::from_static(wire.as_bytes())).unwrap();
    let wire = String::from_utf8(output.concat()).unwrap();
    let tool = wire
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|value| {
            value["candidates"][0]["content"]["parts"][0]
                .get("functionCall")
                .is_some()
        })
        .unwrap();
    let part = &tool["candidates"][0]["content"]["parts"][0];
    assert_eq!(part["functionCall"]["name"], "Read");
    assert_eq!(part["thoughtSignature"], "opaque-signature", "{wire}");
}
