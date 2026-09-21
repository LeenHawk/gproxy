use gproxy_protocol::{
    Dialect,
    transform::{
        generate::{gemini_chat::stream::*, stream::gemini::GeminiStreamCollector},
        identity::{IdNamespace, IdSyntax, IdentityFlow, IdentityRole, TargetIdPolicy},
    },
    wire::openai::chat::stream::ChatCompletionChunk,
};
use serde_json::json;
fn flow() -> IdentityFlow {
    IdentityFlow::new(IdNamespace::with_bytes([92; 16]))
}
#[test]
fn chat_to_gemini_applies_policy_to_response_and_tool_before_exposure() {
    let policy = TargetIdPolicy::new(Dialect::Gemini).with_syntax(IdSyntax::AsciiIdentifier);
    let mut stream =
        ChatToGeminiStream::new_with_policy(flow(), Default::default(), policy.clone()).unwrap();
    let chunk: ChatCompletionChunk = serde_json::from_value(json!({
        "id":"response:invalid", "object":"chat.completion.chunk", "created":7, "model":"m",
        "choices":[{"index":0,"delta":{"role":"assistant","content":"run","tool_calls":[
            {"index":0,"id":"tool:invalid","type":"function","function":{"name":"same","arguments":"{}"}}
        ]}, "finish_reason":"tool_calls"}]
    })).unwrap();
    let mut out = stream.push(chunk).unwrap().value;
    let response_id = out[0].response_id.clone().unwrap();
    assert_ne!(response_id, "response:invalid");
    assert!(policy.accepts_source(&response_id));
    stream.push_done().unwrap();
    let end = stream.finish().unwrap();
    out.extend(end.chunks);
    assert!(
        out.iter()
            .all(|v| v.response_id.as_ref() == Some(&response_id))
    );
    let mut collector = GeminiStreamCollector::new(Default::default());
    for event in out {
        collector.push(event).unwrap();
    }
    let value = serde_json::to_value(collector.finish().unwrap().value).unwrap();
    let call_id = value["candidates"][0]["content"]["parts"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(|v| v.get("functionCall"))
        .unwrap()["id"]
        .as_str()
        .unwrap();
    assert_ne!(call_id, "tool:invalid");
    assert!(policy.accepts_source(call_id));
    assert_eq!(
        end.identities
            .lookup_emitted_as(IdentityRole::Response, &response_id)
            .unwrap()
            .source_id(),
        Some("response:invalid")
    );
    assert_eq!(
        end.identities
            .lookup_emitted_as(IdentityRole::ToolCall, call_id)
            .unwrap()
            .source_id(),
        Some("tool:invalid")
    );
}
#[test]
fn source_dialect_policy_is_rejected_before_any_event() {
    assert!(
        ChatToGeminiStream::new_with_policy(
            flow(),
            Default::default(),
            TargetIdPolicy::new(Dialect::OpenAiChat)
        )
        .is_err()
    );
    assert!(
        GeminiToChatStream::new_with_policy(
            GeminiToChatContext {
                created: 7,
                model: None
            },
            flow(),
            Default::default(),
            TargetIdPolicy::new(Dialect::Gemini)
        )
        .is_err()
    );
}

#[test]
fn responses_to_chat_nonpreserving_policy_does_not_reallocate_already_emitted_ids() {
    use gproxy_protocol::{
        transform::generate::{
            chat_responses::stream::ResponsesToChatStream,
            stream::responses::synthesize_responses_stream,
        },
        wire::openai::responses::response::GenerateContentResponseBody,
    };
    let body:GenerateContentResponseBody=serde_json::from_value(json!({"id":"response:invalid","object":"response","created_at":7,"model":"m","status":"completed","error":null,"incomplete_details":null,"instructions":null,"metadata":null,"parallel_tool_calls":false,"temperature":null,"top_p":null,"tool_choice":"auto","tools":[],"output":[{"type":"function_call","id":"fc-source","call_id":"tool:invalid","name":"same","arguments":"{}","status":"completed"}],"usage":{"input_tokens":3,"input_tokens_details":{"cached_tokens":0,"cache_write_tokens":0},"output_tokens":1,"output_tokens_details":{"reasoning_tokens":0},"total_tokens":4}})).unwrap();
    let mut policy =
        TargetIdPolicy::new(Dialect::OpenAiChat).with_syntax(IdSyntax::AsciiIdentifier);
    policy.preserve_source_ids = false;
    let mut stream =
        ResponsesToChatStream::new_with_policy(flow(), Default::default(), policy).unwrap();
    for event in synthesize_responses_stream(body, &mut flow(), Default::default())
        .unwrap()
        .value
    {
        stream.push(event).unwrap();
    }
    stream
        .finish()
        .expect("collector must validate generated IDs without renaming them again");
}

#[path = "transform_stream_policy/all_pairs.rs"]
mod all_pairs;
