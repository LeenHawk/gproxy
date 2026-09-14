use gproxy_protocol::{
    Dialect,
    transform::{
        Converted,
        count_tokens::request as count,
        generate::{
            chat_responses, claude_chat, claude_gemini, claude_responses, gemini_chat,
            gemini_responses,
        },
        identity::{IdNamespace, IdentityFlow, TargetIdPolicy},
    },
    wire::claude::generate_content as c,
};
use serde_json::{Value, json};

fn flow() -> IdentityFlow {
    IdentityFlow::new(IdNamespace::with_bytes([96; 16]))
}
fn policy(dialect: Dialect) -> TargetIdPolicy {
    TargetIdPolicy::new(dialect)
}
fn chat(turns: &Value) -> Value {
    json!({"model":"client-alias", "max_completion_tokens":32, "messages":turns})
}
fn responses(turns: &Value) -> Value {
    json!({"model":"client-alias", "max_output_tokens":32, "input":turns})
}
fn gemini(turns: &Value) -> Value {
    let contents: Vec<_> = turns
        .as_array()
        .unwrap()
        .iter()
        .map(|turn| {
            let role = match turn["role"].as_str().unwrap() {
                "assistant" => "model",
                "developer" => "system",
                role => role,
            };
            json!({"role":role, "parts":[{"text":turn["content"]}]})
        })
        .collect();
    json!({"contents":contents, "generationConfig":{"maxOutputTokens":32}})
}
fn claude(turns: &Value) -> Value {
    let mut turns = turns.clone();
    for turn in turns.as_array_mut().unwrap() {
        if turn["role"] == "developer" {
            turn["role"] = json!("system");
        }
    }
    json!({"model":"source", "max_tokens":32, "messages":turns})
}
fn to_claude(turns: &Value, model: &str) -> Vec<Converted<c::GenerateContentRequestBody>> {
    vec![
        claude_chat::openai_to_claude(&serde_json::from_value(chat(turns)).unwrap(), model)
            .unwrap(),
        claude_responses::responses_to_claude_request(
            serde_json::from_value(responses(turns)).unwrap(),
            model,
            Default::default(),
        )
        .unwrap(),
        claude_gemini::gemini_to_claude_request(
            serde_json::from_value(gemini(turns)).unwrap(),
            model,
            None,
            &mut flow(),
            &policy(Dialect::Claude),
        )
        .unwrap(),
    ]
}
fn roles(messages: &Value) -> Vec<&str> {
    messages
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["role"].as_str().unwrap())
        .collect()
}
fn text(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(parts) => parts
            .iter()
            .map(|part| part["text"].as_str().unwrap())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => panic!("unexpected content: {content}"),
    }
}

#[test]
fn appending_instructions_preserves_each_converted_prefix_for_all_claude_entries() {
    let prefix = json!([
        {"role":"system","content":"initial"},
        {"role":"developer","content":"initial developer"},
        {"role":"user","content":"先回答问题 A"},
        {"role":"assistant","content":"A 的回答"},
        {"role":"user","content":"再回答问题 B"}
    ]);
    let mut extended = prefix.clone();
    extended.as_array_mut().unwrap().extend([
        json!({"role":"system","content":"从现在起只用中文"}),
        json!({"role":"developer","content":"保持简短"}),
    ]);
    for (model, expected) in [
        ("claude-opus-4-8", "system"),
        ("claude-fable-5-1", "system"),
        ("claude-mythos-5", "system"),
        ("CLAUDE-OPUS-5", "system"),
        ("us.anthropic.claude-opus-4-8-20260101-v1:0", "system"),
        ("claude-sonnet-4-5", "user"),
        ("claude-sonnet-5", "user"),
        ("unknown-model", "user"),
        ("claude-opus-4-80", "user"),
    ] {
        for (before, after) in to_claude(&prefix, model)
            .into_iter()
            .zip(to_claude(&extended, model))
        {
            let before = serde_json::to_value(before.value).unwrap();
            let after_wire = serde_json::to_value(after.value).unwrap();
            assert_eq!(before["system"], after_wire["system"], "{model}");
            assert_eq!(text(&after_wire["system"]), "initial\ninitial developer");
            assert_eq!(
                before["messages"].as_array().unwrap(),
                &after_wire["messages"].as_array().unwrap()[..3]
            );
            assert_eq!(
                roles(&after_wire["messages"]),
                ["user", "assistant", "user", expected, expected]
            );
            assert_eq!(
                text(&after_wire["messages"][3]["content"]),
                "从现在起只用中文"
            );
            assert_eq!(text(&after_wire["messages"][4]["content"]), "保持简短");
            let changes = after
                .report
                .diagnostics
                .iter()
                .filter(|d| d.message.contains("downgraded to user in place"))
                .count();
            assert_eq!(changes, if expected == "user" { 2 } else { 0 });
        }
    }
}

