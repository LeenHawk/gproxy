use gproxy_protocol::{claude, gemini, openai, wire::DeclaredFields};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

fn check<T: DeclaredFields + DeserializeOwned + Serialize>(source: Value, expected: Value) {
    let parsed: T = serde_json::from_value(source.clone()).unwrap();
    // Adding the derive must not change ordinary forward-compatible serde.
    assert_eq!(serde_json::to_value(&parsed).unwrap(), source);
    assert_eq!(
        serde_json::to_value(parsed.into_declared()).unwrap(),
        expected
    );
}

#[test]
fn claude_nested_content_and_formal_schema_data() {
    check::<claude::generate_content::GenerateContentRequestBody>(
        json!({"model":"claude", "max_tokens":12, "future":1,
            "messages":[{"role":"user","future":2,"content":[
                {"type":"text","text":"hello","future":3,
                 "cache_control":{"type":"ephemeral","future":4}},
                {"type":"tool_result","tool_use_id":"toolu_1","future":5,
                 "content":[{"type":"text","text":"result","future":6}]}]}],
            "tools":[{"name":"lookup","future":7,"input_schema":{"type":"object",
                "properties":{"future":{"type":"string","custom-keyword":1}},
                "future":8}}]}),
        json!({"model":"claude", "max_tokens":12,
            "messages":[{"role":"user","content":[
                {"type":"text","text":"hello","cache_control":{"type":"ephemeral"}},
                {"type":"tool_result","tool_use_id":"toolu_1",
                 "content":[{"type":"text","text":"result"}]}]}],
            "tools":[{"name":"lookup","input_schema":{"type":"object",
                "properties":{"future":{"type":"string","custom-keyword":1}}}}]}),
    );
}

#[test]
fn gemini_recursive_schema_and_arbitrary_function_data() {
    check::<gemini::generate_content::GenerateContentRequestBody>(
        json!({"contents":[{"role":"user","future":1,"parts":[
            {"functionCall":{"name":"lookup","args":{"future":{"x":1}},"future":2},"future":3}]}],
            "generationConfig":{"future":4,"responseSchema":{"type":"OBJECT","future":5,
                "properties":{"items":{"type":"ARRAY","items":{"type":"STRING","future":6}}}}},
            "tools":[{"functionDeclarations":[{"name":"lookup","description":"Lookup","future":7,
                "parametersJsonSchema":{"type":"object","custom-keyword":{"future":8}}}],"future":9}],"future":10}),
        json!({"contents":[{"role":"user","parts":[
            {"functionCall":{"name":"lookup","args":{"future":{"x":1}}}}]}],
            "generationConfig":{"responseSchema":{"type":"OBJECT",
                "properties":{"items":{"type":"ARRAY","items":{"type":"STRING"}}}}},
            "tools":[{"functionDeclarations":[{"name":"lookup","description":"Lookup",
                "parametersJsonSchema":{"type":"object","custom-keyword":{"future":8}}}]}]}),
    );
}

#[test]
fn responses_nullable_fields_and_macro_generated_items() {
    check::<openai::responses::generate::GenerateContentRequestBody>(
        json!({"model":"openai", "instructions":null,"metadata":{"future":"data"},"future":1,
            "input":[{"type":"tool_search_call","arguments":{"future":2},"call_id":"call_1","future":3}],
            "tools":[{"type":"function","name":"lookup","description":null,"strict":null,
                "parameters":{"type":"object","future":4},"future":5}]}),
        json!({"model":"openai", "instructions":null,"metadata":{"future":"data"},
            "input":[{"type":"tool_search_call","arguments":{"future":2},"call_id":"call_1"}],
            "tools":[{"type":"function","name":"lookup","description":null,"strict":null,
                "parameters":{"type":"object","future":4}}]}),
    );
}

#[test]
fn chat_nested_audio_tool_arguments_and_stream_options() {
    check::<openai::chat::request::GenerateContentRequestBody>(
        json!({"model":"openai","future":1,"messages":[{"role":"user","future":2,
            "content":[{"type":"input_audio","future":3,"input_audio":{"data":"YQ==","format":"wav","future":4}}]}],
            "stream_options":{"include_usage":true,"future":5},
            "tools":[{"type":"function","future":6,"function":{"name":"lookup","future":7,
                "parameters":{"type":"object","custom-keyword":{"future":8}}}}]}),
        json!({"model":"openai","messages":[{"role":"user",
            "content":[{"type":"input_audio","input_audio":{"data":"YQ==","format":"wav"}}]}],
            "stream_options":{"include_usage":true},
            "tools":[{"type":"function","function":{"name":"lookup",
                "parameters":{"type":"object","custom-keyword":{"future":8}}}}]}),
    );
}
