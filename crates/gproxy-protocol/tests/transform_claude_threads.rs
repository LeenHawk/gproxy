use gproxy_protocol::{
    Dialect,
    claude::generate_content::GenerateContentRequestBody,
    transform::{
        TransformErrorKind,
        generate::{claude_chat, claude_gemini, claude_responses},
        identity::{IdNamespace, IdentityFlow, TargetIdPolicy},
    },
};
use serde_json::json;

#[test]
fn threads_require_full_history_for_every_cross_dialect_target() {
    for thread in [
        json!({"type":"create"}),
        json!({"type":"continue", "previous_message_id":"msg_previous"}),
    ] {
        let input: GenerateContentRequestBody = serde_json::from_value(json!({
            "model":"claude-fable-5-2", "max_tokens":32,
            "thread":thread, "messages":[{"role":"user", "content":"new turn"}]
        }))
        .unwrap();
        assert_eq!(serde_json::to_value(&input).unwrap()["thread"], thread);
        let mut flow = IdentityFlow::new(IdNamespace::with_bytes([63; 16]));
        let outputs = [
            claude_chat::claude_to_openai(&input, "target")
                .map(|v| (serde_json::to_value(v.value).unwrap(), v.report)),
            claude_responses::claude_to_responses_request(
                input.clone(),
                "target",
                &mut flow,
                &TargetIdPolicy::new(Dialect::OpenAi),
            )
            .map(|v| (serde_json::to_value(v.value).unwrap(), v.report)),
            claude_gemini::claude_to_gemini_request(
                input,
                "target",
                Default::default(),
                &mut flow,
                &TargetIdPolicy::new(Dialect::Gemini),
            )
            .map(|v| (serde_json::to_value(v.value).unwrap(), v.report)),
        ];
        for output in outputs {
            if thread["type"] == "continue" {
                let error = output.unwrap_err();
                assert_eq!(error.kind(), TransformErrorKind::Unsupported);
                assert_eq!(error.context(), "thread.previous_message_id");
            } else {
                let (value, report) = output.unwrap();
                assert!(value.to_string().contains("new turn"));
                assert!(value.get("thread").is_none());
                assert!(report.diagnostics.iter().any(|d| d.field == "thread"));
            }
        }
    }
}

#[test]
fn fable_and_mythos_52_relax_incompatible_chat_controls() {
    let input = serde_json::from_value(json!({
        "model":"source", "max_tokens":32, "messages":[{"role":"user", "content":"hello"}],
        "reasoning_effort":"none", "tool_choice":"required",
        "tools":[{"type":"function", "function":{"name":"lookup", "parameters":{"type":"object"}}}]
    }))
    .unwrap();
    for model in [
        "claude-fable-5-2",
        "claude-mythos-5-2",
        "anthropic/claude-fable-5-2",
        "anthropic/claude-fable-5.2",
        "anthropic/claude-mythos-5.2",
        "claude-mythos-5-2-20261109",
    ] {
        let converted = claude_chat::openai_to_claude(&input, model).unwrap();
        let value = serde_json::to_value(converted.value).unwrap();
        assert!(value.get("thinking").is_none(), "{model}: {value}");
        assert_eq!(value["tool_choice"]["type"], "auto", "{model}");
    }
}