#[test]
fn ordinary_assistant_cannot_be_followed_by_system_even_inside_history() {
    let turns = json!([
        {"role":"user","content":"A"},
        {"role":"assistant","content":"answer A"},
        {"role":"system","content":"middle instruction"},
        {"role":"assistant","content":"answer B"},
        {"role":"system","content":"tail 1"},
        {"role":"developer","content":"tail 2"}
    ]);
    for model in ["claude-opus-4-8", "claude-sonnet-5"] {
        for output in to_claude(&turns, model) {
            let wire = serde_json::to_value(output.value).unwrap();
            assert!(wire.get("system").is_none());
            assert_eq!(
                roles(&wire["messages"]),
                ["user", "assistant", "user", "assistant", "user", "user"]
            );
            let texts: Vec<_> = wire["messages"]
                .as_array()
                .unwrap()
                .iter()
                .map(|m| text(&m["content"]))
                .collect();
            assert_eq!(
                texts,
                [
                    "A",
                    "answer A",
                    "middle instruction",
                    "answer B",
                    "tail 1",
                    "tail 2"
                ]
            );
        }
    }
}

#[test]
fn system_after_user_must_end_the_request_or_precede_assistant() {
    for next in ["user", "assistant"] {
        let turns = json!([
            {"role":"user","content":"A"},
            {"role":"system","content":"note 1"},
            {"role":"developer","content":"note 2"},
            {"role":next,"content":"B"}
        ]);
        for output in to_claude(&turns, "claude-opus-4-8") {
            let wire = serde_json::to_value(output.value).unwrap();
            let expected = if next == "assistant" {
                "system"
            } else {
                "user"
            };
            assert_eq!(roles(&wire["messages"]), ["user", expected, expected, next]);
        }
    }
}

#[test]
fn empty_user_turn_does_not_promote_a_later_instruction() {
    let turns = json!([{"role":"user","content":""},{"role":"system","content":"later"}]);
    for output in to_claude(&turns, "claude-opus-4-8") {
        let wire = serde_json::to_value(output.value).unwrap();
        assert!(wire.get("system").is_none());
        assert_eq!(
            wire["messages"].as_array().unwrap().last().unwrap()["role"],
            "system"
        );
    }
}

#[test]
fn all_gemini_entries_keep_later_instructions_as_separate_user_turns() {
    let turns = json!([
        {"role":"system","content":"initial"},
        {"role":"user","content":"A"},
        {"role":"assistant","content":"answer A"},
        {"role":"system","content":"middle"},
        {"role":"user","content":"B"},
        {"role":"developer","content":"tail"}
    ]);
    let outputs = [
        gemini_chat::openai_to_gemini_request(
            &serde_json::from_value(chat(&turns)).unwrap(),
            "gemini",
            &Default::default(),
        )
        .unwrap(),
        gemini_responses::responses_to_gemini_request(
            serde_json::from_value(responses(&turns)).unwrap(),
            "gemini",
            Default::default(),
        )
        .unwrap(),
        claude_gemini::claude_to_gemini_request(
            serde_json::from_value(claude(&turns)).unwrap(),
            "gemini",
            Default::default(),
            &mut flow(),
            &policy(Dialect::Gemini),
        )
        .unwrap(),
    ];
    for output in outputs {
        let wire = serde_json::to_value(output.value).unwrap();
        assert_eq!(text(&wire["systemInstruction"]["parts"]), "initial");
        assert_eq!(
            roles(&wire["contents"]),
            ["user", "model", "user", "user", "user"]
        );
        let texts: Vec<_> = wire["contents"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| text(&m["parts"]))
            .collect();
        assert_eq!(texts, ["A", "answer A", "middle", "B", "tail"]);
        assert_eq!(
            output
                .report
                .diagnostics
                .iter()
                .filter(|d| d.message.contains("instruction role downgraded"))
                .count(),
            2
        );
    }
}

#[test]
fn openai_uses_an_available_instruction_role_before_falling_back_to_user() {
    let turns = json!([
        {"role":"user","content":"A"},
        {"role":"system","content":"system note"},
        {"role":"developer","content":"developer note"},
        {"role":"user","content":"B"}
    ]);
    for (model, expected) in [
        ("o1", ["developer", "developer"]),
        ("openai/o3-2025-04-16", ["developer", "developer"]),
        ("o4-mini", ["developer", "developer"]),
        ("gpt-4-0613", ["system", "system"]),
        ("gpt-4-turbo", ["system", "system"]),
        ("gpt-3.5-turbo", ["system", "system"]),
        ("gpt-4.1", ["system", "developer"]),
        ("gpt-5", ["system", "developer"]),
        ("compatible-model", ["system", "developer"]),
        ("o1-preview-2024-09-12", ["user", "user"]),
        ("o1-mini", ["user", "user"]),
    ] {
        let output = chat_responses::chat_to_responses_request(
            serde_json::from_value(chat(&turns)).unwrap(),
            model,
            &mut flow(),
            &policy(Dialect::OpenAi),
        )
        .unwrap();
        let wire = serde_json::to_value(output.value).unwrap();
        assert_eq!(
            roles(&wire["input"]),
            ["user", expected[0], expected[1], "user"],
            "{model}"
        );
        let output = chat_responses::responses_to_chat_request(
            serde_json::from_value(responses(&turns)).unwrap(),
            model,
        )
        .unwrap();
        let wire = serde_json::to_value(output.value).unwrap();
        assert_eq!(
            roles(&wire["messages"]),
            ["user", expected[0], expected[1], "user"],
            "{model}"
        );
        assert_eq!(wire["messages"][1]["content"], "system note");
        assert_eq!(wire["messages"][2]["content"], "developer note");
    }
    // The typed Responses input-message form must follow the same rule.
    let typed = json!({"model":"alias","input":[
        {"type":"message","role":"user","content":[{"type":"input_text","text":"A"}]},
        {"type":"message","role":"system","content":[{"type":"input_text","text":"note"}]}
    ]});
    let out =
        chat_responses::responses_to_chat_request(serde_json::from_value(typed).unwrap(), "o1")
            .unwrap();
    assert_eq!(
        serde_json::to_value(out.value).unwrap()["messages"][1]["role"],
        "developer"
    );
}

