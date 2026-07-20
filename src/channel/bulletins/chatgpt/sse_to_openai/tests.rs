use super::*;

fn joined(chunks: &[OpenAiChunk], key: &str) -> String {
    chunks
        .iter()
        .flat_map(|chunk| chunk.choices.iter())
        .filter_map(|choice| choice.delta.get(key).and_then(Value::as_str))
        .collect()
}

#[test]
fn ignores_system_and_user_channels() {
    let body = br#"event: delta_encoding
data: "v1"

event: delta
data: {"p":"","o":"add","v":{"message":{"id":"sys","author":{"role":"system"},"content":{"content_type":"text","parts":[""]},"status":"finished_successfully","metadata":{"is_visually_hidden_from_conversation":true}},"conversation_id":"c1"},"c":0}

event: delta
data: {"p":"","o":"add","v":{"message":{"id":"user","author":{"role":"user"},"content":{"content_type":"text","parts":["hi"]},"status":"finished_successfully"},"conversation_id":"c1"},"c":1}

event: delta
data: {"p":"","o":"add","v":{"message":{"id":"asst","author":{"role":"assistant"},"content":{"content_type":"text","parts":[""]},"status":"in_progress","metadata":{"model_slug":"gpt-5"}},"conversation_id":"c1"},"c":2}

event: delta
data: {"v":[{"p":"/message/content/parts/0","o":"append","v":"hello"}]}

event: delta
data: {"v":[{"p":"/message/content/parts/0","o":"append","v":" world"},{"p":"/message/status","o":"replace","v":"finished_successfully"}]}

"#;
    let chunks = collect_all("gpt-5", body);
    assert_eq!(joined(&chunks, "content"), "hello world");
    assert_eq!(
        chunks.last().unwrap().choices[0].finish_reason.as_deref(),
        Some("stop")
    );
}

#[test]
fn surfaces_reasoning_channel_as_reasoning_content() {
    let body = br#"event: delta_encoding
data: "v1"

event: delta
data: {"p":"","o":"add","v":{"message":{"id":"think","author":{"role":"assistant"},"content":{"content_type":"thoughts","parts":[""]},"status":"in_progress"},"conversation_id":"c1"},"c":0}

event: delta
data: {"v":[{"p":"/message/content/parts/0","o":"append","v":"let me think"}]}

event: delta
data: {"p":"","o":"add","v":{"message":{"id":"asst","author":{"role":"assistant"},"content":{"content_type":"text","parts":[""]},"status":"in_progress","metadata":{"model_slug":"gpt-5"}},"conversation_id":"c1"},"c":1}

event: delta
data: {"v":[{"p":"/message/content/parts/0","o":"append","v":"answer"},{"p":"/message/status","o":"replace","v":"finished_successfully"}]}

"#;
    let chunks = collect_all("gpt-5", body);
    assert_eq!(joined(&chunks, "reasoning_content"), "let me think");
    assert_eq!(joined(&chunks, "content"), "answer");
    assert_eq!(
        chunks.last().unwrap().choices[0].finish_reason.as_deref(),
        Some("stop")
    );
}

#[test]
fn real_stream_path_elision_and_reasoning_recap() {
    let body = r#"event: delta_encoding
data: "v1"

event: delta
data: {"o":"add","v":{"message":{"id":"r1","author":{"role":"assistant"},"content":{"content_type":"reasoning_recap","content":"已思考 5s"},"status":"finished_successfully"},"conversation_id":"c1"},"c":6}

event: delta
data: {"v":{"message":{"id":"a1","author":{"role":"assistant"},"content":{"content_type":"text","parts":[""]},"status":"in_progress","metadata":{"model_slug":"gpt-5-5-thinking"}},"conversation_id":"c1"},"c":7}

event: delta
data: {"p":"/message/content/parts/0","o":"append","v":"我不能展示"}

event: delta
data: {"v":"内部逐字思考过程"}

event: delta
data: {"v":"，但可以给出推导。"}

event: delta
data: {"v":[{"p":"/message/status","o":"replace","v":"finished_successfully"}]}

data: [DONE]
"#
    .as_bytes();
    let chunks = collect_all("gpt-5-5-thinking", body);
    assert_eq!(joined(&chunks, "reasoning_content"), "已思考 5s");
    assert_eq!(
        joined(&chunks, "content"),
        "我不能展示内部逐字思考过程，但可以给出推导。"
    );
    assert_eq!(
        chunks.last().unwrap().choices[0].finish_reason.as_deref(),
        Some("stop")
    );
}

#[test]
fn strips_web_search_citation_markers() {
    let body = "event: delta_encoding\ndata: \"v1\"\n\n\
event: delta\n\
data: {\"v\":{\"message\":{\"author\":{\"role\":\"assistant\"},\"content\":{\"content_type\":\"text\",\"parts\":[\"\"]},\"status\":\"in_progress\",\"metadata\":{\"model_slug\":\"gpt-5\"}}},\"c\":2}\n\n\
event: delta\n\
data: {\"p\":\"/message/content/parts/0\",\"o\":\"append\",\"v\":\"Rust 1.89 于 2025-08-07 发布。\u{e200}cite\u{e202}turn0search0\"}\n\n\
event: delta\n\
data: {\"v\":\"\u{e202}turn0search9\u{e201} 出处：blog.rust-lang.org\"}\n\n\
event: delta\n\
data: {\"v\":[{\"p\":\"/message/status\",\"o\":\"replace\",\"v\":\"finished_successfully\"}]}\n\n\
data: [DONE]\n\n";
    let chunks = collect_all("gpt-5", body.as_bytes());
    let content = joined(&chunks, "content");
    assert_eq!(
        content,
        "Rust 1.89 于 2025-08-07 发布。 出处：blog.rust-lang.org"
    );
    assert!(!content.contains('\u{e200}') && !content.contains('\u{e202}'));
}
