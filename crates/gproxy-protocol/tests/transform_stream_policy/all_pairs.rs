use super::*;
use gproxy_protocol::{
    transform::generate::{
        chat_responses::{self as hr_pair, stream as hr},
        claude_chat::stream as cc,
        claude_gemini::{self as cg_pair, stream as cg},
        claude_responses::{self as cr_pair, stream as cr},
        gemini_chat::stream as hg,
        stream::{
            chat::ChatStreamCollector,
            claude::{ClaudeStreamCollector, synthesize_claude_stream},
            gemini::GeminiStreamCollector,
            responses::{ResponsesStreamCollector, synthesize_responses_stream},
        },
    },
    wire::{
        claude::{generate_content as c, stream as cs},
        gemini as g,
        openai::{chat::stream as hs, responses as r},
    },
};
use serde_json::Value;
fn policy(dialect: Dialect, all: bool) -> TargetIdPolicy {
    let mut p = TargetIdPolicy::new(dialect).with_syntax(IdSyntax::AsciiIdentifier);
    p.preserve_source_ids = !all;
    p
}
fn initial() -> c::Usage {
    serde_json::from_value(json!({"input_tokens":3,"output_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0,"output_tokens_details":{"thinking_tokens":0}})).unwrap()
}
fn c_events() -> Vec<cs::StreamEvent> {
    let body=serde_json::from_value(json!({"id":"response:invalid","type":"message","model":"m","role":"assistant","content":[{"type":"tool_use","id":"tool:invalid","name":"same","input":{}}],"stop_reason":"tool_use","usage":{"input_tokens":3,"output_tokens":1,"cache_creation_input_tokens":0,"cache_read_input_tokens":0,"output_tokens_details":{"thinking_tokens":0}}})).unwrap();
    synthesize_claude_stream(body, Default::default())
        .unwrap()
        .value
}
fn h_chunk() -> hs::ChatCompletionChunk {
    serde_json::from_value(json!({"id":"response:invalid","object":"chat.completion.chunk","created":7,"model":"m","choices":[{"index":0,"delta":{"role":"assistant","tool_calls":[{"index":0,"id":"tool:invalid","type":"function","function":{"name":"same","arguments":"{}"}}]},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":3,"completion_tokens":1,"total_tokens":4,"prompt_tokens_details":{"cached_tokens":0,"cache_write_tokens":0},"completion_tokens_details":{"reasoning_tokens":0}}})).unwrap()
}
fn g_chunk() -> g::GenerateContentResponseBody {
    serde_json::from_value(json!({"responseId":"response:invalid","modelVersion":"m","candidates":[{"index":0,"content":{"role":"model","parts":[{"functionCall":{"id":"tool:invalid","name":"same","args":{}}}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":3,"cachedContentTokenCount":0,"candidatesTokenCount":1,"thoughtsTokenCount":0,"totalTokenCount":4}})).unwrap()
}
fn r_events() -> Vec<r::stream::StreamEvent> {
    let body=serde_json::from_value(json!({"id":"response:invalid","object":"response","created_at":7,"model":"m","status":"completed","error":null,"incomplete_details":null,"instructions":null,"metadata":null,"parallel_tool_calls":false,"temperature":null,"top_p":null,"tool_choice":"auto","tools":[],"output":[{"type":"function_call","id":"fc-source","call_id":"tool:invalid","name":"same","arguments":"{}","status":"completed"}],"usage":{"input_tokens":3,"input_tokens_details":{"cached_tokens":0,"cache_write_tokens":0},"output_tokens":1,"output_tokens_details":{"reasoning_tokens":0},"total_tokens":4}})).unwrap();
    synthesize_responses_stream(body, &mut flow(), Default::default())
        .unwrap()
        .value
}
fn request() -> r::GenerateContentRequestBody {
    serde_json::from_value(
        json!({"model":"m","input":"task","parallel_tool_calls":false,"tool_choice":"auto"}),
    )
    .unwrap()
}
fn ccontext() -> cr_pair::ClaudeResponseContext {
    cr_pair::ClaudeResponseContext {
        request: request(),
        effective_parallel_tool_calls: false,
        effective_tool_choice: serde_json::from_value(json!("auto")).unwrap(),
        usage: Default::default(),
        created_at: 7,
        effective_prompt_cache_options: None,
    }
}
fn hcontext() -> hr_pair::ResponsesResponseContext {
    hr_pair::ResponsesResponseContext {
        request: request(),
        effective_parallel_tool_calls: false,
        effective_tool_choice: serde_json::from_value(json!("auto")).unwrap(),
        usage: hr_pair::ChatUsageSupplement {
            cache_write_tokens: Some(0),
            cached_tokens: Some(0),
            reasoning_tokens: Some(0),
        },
        effective_prompt_cache_options: None,
    }
}
fn facts() -> cg_pair::ClaudeGeminiUsageFacts {
    cg_pair::ClaudeGeminiUsageFacts {
        cache_creation_input_tokens: Some(0),
        cache_read_input_tokens: Some(0),
        thinking_tokens: Some(0),
    }
}
fn chat(chunks: Vec<hs::ChatCompletionChunk>) -> Value {
    let mut c = ChatStreamCollector::new(flow(), TargetIdPolicy::new(Dialect::OpenAiChat));
    for chunk in chunks {
        c.push(chunk).unwrap();
    }
    c.push_done().unwrap();
    serde_json::to_value(c.finish().unwrap().value).unwrap()
}
fn claude(chunks: Vec<cs::StreamEvent>) -> Value {
    let mut c = ClaudeStreamCollector::new(Default::default());
    for chunk in chunks {
        c.push(chunk).unwrap();
    }
    serde_json::to_value(c.finish().unwrap().value).unwrap()
}
fn gemini(chunks: Vec<g::GenerateContentResponseBody>) -> Value {
    let mut c = GeminiStreamCollector::new(Default::default());
    for chunk in chunks {
        c.push(chunk).unwrap();
    }
    serde_json::to_value(c.finish().unwrap().value).unwrap()
}
fn responses(chunks: Vec<r::stream::StreamEvent>) -> Value {
    let mut c = ResponsesStreamCollector::new(Default::default());
    for chunk in chunks {
        c.push(chunk).unwrap();
    }
    serde_json::to_value(c.finish().unwrap().value).unwrap()
}
fn check(value: Value, dialect: Dialect, mut p: TargetIdPolicy, flow: &IdentityFlow) {
    p.preserve_source_ids = true;
    let response = if dialect == Dialect::Gemini {
        value["responseId"].as_str().unwrap()
    } else {
        value["id"].as_str().unwrap()
    };
    let call = match dialect {
        Dialect::OpenAiChat => value["choices"][0]["message"]["tool_calls"][0]["id"]
            .as_str()
            .unwrap(),
        Dialect::Claude => value["content"][0]["id"].as_str().unwrap(),
        Dialect::Gemini => value["candidates"][0]["content"]["parts"][0]["functionCall"]["id"]
            .as_str()
            .unwrap(),
        Dialect::OpenAi => value["output"][0]["call_id"].as_str().unwrap(),
        _ => unreachable!(),
    };
    for (role, id, source) in [
        (IdentityRole::Response, response, "response:invalid"),
        (IdentityRole::ToolCall, call, "tool:invalid"),
    ] {
        assert!(p.accepts_source(id), "policy rejected emitted {id}");
        assert_ne!(id, source);
        assert_eq!(
            flow.lookup_emitted_as(role, id).unwrap().source_id(),
            Some(source)
        );
    }
}
#[test]
fn all_ten_states_apply_policy_once_to_early_ids_and_final_native_output() {
    for all in [false, true] {
        let p = policy(Dialect::OpenAiChat, all);
        let mut s = cc::ClaudeToChatStream::new_with_policy(
            cc::ClaudeToChatContext { created: 7 },
            flow(),
            Default::default(),
            p.clone(),
        )
        .unwrap();
        let mut out = Vec::new();
        for e in c_events() {
            out.extend(s.push(e).unwrap().value);
        }
        let end = s.finish().unwrap();
        out.extend(end.chunks);
        check(chat(out), Dialect::OpenAiChat, p, &end.identities);
        let p = policy(Dialect::Claude, all);
        let mut s = cc::ChatToClaudeStream::new_with_policy(
            cc::ChatToClaudeContext {
                start_usage: Some(initial()),
            },
            flow(),
            Default::default(),
            p.clone(),
        )
        .unwrap();
        let mut out = s.push(h_chunk()).unwrap().value;
        s.push_done().unwrap();
        let end = s.finish().unwrap();
        out.extend(end.chunks);
        check(claude(out), Dialect::Claude, p, &end.identities);
        let p = policy(Dialect::Gemini, all);
        let mut s = cg::ClaudeToGeminiStream::new_with_policy(
            cg::ClaudeToGeminiContext { usage: facts() },
            flow(),
            Default::default(),
            p.clone(),
        )
        .unwrap();
        let mut out = Vec::new();
        for e in c_events() {
            out.extend(s.push(e).unwrap().value);
        }
        let end = s.finish().unwrap();
        out.extend(end.chunks);
        check(gemini(out), Dialect::Gemini, p, &end.identities);
        let p = policy(Dialect::Claude, all);
        let mut s = cg::GeminiToClaudeStream::new_with_policy(
            cg::GeminiToClaudeContext {
                usage: Some(initial()),
                facts: facts(),
                ..Default::default()
            },
            flow(),
            Default::default(),
            p.clone(),
        )
        .unwrap();
        let mut out = s.push(g_chunk()).unwrap().value;
        let end = s.finish().unwrap();
        out.extend(end.chunks);
        check(claude(out), Dialect::Claude, p, &end.identities);
        let p = policy(Dialect::OpenAi, all);
        let mut s = cr::ClaudeToResponsesStream::new_with_policy(
            ccontext(),
            flow(),
            Default::default(),
            p.clone(),
        )
        .unwrap();
        let mut out = Vec::new();
        for e in c_events() {
            out.extend(s.push(e).unwrap().value);
        }
        let end = s.finish().unwrap();
        out.extend(end.chunks);
        check(responses(out), Dialect::OpenAi, p, &end.identities);
        let p = policy(Dialect::Claude, all);
        let mut s = cr::ResponsesToClaudeStream::new_with_policy(
            cr::ResponsesToClaudeContext {
                usage: Some(initial()),
                restoration: None,
            },
            flow(),
            Default::default(),
            p.clone(),
        )
        .unwrap();
        let mut out = Vec::new();
        for e in r_events() {
            out.extend(s.push(e).unwrap().value);
        }
        let end = s.finish().unwrap();
        out.extend(end.chunks);
        check(claude(out), Dialect::Claude, p, &end.identities);
        let p = policy(Dialect::OpenAi, all);
        let mut s = hr::ChatToResponsesStream::new_with_policy(
            hcontext(),
            flow(),
            Default::default(),
            p.clone(),
        )
        .unwrap();
        let mut out = s.push(h_chunk()).unwrap().value;
        s.push_done().unwrap();
        let end = s.finish().unwrap();
        out.extend(end.chunks);
        check(responses(out), Dialect::OpenAi, p, &end.identities);
        let p = policy(Dialect::OpenAiChat, all);
        let mut s =
            hr::ResponsesToChatStream::new_with_policy(flow(), Default::default(), p.clone())
                .unwrap();
        let mut out = Vec::new();
        for e in r_events() {
            out.extend(s.push(e).unwrap().value);
        }
        let end = s.finish().unwrap();
        out.extend(end.chunks);
        check(chat(out), Dialect::OpenAiChat, p, &end.identities);
        let p = policy(Dialect::Gemini, all);
        let mut s =
            hg::ChatToGeminiStream::new_with_policy(flow(), Default::default(), p.clone()).unwrap();
        let mut out = s.push(h_chunk()).unwrap().value;
        s.push_done().unwrap();
        let end = s.finish().unwrap();
        out.extend(end.chunks);
        check(gemini(out), Dialect::Gemini, p, &end.identities);
        let p = policy(Dialect::OpenAiChat, all);
        let mut s = hg::GeminiToChatStream::new_with_policy(
            hg::GeminiToChatContext {
                created: 7,
                model: None,
            },
            flow(),
            Default::default(),
            p.clone(),
        )
        .unwrap();
        let mut out = s.push(g_chunk()).unwrap().value;
        let end = s.finish().unwrap();
        out.extend(end.chunks);
        check(chat(out), Dialect::OpenAiChat, p, &end.identities);
    }
}
