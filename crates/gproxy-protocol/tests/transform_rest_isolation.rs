//! Every generation request pair, fed sources whose every object carries an
//! unknown extension, must build targets whose every `rest` is empty: the
//! output equals its own `into_declared` form, and no extension value leaks.

use gproxy_protocol::{
    Dialect,
    transform::{
        generate::{
            chat_responses, claude_chat, claude_gemini, claude_responses, gemini_chat,
            gemini_responses,
        },
        identity::{IdNamespace, IdentityFlow, TargetIdPolicy},
    },
    wire::{
        DeclaredFields,
        claude::generate_content as c,
        gemini as g,
        openai::{chat as h, responses as r},
    },
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

const LEAK: &str = "x-gproxy-leak";

/// Fields whose value is formally arbitrary JSON; their contents are data.
const OPAQUE: &[&str] = &[
    "input_schema",
    "parameters",
    "parametersJsonSchema",
    "schema",
    "input",
    "args",
    "response",
];

fn inject(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                if !OPAQUE.contains(&key.as_str()) {
                    inject(child);
                }
            }
            map.insert(LEAK.into(), json!(LEAK));
        }
        Value::Array(items) => items.iter_mut().for_each(inject),
        _ => {}
    }
}

fn source<T: DeserializeOwned + Serialize>(mut value: Value) -> T {
    inject(&mut value);
    let parsed: T = serde_json::from_value(value).expect("source with extensions parses");
    let kept = serde_json::to_string(&parsed)
        .unwrap()
        .matches(LEAK)
        .count();
    assert!(kept > 4, "extensions were not retained in the source");
    parsed
}

fn assert_declared<T: DeclaredFields + Clone + Serialize>(pair: &str, value: T) {
    let built = serde_json::to_value(&value).unwrap();
    assert!(!built.to_string().contains(LEAK), "{pair} leaked: {built}");
    let declared = serde_json::to_value(value.into_declared()).unwrap();
    assert_eq!(built, declared, "{pair} wrote a target rest");
}

fn flow() -> IdentityFlow {
    IdentityFlow::new(IdNamespace::with_bytes([7; 16]))
}

fn claude() -> c::GenerateContentRequestBody {
    source(json!({
        "model": "source", "max_tokens": 256,
        "system": [{"type": "text", "text": "be brief"}],
        "thinking": {"type": "enabled", "budget_tokens": 1024},
        "tools": [{"name": "lookup", "description": "find", "input_schema": {
            "type": "object", "properties": {"q": {"type": "string"}}
        }}],
        "tool_choice": {"type": "auto"},
        "messages": [
            {"role": "user", "content": [
                {"type": "text", "text": "hi"},
                {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "iVBORw0KGgo="}}
            ]},
            {"role": "assistant", "content": [
                {"type": "text", "text": "looking"},
                {"type": "tool_use", "id": "toolu_1", "name": "lookup", "input": {"q": "x"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "toolu_1", "content": [{"type": "text", "text": "found"}]}
            ]}
        ]
    }))
}

fn chat() -> h::GenerateContentRequestBody {
    source(json!({
        "model": "source", "max_completion_tokens": 256, "reasoning_effort": "high",
        "tools": [{"type": "function", "function": {"name": "lookup", "parameters": {
            "type": "object", "properties": {"q": {"type": "string"}}
        }}}],
        "tool_choice": "auto",
        "messages": [
            {"role": "system", "content": "be brief"},
            {"role": "user", "content": [
                {"type": "text", "text": "hi"},
                {"type": "image_url", "image_url": {"url": "data:image/png;base64,iVBORw0KGgo="}}
            ]},
            {"role": "assistant", "content": "looking", "tool_calls": [
                {"id": "call_1", "type": "function", "function": {"name": "lookup", "arguments": "{\"q\":\"x\"}"}}
            ]},
            {"role": "tool", "tool_call_id": "call_1", "content": "found"}
        ]
    }))
}

