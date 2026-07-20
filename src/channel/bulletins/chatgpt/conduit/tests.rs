use super::*;

fn decoded(sse: &str, key: &str) -> String {
    crate::channel::bulletins::chatgpt::sse_to_openai::collect_all("gpt-5", sse.as_bytes())
        .iter()
        .flat_map(|chunk| chunk.choices.iter())
        .filter_map(|choice| choice.delta.get(key).and_then(Value::as_str))
        .collect()
}

#[test]
fn extracts_handoff_turn_id() {
    let stub = b"event: delta_encoding\ndata: \"v1\"\n\ndata: {\"type\": \"stream_handoff\", \"conversation_id\": \"c1\", \"turn_exchange_id\": \"turn-abc\", \"options\": []}\n\ndata: [DONE]\n\n";
    assert_eq!(extract_handoff_turn(stub).as_deref(), Some("turn-abc"));
}

#[test]
fn no_handoff_in_inline_stream() {
    let inline = b"event: delta\ndata: {\"v\":{\"message\":{\"author\":{\"role\":\"assistant\"},\"content\":{\"content_type\":\"text\",\"parts\":[\"\"]}}},\"c\":2}\n\n";
    assert_eq!(extract_handoff_turn(inline), None);
}

#[test]
fn absorbs_stream_items_and_detects_complete() {
    let mut sse = String::new();
    let mut synth = MsgSynth::default();
    let first = r#"[{"type":"message","topic_id":"conversation-turn-x","payload":{"type":"conversation-turn-stream","payload":{"type":"stream-item","encoded_item":"event: delta\ndata: {\"v\":\"hi\"}\n\n"}}}]"#;
    assert!(!absorb_frame(first, &mut sse, &mut synth));
    assert!(sse.contains("\"v\":\"hi\""));

    let complete = r#"[{"type":"message","topic_id":"conversation-turn-x","payload":{"type":"conversation-turn-stream","payload":{"type":"stream-item","encoded_item":"data: {\"type\": \"message_stream_complete\"}\n\n"}}}]"#;
    assert!(absorb_frame(complete, &mut sse, &mut synth));
}

#[test]
fn absorbs_catchups_from_subscribe_reply() {
    let mut sse = String::new();
    let mut synth = MsgSynth::default();
    let frame = r#"[{"id":2,"type":"reply","reply":{"type":"subscribe","topic_id":"conversation-turn-x","catchups":[{"type":"message","payload":{"type":"conversation-turn-stream","payload":{"type":"stream-item","encoded_item":"data: {\"v\":\"replayed\"}\n\n"}}}]}}]"#;
    assert!(!absorb_frame(frame, &mut sse, &mut synth));
    assert!(sse.contains("replayed"));
}

#[test]
fn synthesizes_deep_research_add_messages() {
    let mut sse = String::new();
    let mut synth = MsgSynth::default();
    let thought = r#"[{"type":"message","payload":{"type":"conversation-update","update_type":"add-messages","update_content":{"messages":[{"id":"t1","author":{"role":"assistant"},"recipient":"all","content":{"content_type":"thoughts","thoughts":[{"summary":"Searching","content":"look up tokio releases"}]}}]}}}]"#;
    let partial = r#"[{"type":"message","payload":{"type":"conversation-update","update_type":"add-messages","update_content":{"messages":[{"id":"a1","author":{"role":"assistant"},"recipient":"all","content":{"content_type":"text","parts":["报告:tokio "]},"status":"in_progress"}]}}}]"#;
    let final_report = r#"[{"type":"message","payload":{"type":"conversation-update","update_type":"add-messages","update_content":{"messages":[{"id":"a1","author":{"role":"assistant"},"recipient":"all","content":{"content_type":"text","parts":["报告:tokio 1.0 于 2020 发布。"]},"status":"finished_successfully"}]}}}]"#;
    absorb_frame(thought, &mut sse, &mut synth);
    absorb_frame(partial, &mut sse, &mut synth);
    assert!(absorb_frame(final_report, &mut sse, &mut synth));

    let reasoning = decoded(&sse, "reasoning_content");
    assert!(
        reasoning.contains("look up tokio releases"),
        "CoT not surfaced: {reasoning}"
    );
    assert_eq!(decoded(&sse, "content"), "报告:tokio 1.0 于 2020 发布。");
}

#[test]
fn decodes_deep_research_widget_progress() {
    let mut sse = String::new();
    let mut synth = MsgSynth::default();
    let first = r#"{"type":"conversation-update","payload":{"conversation_id":"c","update_type":"update-widget-state","update_content":{"widget_session_id":"w","updates":[{"message_id":"m","widget_state":{"plan":{"plan_id":"p","version":1,"steps":[{"id":"step-1","text":"检索 serde 官方仓库","status":"completed","reason":"已检索 releases/tags"},{"id":"step-2","text":"查找 0.x 到 1.0 版本","status":"in_progress","reason":"正在定位 1.0"},{"id":"step-3","text":"定位 derive 宏","status":"pending","reason":null}]},"status":"researching"}}]}}}"#;
    let second = r#"{"type":"conversation-update","payload":{"conversation_id":"c","update_type":"update-widget-state","update_content":{"widget_session_id":"w","updates":[{"message_id":"m","widget_state":{"plan":{"plan_id":"p","version":2,"steps":[{"id":"step-1","text":"检索 serde 官方仓库","status":"completed","reason":"已检索 releases/tags"},{"id":"step-2","text":"查找 0.x 到 1.0 版本","status":"completed","reason":"确认 1.0.0 为 2017-04"},{"id":"step-3","text":"定位 derive 宏","status":"pending","reason":null}]},"status":"researching"}}]}}}"#;
    let report = r##"{"type":"conversation-update","payload":{"conversation_id":"c","update_type":"update-widget-state","update_content":{"updates":[{"message_id":"m","widget_state":{"status":"completed","report_message":{"id":"r1","author":{"role":"assistant","metadata":{"real_author":"tool:web"}},"recipient":"all","content":{"content_type":"text","parts":["# Serde 报告\n\n执行摘要:serde_derive 0.8.6。"]},"status":"finished_successfully"}}}]}}}"##;
    assert!(!absorb_frame(first, &mut sse, &mut synth));
    assert!(!absorb_frame(second, &mut sse, &mut synth));
    assert!(absorb_frame(report, &mut sse, &mut synth));

    let reasoning = decoded(&sse, "reasoning_content");
    assert!(reasoning.contains("检索 serde 官方仓库"));
    assert!(reasoning.contains("正在定位 1.0"));
    assert!(reasoning.contains("确认 1.0.0 为 2017-04"));
    assert!(!reasoning.contains("定位 derive 宏"));
    assert_eq!(reasoning.matches("已检索 releases/tags").count(), 1);
    assert!(decoded(&sse, "content").contains("执行摘要:serde_derive 0.8.6"));
}
