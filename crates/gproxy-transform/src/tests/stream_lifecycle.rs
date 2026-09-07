use bytes::Bytes;
use gproxy_protocol::{ContentGenerationKind as Kind, Operation};
use serde_json::json;

use super::bytes_text;
use super::support::drive;
use super::{
    BufferedResponse, ResponseCollector, ResponseStream, can_transform, content, convert_request,
    convert_response, request, response,
};
use crate::TransformError;

mod anthropic;
mod collector;
mod order;
mod responses;

#[test]
fn transformed_streams_emit_nonterminal_text_and_tool_deltas_immediately() {
    let chat = content(Operation::StreamGenerateContent, Kind::OpenAiChat);
    let claude = content(Operation::StreamGenerateContent, Kind::ClaudeMessages);
    let mut to_chat = ResponseStream::new(chat, claude).unwrap();
    let start = Bytes::from_static(
        b"event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_live\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-opus\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":1,\"output_tokens\":0}}}\n\n",
    );
    assert!(!to_chat.push(start).unwrap().is_empty());
    let tool_start = Bytes::from_static(
        b"event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"call_live\",\"name\":\"lookup\",\"input\":{}}}\n\n",
    );
    let output = to_chat.push(tool_start).unwrap();
    assert!(bytes_text(&output).contains("lookup"));
    let tool_delta = Bytes::from_static(
        b"event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"q\\\":1}\"}}\n\n",
    );
    let output = to_chat.push(tool_delta).unwrap();
    assert!(bytes_text(&output).contains("arguments"));

    let mut to_claude = ResponseStream::new(claude, chat).unwrap();
    let text = Bytes::from_static(
        b"data: {\"id\":\"chat_live\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"gpt\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"now\"},\"finish_reason\":null}]}\n\n",
    );
    let output = to_claude.push(text).unwrap();
    let output = bytes_text(&output);
    assert!(output.contains("message_start"));
    assert!(output.contains("text_delta"));
    assert!(output.contains("now"));
}