fn responses() -> r::GenerateContentRequestBody {
    source(json!({
        "model": "source", "max_output_tokens": 256, "instructions": "be brief",
        "reasoning": {"effort": "high"},
        "tools": [{"type": "function", "name": "lookup", "strict": false, "parameters": {
            "type": "object", "properties": {"q": {"type": "string"}}
        }}],
        "tool_choice": "auto",
        "input": [
            {"type": "message", "role": "user", "content": [
                {"type": "input_text", "text": "hi"},
                {"type": "input_image", "detail": "auto", "image_url": "data:image/png;base64,iVBORw0KGgo="}
            ]},
            {"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "looking"}]},
            {"type": "function_call", "call_id": "call_1", "name": "lookup", "arguments": "{\"q\":\"x\"}"},
            {"type": "function_call_output", "call_id": "call_1", "output": "found"}
        ]
    }))
}

fn gemini() -> g::GenerateContentRequestBody {
    source(json!({
        "systemInstruction": {"parts": [{"text": "be brief"}]},
        "generationConfig": {"maxOutputTokens": 256, "thinkingConfig": {"thinkingBudget": 1024}},
        "tools": [{"functionDeclarations": [{"name": "lookup", "description": "find", "parameters": {
            "type": "OBJECT", "properties": {"q": {"type": "STRING"}}
        }}]}],
        "toolConfig": {"functionCallingConfig": {"mode": "AUTO"}},
        "contents": [
            {"role": "user", "parts": [
                {"text": "hi"},
                {"inlineData": {"mimeType": "image/png", "data": "iVBORw0KGgo="}}
            ]},
            {"role": "model", "parts": [
                {"text": "looking"},
                {"functionCall": {"name": "lookup", "args": {"q": "x"}}}
            ]},
            {"role": "user", "parts": [
                {"functionResponse": {"name": "lookup", "response": {"result": "found"}}}
            ]}
        ]
    }))
}

#[test]
fn claude_sources_build_declared_targets() {
    let to_chat = claude_chat::claude_to_openai(&claude(), "target").unwrap();
    assert_declared("claude->chat", to_chat.value);
    let to_gemini = claude_gemini::claude_to_gemini_request(
        claude(),
        "target",
        Default::default(),
        &mut flow(),
        &TargetIdPolicy::new(Dialect::Gemini),
    )
    .unwrap();
    assert_declared("claude->gemini", to_gemini.value);
    let to_responses = claude_responses::claude_to_responses_request(
        claude(),
        "target",
        &mut flow(),
        &TargetIdPolicy::new(Dialect::OpenAi),
    )
    .unwrap();
    assert_declared("claude->responses", to_responses.value);
}

#[test]
fn chat_sources_build_declared_targets() {
    // Haiku 5.5 exercises the adaptive-thinking rewrite of the Claude target.
    for model in ["claude-sonnet-5-5", "claude-haiku-5-5"] {
        let to_claude = claude_chat::openai_to_claude(&chat(), model).unwrap();
        assert_declared("chat->claude", to_claude.value);
    }
    let to_gemini =
        gemini_chat::openai_to_gemini_request(&chat(), "target", &Default::default()).unwrap();
    assert_declared("chat->gemini", to_gemini.value);
    let to_responses = chat_responses::chat_to_responses_request(
        chat(),
        "target",
        &mut flow(),
        &TargetIdPolicy::new(Dialect::OpenAi),
    )
    .unwrap();
    assert_declared("chat->responses", to_responses.value);
}

#[test]
fn responses_sources_build_declared_targets() {
    for model in ["claude-sonnet-5-5", "claude-haiku-5-5"] {
        let to_claude =
            claude_responses::responses_to_claude_request(responses(), model, Default::default())
                .unwrap();
        assert_declared("responses->claude", to_claude.value);
    }
    let to_chat = chat_responses::responses_to_chat_request(responses(), "target").unwrap();
    assert_declared("responses->chat", to_chat.value);
    let to_gemini =
        gemini_responses::responses_to_gemini_request(responses(), "target", Default::default())
            .unwrap();
    assert_declared("responses->gemini", to_gemini.value);
}

#[test]
fn gemini_sources_build_declared_targets() {
    let to_claude = claude_gemini::gemini_to_claude_request(
        gemini(),
        "claude-haiku-5-5",
        Some(256),
        &mut flow(),
        &TargetIdPolicy::new(Dialect::Claude),
    )
    .unwrap();
    assert_declared("gemini->claude", to_claude.value);
    let to_chat = gemini_chat::gemini_to_openai_request(
        gemini(),
        "target",
        &mut flow(),
        &TargetIdPolicy::new(Dialect::OpenAiChat),
    )
    .unwrap();
    assert_declared("gemini->chat", to_chat.value);
    let to_responses = gemini_responses::gemini_to_responses_request(
        gemini(),
        "target",
        &mut flow(),
        &TargetIdPolicy::new(Dialect::OpenAi),
    )
    .unwrap();
    assert_declared("gemini->responses", to_responses.value);
}