#[test]
fn claude_and_gemini_to_openai_share_the_target_instruction_policy() {
    let turns = json!([{"role":"user","content":"A"},{"role":"system","content":"note"}]);
    let c = serde_json::from_value(claude(&turns)).unwrap();
    let chat_outputs = [
        claude_chat::claude_to_openai(&c, "o1").unwrap().value,
        gemini_chat::gemini_to_openai_request(
            serde_json::from_value(gemini(&turns)).unwrap(),
            "o1",
            &mut flow(),
            &policy(Dialect::OpenAiChat),
        )
        .unwrap()
        .value,
    ];
    for out in chat_outputs {
        let wire = serde_json::to_value(out).unwrap();
        assert_eq!(roles(&wire["messages"]), ["user", "developer"]);
    }
    let outputs = [
        claude_responses::claude_to_responses_request(
            c,
            "o1",
            &mut flow(),
            &policy(Dialect::OpenAi),
        )
        .unwrap()
        .value,
        gemini_responses::gemini_to_responses_request(
            serde_json::from_value(gemini(&turns)).unwrap(),
            "o1",
            &mut flow(),
            &policy(Dialect::OpenAi),
        )
        .unwrap()
        .value,
    ];
    for out in outputs {
        assert_eq!(
            roles(&serde_json::to_value(out).unwrap()["input"]),
            ["user", "developer"]
        );
    }
}

#[test]
fn counting_uses_the_same_instruction_positions_and_roles_as_generation() {
    let turns = json!([
        {"role":"system","content":"initial"},
        {"role":"user","content":"A"},
        {"role":"system","content":"middle"},
        {"role":"assistant","content":"answer"},
        {"role":"developer","content":"tail"}
    ]);
    for model in ["claude-opus-4-8", "claude-sonnet-5"] {
        let generation = to_claude(&turns, model);
        let counts = [
            count::openai_to_claude(
                serde_json::from_value(responses(&turns)).unwrap(),
                model,
                Default::default(),
            )
            .unwrap()
            .value,
            count::gemini_to_claude(
                serde_json::from_value(json!({"contents":gemini(&turns)["contents"]})).unwrap(),
                model,
                &mut flow(),
                &policy(Dialect::Claude),
            )
            .unwrap()
            .value,
        ];
        for (generation, count) in generation[1..].iter().zip(counts) {
            assert_eq!(generation.value.messages, count.messages);
            let generate = serde_json::to_value(&generation.value).unwrap();
            let count = serde_json::to_value(count).unwrap();
            assert_eq!(text(&generate["system"]), text(&count["system"]));
        }
    }
    let outputs = [
        count::openai_to_gemini(
            serde_json::from_value(responses(&turns)).unwrap(),
            "gemini",
            Default::default(),
        )
        .unwrap()
        .value,
        count::claude_to_gemini(
            serde_json::from_value(claude(&turns)).unwrap(),
            "gemini",
            Default::default(),
            &mut flow(),
            &policy(Dialect::Gemini),
        )
        .unwrap()
        .value,
    ];
    for out in outputs {
        let wire = serde_json::to_value(out.generate_content_request.unwrap()).unwrap();
        assert_eq!(text(&wire["systemInstruction"]["parts"]), "initial");
        assert_eq!(roles(&wire["contents"]), ["user", "user", "model", "user"]);
    }
    let outputs = [
        count::claude_to_openai(
            serde_json::from_value(claude(&turns)).unwrap(),
            "o1",
            &mut flow(),
            &policy(Dialect::OpenAi),
        )
        .unwrap()
        .value,
        count::gemini_to_openai(
            serde_json::from_value(json!({"contents":gemini(&turns)["contents"]})).unwrap(),
            "o1",
            &mut flow(),
            &policy(Dialect::OpenAi),
        )
        .unwrap()
        .value,
    ];
    for out in outputs {
        let wire = serde_json::to_value(out).unwrap();
        assert_eq!(
            roles(&wire["input"]),
            ["developer", "user", "developer", "assistant", "developer"]
        );
    }
}