#[test]
fn gemini_pairs_register_streams_and_preserve_native_code_ids() {
    let gemini = content(Operation::GenerateContent, Kind::GeminiGenerateContent);
    let chat = content(Operation::GenerateContent, Kind::OpenAiChat);
    let stream = |kind| content(Operation::StreamGenerateContent, kind);
    for peer_kind in [
        Kind::OpenAiChat,
        Kind::OpenAiResponses,
        Kind::ClaudeMessages,
    ] {
        let peer = content(Operation::GenerateContent, peer_kind);
        assert!(can_transform(gemini, peer));
        assert!(can_transform(peer, gemini));
        assert!(
            ResponseStream::new(stream(peer_kind), stream(Kind::GeminiGenerateContent)).is_ok()
        );
        assert!(
            ResponseStream::new(stream(Kind::GeminiGenerateContent), stream(peer_kind)).is_ok()
        );
    }

    let responses = content(Operation::GenerateContent, Kind::OpenAiResponses);
    let native = convert_request(
        responses,
        gemini,
        json!({
            "model":"route","max_output_tokens":16,
            "input":[
                {"type":"shell_call","call_id":"code_1","action":{"commands":["print(1)"]},"status":"completed"},
                {"type":"shell_call_output","call_id":"code_1","output":[{
                    "outcome":{"type":"exit","exit_code":0},"stdout":"1","stderr":""
                }]}
            ]
        }),
    );
    assert_eq!(
        native.pointer("/contents/0/parts/0/executableCode/id"),
        Some(&json!("code_1"))
    );
    assert_eq!(
        native.pointer("/contents/1/parts/0/codeExecutionResult/id"),
        Some(&json!("code_1"))
    );

    let outward = convert_request(
        gemini,
        responses,
        json!({
            "model":"models/gemini","contents":[
                {"role":"model","parts":[{"executableCode":{
                    "id":"code_2","language":"PYTHON","code":"print(2)"
                }}]},
                {"role":"user","parts":[{"codeExecutionResult":{
                    "id":"code_2","outcome":"OUTCOME_OK","output":"2"
                }}]}
            ],
            "generationConfig":{"maxOutputTokens":16}
        }),
    );
    assert_eq!(outward.pointer("/input/0/call_id"), Some(&json!("code_2")));
    assert_eq!(outward.pointer("/input/1/call_id"), Some(&json!("code_2")));

    let correlated = convert_request(
        gemini,
        chat,
        json!({
            "contents":[
                {"role":"model","parts":[
                    {"functionCall":{"id":"first","name":"lookup","args":{}}},
                    {"functionCall":{"id":"second","name":"lookup","args":{}}},
                    {"functionCall":{"id":"third","name":"lookup","args":{}}}
                ]},
                {"role":"user","parts":[
                    {"functionResponse":{"id":"first","name":"lookup","response":{"ok":1}}},
                    {"functionResponse":{"id":"first","name":"lookup","response":{"ok":1}}},
                    {"functionResponse":{"name":"lookup","response":{"ok":2}}},
                    {"functionResponse":{"name":"lookup","response":{"ok":2}}}
                ]}
            ]
        }),
    );
    assert_eq!(correlated["messages"][1]["tool_call_id"], "first");
    assert_eq!(correlated["messages"][2]["tool_call_id"], "second");
    assert_eq!(correlated["messages"][3]["tool_call_id"], "third");
    assert_eq!(correlated["messages"].as_array().unwrap().len(), 4);
    let orphan = request(
        gemini,
        chat,
        Bytes::from_static(
            br#"{"contents":[{"role":"user","parts":[{"functionResponse":{"name":"missing","response":{"ok":false}}}]}]}"#,
        ),
        "upstream-model",
        false,
    );
    assert!(matches!(orphan, Err(TransformError::InvalidShape { .. })));

    let chat_usage = convert_response(
        chat,
        gemini,
        json!({
            "responseId":"usage","modelVersion":"gemini",
            "candidates":[{"index":0,"content":{"role":"model","parts":[{"text":"ok"}]},"finishReason":"STOP"}],
            "usageMetadata":{"promptTokenCount":10,"candidatesTokenCount":5,"thoughtsTokenCount":2,"totalTokenCount":17}
        }),
    );
    assert_eq!(chat_usage["usage"]["completion_tokens"], 5);
    assert_eq!(
        chat_usage["usage"]["completion_tokens_details"]["reasoning_tokens"],
        2
    );
    let gemini_usage = convert_response(
        gemini,
        chat,
        json!({
            "id":"usage","object":"chat.completion","model":"gpt",
            "choices":[{"index":0,"finish_reason":"stop","message":{"role":"assistant","content":"ok"}}],
            "usage":{"prompt_tokens":10,"completion_tokens":7,"total_tokens":17,"completion_tokens_details":{"reasoning_tokens":2}}
        }),
    );
    assert_eq!(gemini_usage["usageMetadata"]["candidatesTokenCount"], 7);
    assert_eq!(gemini_usage["usageMetadata"]["thoughtsTokenCount"], 2);

    let multi = convert_request(
        gemini,
        chat,
        json!({
            "contents":[{"role":"user","parts":[{"text":"go"}]}],
            "toolConfig":{"functionCallingConfig":{"mode":"ANY","allowedFunctionNames":["first","second"]}}
        }),
    );
    assert_eq!(
        multi["tool_choice"]["allowed_tools"]["tools"]
            .as_array()
            .map(Vec::len),
        Some(2)
    );
    let multi_back = convert_request(chat, gemini, multi);
    assert_eq!(
        multi_back["toolConfig"]["functionCallingConfig"]["allowedFunctionNames"],
        json!(["first", "second"])
    );

    let future_chat = convert_request(
        gemini,
        chat,
        json!({
            "contents":[{"role":"user","parts":[{"text":"go"}]}],
            "serviceTier":"future-tier",
            "generationConfig":{"thinkingConfig":{"thinkingLevel":"FUTURE"}}
        }),
    );
    assert!(future_chat.get("service_tier").is_none());
    assert!(future_chat.get("reasoning_effort").is_none());
    let future_gemini = convert_request(
        chat,
        gemini,
        json!({
            "model":"gpt","messages":[{"role":"user","content":"go"}],
            "service_tier":"future-tier","reasoning_effort":"future-effort"
        }),
    );
    assert!(future_gemini.get("serviceTier").is_none());
    assert!(
        future_gemini
            .pointer("/generationConfig/thinkingConfig/thinkingLevel")
            .is_none()
    );

    let bad_chat_usage = response(
        gemini,
        chat,
        Bytes::from_static(
            br#"{"id":"bad","object":"chat.completion","model":"gpt","choices":[{"index":0,"finish_reason":"stop","message":{"role":"assistant"}}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2,"completion_tokens_details":{"reasoning_tokens":2}}}"#,
        ),
    );
    let bad_chat_usage: serde_json::Value =
        serde_json::from_slice(&bad_chat_usage.unwrap()).unwrap();
    assert_eq!(bad_chat_usage["usageMetadata"]["candidatesTokenCount"], 1);
    assert_eq!(bad_chat_usage["usageMetadata"]["thoughtsTokenCount"], 2);
    let bad_gemini_usage = response(
        chat,
        gemini,
        Bytes::from_static(
            br#"{"responseId":"bad","modelVersion":"gemini","candidates":[{"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":1,"candidatesTokenCount":1,"totalTokenCount":9}}"#,
        ),
    );
    let bad_gemini_usage: serde_json::Value =
        serde_json::from_slice(&bad_gemini_usage.unwrap()).unwrap();
    assert_eq!(bad_gemini_usage["usage"]["completion_tokens"], 1);
    assert_eq!(bad_gemini_usage["usage"]["total_tokens"], 9);

    let unspecified = response(
        chat,
        gemini,
        Bytes::from_static(
            br#"{"responseId":"bad","modelVersion":"gemini","candidates":[{"content":{"parts":[{"text":"bad"}]},"finishReason":"FINISH_REASON_UNSPECIFIED"}],"usageMetadata":{"promptTokenCount":1,"candidatesTokenCount":1,"totalTokenCount":2}}"#,
        ),
    );
    let unspecified: serde_json::Value = serde_json::from_slice(&unspecified.unwrap()).unwrap();
    assert_eq!(unspecified["choices"][0]["finish_reason"], "stop");
    let mut unspecified_stream = ResponseStream::new(
        stream(Kind::OpenAiChat),
        stream(Kind::GeminiGenerateContent),
    )
    .unwrap();
    assert!(!unspecified_stream
        .push(Bytes::from_static(
            b"data: {\"responseId\":\"bad\",\"modelVersion\":\"gemini\",\"candidates\":[{\"index\":0,\"finishReason\":\"FINISH_REASON_UNSPECIFIED\"}]}\n\n"
        ))
        .unwrap()
        .is_empty());

    let bad_top_k = request(
        gemini,
        responses,
        Bytes::from_static(
            br#"{"contents":[],"tools":[{"fileSearch":{"fileSearchStoreNames":["stores/1"],"topK":-1}}]}"#,
        ),
        "upstream-model",
        false,
    );
    assert!(matches!(
        bad_top_k,
        Err(TransformError::InvalidShape { .. })
    ));
    let bad_mcp = request(
        gemini,
        responses,
        Bytes::from_static(
            br#"{"contents":[],"tools":[{"mcpServers":[{"name":"remote","streamableHttpTransport":{"url":"https://mcp.invalid","timeout":"1s"}}]}]}"#,
        ),
        "upstream-model",
        false,
    );
    let bad_mcp: serde_json::Value = serde_json::from_slice(&bad_mcp.unwrap()).unwrap();
    assert_eq!(bad_mcp["tools"][0]["type"], "mcp");
    assert_eq!(bad_mcp["tools"][0]["server_url"], "https://mcp.invalid");

    let multipart = convert_request(
        responses,
        gemini,
        json!({
            "model":"gpt","max_output_tokens":16,
            "input":[
                {"type":"function_call","id":"fc_media","call_id":"media","name":"inspect","arguments":"{}"},
                {"type":"function_call_output","call_id":"media","output":[
                    {"type":"input_text","text":"{\"ok\":true}"},
                    {"type":"input_image","image_url":"data:image/png;base64,aW1hZ2U="}
                ]}
            ]
        }),
    );
    assert_eq!(
        multipart.pointer("/contents/1/parts/0/functionResponse/response/ok"),
        Some(&json!(true))
    );
    assert_eq!(
        multipart.pointer("/contents/1/parts/0/functionResponse/parts/0/inlineData/mimeType"),
        Some(&json!("image/png"))
    );

    let nonterminal_local = request(
        responses,
        gemini,
        Bytes::from_static(
            br#"{"model":"gpt","max_output_tokens":16,"input":[{"type":"local_shell_call","id":"local_item","call_id":"local_call","action":{"type":"exec","command":["pwd"],"env":{}},"status":"completed"},{"type":"local_shell_call_output","id":"local_item","output":"pwd"}]}"#,
        ),
        "upstream-model",
        false,
    );
    assert!(matches!(
        nonterminal_local,
        Err(TransformError::InvalidShape { .. })
    ));

    let multi_candidate = response(
        responses,
        gemini,
        Bytes::from_static(
            br#"{"responseId":"multi","candidates":[{"index":0,"finishReason":"STOP"},{"index":0,"finishReason":"STOP"}]}"#,
        ),
    );
    let multi_candidate: serde_json::Value =
        serde_json::from_slice(&multi_candidate.unwrap()).unwrap();
    assert_eq!(multi_candidate["status"], "completed");
    let missing_incomplete = response(
        gemini,
        responses,
        Bytes::from_static(
            br#"{"id":"incomplete","object":"response","status":"incomplete","output":[]}"#,
        ),
    );
    let missing_incomplete: serde_json::Value =
        serde_json::from_slice(&missing_incomplete.unwrap()).unwrap();
    assert_eq!(
        missing_incomplete["candidates"][0]["finishReason"],
        "MAX_TOKENS"
    );
    let unknown_incomplete = convert_response(
        gemini,
        responses,
        json!({
            "id":"future","object":"response","status":"incomplete",
            "incomplete_details":{"reason":"future_limit"},"output":[]
        }),
    );
    assert!(
        unknown_incomplete["candidates"][0]
            .get("finishReason")
            .is_none()
    );

    let mut after_finish = ResponseStream::new(
        stream(Kind::OpenAiResponses),
        stream(Kind::GeminiGenerateContent),
    )
    .unwrap();
    after_finish
        .push(Bytes::from_static(
            b"data: {\"responseId\":\"done\",\"candidates\":[{\"index\":0,\"finishReason\":\"STOP\"}]}\n\n",
        ))
        .unwrap();
    assert!(matches!(
        after_finish.push(Bytes::from_static(
            b"data: {\"responseId\":\"done\",\"candidates\":[{\"index\":0,\"content\":{\"role\":\"model\",\"parts\":[{\"text\":\"late\"}]}}]}\n\n",
        )),
        Err(TransformError::InvalidShape { .. })
    ));
    let mut after_terminal = ResponseStream::new(
        stream(Kind::GeminiGenerateContent),
        stream(Kind::OpenAiResponses),
    )
    .unwrap();
    after_terminal
        .push(Bytes::from_static(
            b"event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"done\",\"object\":\"response\",\"status\":\"completed\",\"output\":[]}}\n\n",
        ))
        .unwrap();
    assert!(matches!(
        after_terminal.push(Bytes::from_static(
            b"event: response.queued\ndata: {\"type\":\"response.queued\",\"response\":{\"id\":\"late\",\"object\":\"response\",\"status\":\"queued\",\"output\":[]}}\n\n",
        )),
        Err(TransformError::InvalidShape { .. })
    ));

    let stream_chunk = concat!(
        "data: {\"responseId\":\"resp_gemini\",\"modelVersion\":\"gemini\",",
        "\"candidates\":[{\"index\":0,\"content\":{\"role\":\"model\",\"parts\":[{\"text\":\"ok\"}]},\"finishReason\":\"STOP\"}],",
        "\"usageMetadata\":{\"promptTokenCount\":1,\"candidatesTokenCount\":1,\"totalTokenCount\":2}}\n\n"
    );
    for peer in [
        Kind::OpenAiChat,
        Kind::OpenAiResponses,
        Kind::ClaudeMessages,
    ] {
        let output = drive(
            ResponseStream::new(stream(peer), stream(Kind::GeminiGenerateContent)).unwrap(),
            stream_chunk,
            17,
        );
        assert!(!output.is_empty());
    }

    let mut collector = ResponseCollector::new(Kind::GeminiGenerateContent).unwrap();
    for chunk in stream_chunk.as_bytes().chunks(13) {
        collector.push(Bytes::copy_from_slice(chunk)).unwrap();
    }
    assert!(collector.is_complete());
    let BufferedResponse::Gemini(response) = collector.finish().unwrap() else {
        panic!("wrong buffered family");
    };
    assert_eq!(
        serde_json::to_value(response).unwrap()["candidates"][0]["content"]["parts"][0]["text"],
        "ok"
    );
    let mut incomplete = ResponseCollector::new(Kind::GeminiGenerateContent).unwrap();
    incomplete
        .push(Bytes::from_static(
            b"data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"cut\"}]}}]}\n\n",
        ))
        .unwrap();
    assert!(incomplete.finish().is_err());
}
