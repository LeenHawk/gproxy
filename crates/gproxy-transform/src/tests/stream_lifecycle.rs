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

#[test]
fn public_collector_handles_split_tool_stream_and_rejects_incomplete_lifecycle() {
    let wire = concat!(
        "data: {\"id\":\"chat_tool\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"gpt\",\"trace\":\"a\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"lookup\",\"arguments\":\"{\"}}]},\"finish_reason\":null}]}\n\n",
        "data: {\"id\":\"chat_tool\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"gpt\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"} \"}}]},\"finish_reason\":\"tool_calls\"}],\"usage\":{\"prompt_tokens\":2,\"completion_tokens\":1,\"total_tokens\":3}}\n\n",
        "data: [DONE]\n\n"
    );
    let mut collector = ResponseCollector::new(Kind::OpenAiChat).unwrap();
    for chunk in wire.as_bytes().chunks(11) {
        collector.push(Bytes::copy_from_slice(chunk)).unwrap();
    }
    assert!(collector.is_complete());
    let BufferedResponse::OpenAiChat(response) = collector.finish().unwrap() else {
        panic!("wrong buffered family");
    };
    let call = response.choices[0].message.tool_calls.as_ref().unwrap();
    let gproxy_protocol::openai::ChatToolCall::Function(call) = &call[0] else {
        panic!("wrong tool call type");
    };
    assert_eq!(call.function.name, "lookup");
    assert_eq!(response.usage.as_ref().unwrap().total_tokens, 3);
    assert_eq!(response.rest["trace"], "a");

    let mut incomplete = ResponseCollector::new(Kind::OpenAiChat).unwrap();
    incomplete
        .push(Bytes::from_static(
            b"data: {\"id\":\"x\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"gpt\",\"choices\":[]}\n\n",
        ))
        .unwrap();
    assert!(incomplete.finish().is_err());

    let mut false_stop = ResponseCollector::new(Kind::OpenAiChat).unwrap();
    false_stop
        .push(Bytes::from_static(
            b"data: {\"id\":\"x\",\"object\":\"chat.completion.chunk\",\"model\":\"gpt\",\"choices\":[]}\n\ndata: [DONE]\n\n",
        ))
        .unwrap();
    assert!(false_stop.finish().is_err());

    let false_end_turn_wire = Bytes::from_static(
        b"event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":1,\"output_tokens\":0}}}\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
    );
    let mut false_end_turn = ResponseCollector::new(Kind::ClaudeMessages).unwrap();
    false_end_turn.push(false_end_turn_wire.clone()).unwrap();
    assert!(false_end_turn.finish().is_err());
    let mut transformed = ResponseStream::new(
        content(
            Operation::StreamGenerateContent,
            Kind::GeminiGenerateContent,
        ),
        content(Operation::StreamGenerateContent, Kind::ClaudeMessages),
    )
    .unwrap();
    transformed.push(false_end_turn_wire).unwrap();
    assert!(transformed.finish().is_err());
}

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
                    {"functionCall":{"id":"second","name":"lookup","args":{}}}
                ]},
                {"role":"user","parts":[
                    {"functionResponse":{"id":"first","name":"lookup","response":{"ok":1}}},
                    {"functionResponse":{"name":"lookup","response":{"ok":2}}}
                ]}
            ]
        }),
    );
    assert_eq!(correlated["messages"][1]["tool_call_id"], "first");
    assert_eq!(correlated["messages"][2]["tool_call_id"], "second");
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
    assert_eq!(chat_usage["usage"]["completion_tokens"], 7);
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
    assert_eq!(gemini_usage["usageMetadata"]["candidatesTokenCount"], 5);
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
    assert_eq!(future_chat["service_tier"], "future-tier");
    assert_eq!(future_chat["reasoning_effort"], "FUTURE");
    let future_gemini = convert_request(
        chat,
        gemini,
        json!({
            "model":"gpt","messages":[{"role":"user","content":"go"}],
            "service_tier":"future-tier","reasoning_effort":"future-effort"
        }),
    );
    assert_eq!(future_gemini["serviceTier"], "future-tier");
    assert_eq!(
        future_gemini["generationConfig"]["thinkingConfig"]["thinkingLevel"],
        "future-effort"
    );

    let bad_chat_usage = response(
        gemini,
        chat,
        Bytes::from_static(
            br#"{"id":"bad","object":"chat.completion","model":"gpt","choices":[{"index":0,"finish_reason":"stop","message":{"role":"assistant"}}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2,"completion_tokens_details":{"reasoning_tokens":2}}}"#,
        ),
    );
    assert!(matches!(
        bad_chat_usage,
        Err(TransformError::InvalidShape { .. })
    ));
    let bad_gemini_usage = response(
        chat,
        gemini,
        Bytes::from_static(
            br#"{"responseId":"bad","modelVersion":"gemini","candidates":[{"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":1,"candidatesTokenCount":1,"totalTokenCount":9}}"#,
        ),
    );
    assert!(matches!(
        bad_gemini_usage,
        Err(TransformError::InvalidShape { .. })
    ));

    let unspecified = response(
        chat,
        gemini,
        Bytes::from_static(
            br#"{"responseId":"bad","modelVersion":"gemini","candidates":[{"content":{"parts":[{"text":"bad"}]},"finishReason":"FINISH_REASON_UNSPECIFIED"}],"usageMetadata":{"promptTokenCount":1,"candidatesTokenCount":1,"totalTokenCount":2}}"#,
        ),
    );
    assert!(matches!(
        unspecified,
        Err(TransformError::Unsupported { .. })
    ));
    let mut unspecified_stream = ResponseStream::new(
        stream(Kind::OpenAiChat),
        stream(Kind::GeminiGenerateContent),
    )
    .unwrap();
    assert!(matches!(
        unspecified_stream.push(Bytes::from_static(
            b"data: {\"responseId\":\"bad\",\"modelVersion\":\"gemini\",\"candidates\":[{\"index\":0,\"finishReason\":\"FINISH_REASON_UNSPECIFIED\"}]}\n\n"
        )),
        Err(TransformError::Unsupported { .. })
    ));

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
    assert!(matches!(bad_mcp, Err(TransformError::Unsupported { .. })));

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
    assert!(matches!(
        multi_candidate,
        Err(TransformError::Unsupported { .. })
    ));
    let missing_incomplete = response(
        gemini,
        responses,
        Bytes::from_static(
            br#"{"id":"incomplete","object":"response","status":"incomplete","output":[]}"#,
        ),
    );
    assert!(matches!(
        missing_incomplete,
        Err(TransformError::InvalidShape { .. })
    ));
    let unknown_incomplete = convert_response(
        gemini,
        responses,
        json!({
            "id":"future","object":"response","status":"incomplete",
            "incomplete_details":{"reason":"future_limit"},"output":[]
        }),
    );
    assert_eq!(
        unknown_incomplete["candidates"][0]["finishReason"],
        "future_limit"
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

/// A live Responses stream carries fields Chat Completions has no slot for —
/// `logprobs` and `annotations` ride on every `output_text`, Codex attaches
/// `encrypted_content` to reasoning, and vendors add event types continuously.
/// Refusing any of them used to kill the reply mid-flight, which took the whole
/// Codex channel down.
#[test]
fn responses_stream_survives_fields_and_events_chat_cannot_express() {
    let chat = content(Operation::StreamGenerateContent, Kind::OpenAiChat);
    let responses = content(Operation::StreamGenerateContent, Kind::OpenAiResponses);
    let stream = ResponseStream::new(chat, responses).unwrap();
    let wire = concat!(
        "data: {\"type\":\"response.created\",\"sequence_number\":0,\"response\":{\"id\":\"resp_1\",\"object\":\"response\",\"created_at\":0,\"status\":\"in_progress\",\"model\":\"gpt-5.5\",\"output\":[]}}\n\n",
        "data: {\"type\":\"response.queued\",\"sequence_number\":1,\"response\":{\"id\":\"resp_1\",\"object\":\"response\",\"created_at\":0,\"status\":\"queued\",\"model\":\"gpt-5.5\",\"output\":[]}}\n\n",
        "data: {\"type\":\"response.something.new\",\"sequence_number\":2}\n\n",
        "data: {\"type\":\"response.output_text.delta\",\"sequence_number\":3,\"item_id\":\"msg_1\",\"output_index\":0,\"content_index\":0,\"delta\":\"Hello\",\"logprobs\":[]}\n\n",
        "data: {\"type\":\"response.output_item.done\",\"sequence_number\":4,\"output_index\":0,\"item\":{\"type\":\"reasoning\",\"id\":\"rs_1\",\"summary\":[],\"encrypted_content\":\"opaque\"}}\n\n",
        "data: {\"type\":\"response.completed\",\"sequence_number\":5,\"response\":{\"id\":\"resp_1\",\"object\":\"response\",\"created_at\":0,\"status\":\"completed\",\"model\":\"gpt-5.5\",\"output\":[{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"assistant\",\"status\":\"completed\",\"content\":[{\"type\":\"output_text\",\"text\":\"Hello\",\"annotations\":[],\"logprobs\":[]}]}],\"usage\":{\"input_tokens\":1,\"output_tokens\":1,\"total_tokens\":2}}}\n\n",
    );
    let text = String::from_utf8(drive(stream, wire, 17)).unwrap();
    assert!(text.contains("Hello"), "text was dropped: {text}");
    assert!(
        text.contains("\"finish_reason\":\"stop\""),
        "no terminal: {text}"
    );
}

#[test]
fn responses_stream_survives_items_and_events_claude_cannot_express() {
    let claude = content(Operation::StreamGenerateContent, Kind::ClaudeMessages);
    let responses = content(Operation::StreamGenerateContent, Kind::OpenAiResponses);
    let stream = ResponseStream::new(claude, responses).unwrap();
    let wire = concat!(
        "data: {\"type\":\"response.created\",\"sequence_number\":0,\"response\":{\"id\":\"resp_1\",\"object\":\"response\",\"created_at\":0,\"status\":\"in_progress\",\"model\":\"gpt-5.5\",\"output\":[]}}\n\n",
        "data: {\"type\":\"response.something.new\",\"sequence_number\":1}\n\n",
        "data: {\"type\":\"response.image_generation_call.in_progress\",\"sequence_number\":2,\"item_id\":\"image_1\",\"output_index\":0}\n\n",
        "data: {\"type\":\"response.output_item.added\",\"sequence_number\":3,\"output_index\":0,\"item\":{\"type\":\"image_generation_call\",\"id\":\"image_1\",\"result\":null,\"status\":\"in_progress\"}}\n\n",
        "data: {\"type\":\"response.output_item.done\",\"sequence_number\":4,\"output_index\":0,\"item\":{\"type\":\"image_generation_call\",\"id\":\"image_1\",\"result\":\"aW1hZ2U=\",\"status\":\"completed\"}}\n\n",
        "data: {\"type\":\"response.output_text.delta\",\"sequence_number\":5,\"item_id\":\"msg_1\",\"output_index\":1,\"content_index\":0,\"delta\":\"Done\",\"logprobs\":[]}\n\n",
        "data: {\"type\":\"response.completed\",\"sequence_number\":6,\"response\":{\"id\":\"resp_1\",\"object\":\"response\",\"created_at\":0,\"status\":\"completed\",\"model\":\"gpt-5.5\",\"output\":[{\"type\":\"image_generation_call\",\"id\":\"image_1\",\"result\":\"aW1hZ2U=\",\"status\":\"completed\"},{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"assistant\",\"status\":\"completed\",\"content\":[{\"type\":\"output_text\",\"text\":\"Done\",\"annotations\":[],\"logprobs\":[]}]}],\"usage\":{\"input_tokens\":1,\"output_tokens\":1,\"total_tokens\":2}}}\n\n",
    );
    let text = String::from_utf8(drive(stream, wire, 23)).unwrap();
    assert!(text.contains("Done"), "text was dropped: {text}");
    assert!(text.contains("message_stop"), "no terminal: {text}");
}

#[test]
fn responses_stream_survives_items_and_events_gemini_cannot_express() {
    let gemini = content(
        Operation::StreamGenerateContent,
        Kind::GeminiGenerateContent,
    );
    let responses = content(Operation::StreamGenerateContent, Kind::OpenAiResponses);
    let stream = ResponseStream::new(gemini, responses).unwrap();
    let wire = concat!(
        "data: {\"type\":\"response.created\",\"sequence_number\":0,\"response\":{\"id\":\"resp_1\",\"object\":\"response\",\"created_at\":0,\"status\":\"in_progress\",\"model\":\"gpt-5.5\",\"output\":[]}}\n\n",
        "data: {\"type\":\"response.something.new\",\"sequence_number\":1}\n\n",
        "data: {\"type\":\"response.image_generation_call.in_progress\",\"sequence_number\":2,\"item_id\":\"image_1\",\"output_index\":0}\n\n",
        "data: {\"type\":\"response.output_item.added\",\"sequence_number\":3,\"output_index\":0,\"item\":{\"type\":\"image_generation_call\",\"id\":\"image_1\",\"result\":null,\"status\":\"in_progress\"}}\n\n",
        "data: {\"type\":\"response.output_item.done\",\"sequence_number\":4,\"output_index\":0,\"item\":{\"type\":\"image_generation_call\",\"id\":\"image_1\",\"result\":\"aW1hZ2U=\",\"status\":\"completed\"}}\n\n",
        "data: {\"type\":\"response.output_text.delta\",\"sequence_number\":5,\"item_id\":\"msg_1\",\"output_index\":1,\"content_index\":0,\"delta\":\"Done\",\"logprobs\":[]}\n\n",
        "data: {\"type\":\"response.completed\",\"sequence_number\":6,\"response\":{\"id\":\"resp_1\",\"object\":\"response\",\"created_at\":0,\"status\":\"completed\",\"model\":\"gpt-5.5\",\"output\":[{\"type\":\"image_generation_call\",\"id\":\"image_1\",\"result\":\"aW1hZ2U=\",\"status\":\"completed\"},{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"assistant\",\"status\":\"completed\",\"content\":[{\"type\":\"output_text\",\"text\":\"Done\",\"annotations\":[],\"logprobs\":[]}]}],\"usage\":{\"input_tokens\":1,\"output_tokens\":1,\"total_tokens\":2}}}\n\n",
    );
    let text = String::from_utf8(drive(stream, wire, 29)).unwrap();
    assert!(text.contains("Done"), "text was dropped: {text}");
    assert!(text.contains("STOP"), "no terminal: {text}");
}

#[test]
fn claude_stream_survives_blocks_and_events_gemini_cannot_express() {
    let gemini = content(
        Operation::StreamGenerateContent,
        Kind::GeminiGenerateContent,
    );
    let claude = content(Operation::StreamGenerateContent, Kind::ClaudeMessages);
    let stream = ResponseStream::new(gemini, claude).unwrap();
    let wire = concat!(
        "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-opus\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":1,\"output_tokens\":0}}}\n\n",
        "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"redacted_thinking\",\"data\":\"opaque\"}}\n\n",
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
        "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"text\",\"text\":\"\",\"citations\":[]}}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"citations_delta\",\"citation\":{\"type\":\"char_location\",\"cited_text\":\"Hello\",\"document_index\":0,\"start_char_index\":0,\"end_char_index\":5}}}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"future_delta\",\"future\":true}}\n\n",
        "event: future_event\ndata: {\"type\":\"future_event\",\"future\":true}\n\n",
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":1}\n\n",
        "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":2,\"content_block\":{\"type\":\"tool_use\",\"id\":\"call_1\",\"name\":\"lookup\",\"input\":{},\"caller\":{\"type\":\"direct\"}}}\n\n",
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":2}\n\n",
        "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\",\"stop_sequence\":null},\"usage\":{\"output_tokens\":1}}\n\n",
        "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
    );
    let text = String::from_utf8(drive(stream, wire, 31)).unwrap();
    assert!(text.contains("Hello"), "text was dropped: {text}");
    assert!(text.contains("STOP"), "no terminal: {text}");
}

#[test]
fn gemini_response_uses_first_candidate_and_ignores_claude_unmapped_fields() {
    let body = response(
        content(Operation::GenerateContent, Kind::ClaudeMessages),
        content(Operation::GenerateContent, Kind::GeminiGenerateContent),
        Bytes::from_static(
            br#"{"responseId":"gemini_1","modelVersion":"gemini-3-flash","candidates":[{"index":0,"finishReason":"STOP","content":{"role":"model","future_content":true,"parts":[{"text":"first","thought":false,"thoughtSignature":"opaque","partMetadata":{"state":"live"}}]}},{"index":1,"finishReason":"STOP","content":{"role":"model","parts":[{"text":"second"}]}}],"usageMetadata":{"promptTokenCount":1,"candidatesTokenCount":1,"totalTokenCount":2}}"#,
        ),
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(value["content"][0]["text"], "first");
    assert!(
        !body
            .windows("second".len())
            .any(|window| window == b"second")
    );

    let stream = ResponseStream::new(
        content(Operation::StreamGenerateContent, Kind::ClaudeMessages),
        content(
            Operation::StreamGenerateContent,
            Kind::GeminiGenerateContent,
        ),
    )
    .unwrap();
    let wire = "data: {\"responseId\":\"gemini_1\",\"modelVersion\":\"gemini-3-flash\",\"candidates\":[{\"index\":0,\"finishReason\":\"STOP\",\"content\":{\"role\":\"model\",\"parts\":[{\"text\":\"first\",\"thought\":false,\"thoughtSignature\":\"opaque\",\"partMetadata\":{\"state\":\"live\"}}]}}],\"usageMetadata\":{\"promptTokenCount\":1,\"candidatesTokenCount\":1,\"totalTokenCount\":2}}\n\n";
    let text = String::from_utf8(drive(stream, wire, 17)).unwrap();
    assert!(text.contains("first"), "stream text was dropped: {text}");
    assert!(text.contains("message_stop"), "no stream terminal: {text}");
}
