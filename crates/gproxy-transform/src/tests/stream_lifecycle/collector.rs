use bytes::Bytes;
use gproxy_protocol::{ContentGenerationKind as Kind, Operation};

use crate::{BufferedResponse, ResponseCollector, ResponseStream, TransformError};

use super::super::content;

#[test]
fn empty_claude_streams_preserve_refusal_and_distinguish_incomplete_streams() {
    use serde_json::json;
    for reason in ["refusal", "end_turn"] {
        let details = if reason == "refusal" {
            json!({"type":"refusal","category":null,"explanation":null})
        } else {
            serde_json::Value::Null
        };
        let events = [
            json!({"type":"message_start","message":{"id":"msg_empty","type":"message","role":"assistant","model":"claude-fable-5","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":10,"output_tokens":0}}}),
            json!({"type":"message_delta","delta":{"stop_reason":reason,"stop_sequence":null,"stop_details":details},"usage":{"output_tokens":0}}),
            json!({"type":"message_stop"}),
        ];
        let wire = events
            .iter()
            .map(|event| format!("data: {event}\n\n"))
            .collect::<String>();
        let mut collector = ResponseCollector::new(Kind::ClaudeMessages).unwrap();
        for chunk in wire.as_bytes().chunks(3) {
            collector.push(Bytes::copy_from_slice(chunk)).unwrap();
        }
        let response: serde_json::Value =
            serde_json::from_slice(&collector.finish().unwrap().into_bytes().unwrap()).unwrap();
        assert_eq!(response["stop_reason"], reason);
        assert_eq!(response["stop_details"], details);
        assert_eq!(response["content"], json!([]));
        let mut incomplete = ResponseCollector::new(Kind::ClaudeMessages).unwrap();
        incomplete
            .push(Bytes::from(format!("data: {}\n\n", events[0])))
            .unwrap();
        assert!(matches!(
            incomplete.finish(),
            Err(TransformError::IncompleteStream)
        ));
        let mut failed = ResponseCollector::new(Kind::ClaudeMessages).unwrap();
        assert!(failed.push(Bytes::from_static(b"data: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"busy\"}}\n\n")).is_err());
    }
}

