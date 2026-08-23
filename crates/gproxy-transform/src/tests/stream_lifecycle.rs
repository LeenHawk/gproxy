use bytes::Bytes;
use gproxy_protocol::{ContentGenerationKind as Kind, Operation};

use super::bytes_text;
use super::{BufferedResponse, ResponseCollector, ResponseStream, content};

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