#[test]
fn fallback_stream_preserves_content_usage_and_updates_serving_model() {
    use super::super::support::{data_frames, drive};
    use serde_json::json;

    for with_trigger in [true, false] {
        let mut events = [
            json!({"type":"message_start","message":{"id":"msg_fallback","type":"message","role":"assistant","model":"claude-fable-5","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":10,"output_tokens":0}}}),
            json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Before "}}),
            json!({"type":"content_block_stop","index":0}),
            json!({"type":"content_block_start","index":1,"content_block":{"type":"fallback","from":{"model":"claude-fable-5"},"to":{"model":"claude-opus-4-8"},"trigger":{"type":"refusal","category":"cyber"}}}),
            json!({"type":"content_block_stop","index":1}),
            json!({"type":"content_block_start","index":2,"content_block":{"type":"text","text":""}}),
            json!({"type":"content_block_delta","index":2,"delta":{"type":"text_delta","text":"after"}}),
            json!({"type":"content_block_stop","index":2}),
            json!({"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":null,"stop_details":null},"usage":{"input_tokens":8,"output_tokens":2,"iterations":[{"type":"message","model":"claude-fable-5","input_tokens":10,"output_tokens":1},{"type":"fallback_message","model":"claude-opus-4-8","input_tokens":8,"output_tokens":2}]}}),
            json!({"type":"message_stop"}),
        ];
        if !with_trigger {
            events[4]["content_block"]
                .as_object_mut()
                .unwrap()
                .remove("trigger");
        }
        let wire = events
            .iter()
            .map(|event| format!("data: {event}\n\n"))
            .collect::<String>();
        let mut collector = ResponseCollector::new(Kind::ClaudeMessages).unwrap();
        for chunk in wire.as_bytes().chunks(7) {
            collector.push(Bytes::copy_from_slice(chunk)).unwrap();
        }
        let BufferedResponse::Claude(response) = collector.finish().unwrap() else {
            panic!("wrong buffered family");
        };
        let response = serde_json::to_value(response).unwrap();
        assert_eq!(response["model"], "claude-opus-4-8");
        assert_eq!(response["content"][0]["text"], "Before ");
        assert_eq!(response["content"][1], events[4]["content_block"]);
        assert_eq!(response["content"][2]["text"], "after");
        assert_eq!(
            response["usage"]["iterations"],
            events[9]["usage"]["iterations"]
        );
        for kind in [Kind::OpenAiChat, Kind::OpenAiResponses] {
            let stream = ResponseStream::new(
                content(Operation::StreamGenerateContent, kind),
                content(Operation::StreamGenerateContent, Kind::ClaudeMessages),
            )
            .unwrap();
            let frames = data_frames(&drive(stream, &wire, 7));
            if kind == Kind::OpenAiChat {
                assert!(
                    frames
                        .iter()
                        .any(|frame| frame["choices"][0]["delta"]["content"] == "after"
                            && frame["model"] == "claude-opus-4-8")
                );
                assert_eq!(frames.last().unwrap()["model"], "claude-opus-4-8");
            } else {
                assert_eq!(
                    frames.last().unwrap()["response"]["model"],
                    "claude-opus-4-8"
                );
                assert!(frames.iter().any(|frame| frame["delta"] == "after"));
            }
        }
    }
}

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
    assert!(
        serde_json::to_value(response)
            .unwrap()
            .get("trace")
            .is_none()
    );

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
    assert!(!false_stop.is_complete());
    assert!(matches!(
        false_stop.finish(),
        Err(TransformError::IncompleteStream)
    ));

    let mut missing_reason = ResponseCollector::new(Kind::OpenAiChat).unwrap();
    missing_reason
        .push(Bytes::from_static(
            b"data: {\"id\":\"x\",\"object\":\"chat.completion.chunk\",\"model\":\"gpt\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"partial\"},\"finish_reason\":null}]}\n\ndata: [DONE]\n\n",
        ))
        .unwrap();
    assert!(!missing_reason.is_complete());
    assert!(matches!(
        missing_reason.finish(),
        Err(TransformError::IncompleteStream)
    ));

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
fn chat_collector_keeps_all_choices_refusal_and_legacy_calls() {
    let mut collector = ResponseCollector::new(Kind::OpenAiChat).unwrap();
    collector
        .push(Bytes::from_static(
            b"data: {\"id\":\"multi\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"gpt\",\"choices\":[{\"index\":1,\"delta\":{\"content\":\"second\",\"refusal\":\"no\"},\"finish_reason\":\"stop\"},{\"index\":0,\"delta\":{\"function_call\":{\"name\":\"legacy\",\"arguments\":\"{}\"}},\"finish_reason\":\"function_call\"}]}\n\ndata: [DONE]\n\n",
        ))
        .unwrap();
    let BufferedResponse::OpenAiChat(response) = collector.finish().unwrap() else {
        panic!("wrong response family");
    };
    assert_eq!(response.choices.len(), 2);
    assert_eq!(response.choices[0].index, 0);
    assert_eq!(
        response.choices[0]
            .message
            .function_call
            .as_ref()
            .unwrap()
            .name,
        "legacy"
    );
    assert_eq!(response.choices[1].index, 1);
    assert_eq!(
        response.choices[1].message.content.as_deref(),
        Some("second")
    );
    assert_eq!(response.choices[1].message.refusal.as_deref(), Some("no"));
}

#[test]
fn responses_collector_keeps_partial_web_search_call_typed() {
    let mut collector = ResponseCollector::new(Kind::OpenAiResponses).unwrap();
    collector
        .push(Bytes::from_static(
            b"data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"web_search_call\",\"id\":\"ws_1\",\"status\":\"in_progress\"}}\n\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"object\":\"response\",\"output\":[{\"type\":\"web_search_call\",\"id\":\"ws_1\",\"status\":\"completed\"}]}}\n\n",
        ))
        .unwrap();
    let BufferedResponse::OpenAiResponses(response) = collector.finish().unwrap() else {
        panic!("wrong response family");
    };
    assert!(matches!(
        &response.output[0],
        gproxy_protocol::openai::ResponseItem::Typed(item)
            if matches!(item.as_ref(), gproxy_protocol::openai::TypedResponseItem::WebSearchCall { action: None, .. })
    ));
}
