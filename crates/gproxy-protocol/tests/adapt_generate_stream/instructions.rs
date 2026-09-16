use super::*;
use gproxy_protocol::adapt::generate::{
    claude_gemini::GeminiViaClaude, claude_responses::ResponsesViaClaude,
};

fn target(source: Dialect, _model: &str) -> StreamTarget {
    let namespace = match source {
        Dialect::OpenAiChat => 61,
        Dialect::OpenAi => 63,
        Dialect::Gemini => 65,
        _ => unreachable!(),
    };
    StreamTarget {
        endpoint: Endpoint::new("/messages").unwrap(),
        identities: GenerationIdentity::new(
            IdNamespace([namespace; 16]),
            IdNamespace([namespace + 1; 16]),
            source,
            Dialect::Claude,
        )
        .unwrap(),
    }
}

#[test]
fn all_three_stream_preparations_preserve_instruction_positions() {
    for model in ["claude-opus-4-8", "claude-sonnet-5"] {
        for preceding in ["user", "assistant"] {
            let store = Store::default();
            let mut access = state(&store);
            access.target = IdentityTarget::new(model, Dialect::Claude)
                .unwrap()
                .with_origin("origin")
                .unwrap();
            let messages = json!([
                {"role":"user","content":"A"},
                {"role":preceding,"content":"B"},
                {"role":"system","content":"从现在起只用中文"},
                {"role":"developer","content":"保持简短"}
            ]);
            let chat = ready(ChatViaClaude::prepare_stream(
                serde_json::from_value(json!({"model":"client-alias","stream":true,"max_completion_tokens":32,"messages":messages})).unwrap(),
                target(Dialect::OpenAiChat, model), ClaudeToChatContext { created: 7 }, settings(), &access,
            )).unwrap();
            let responses = ready(ResponsesViaClaude::prepare_stream(
                serde_json::from_value(json!({"model":"client-alias","stream":true,"max_output_tokens":32,"input":messages})).unwrap(),
                target(Dialect::OpenAi, model), ResponsesViaClaudeStreamFacts {
                    request: Default::default(), response: all_pairs::claude_response_context(),
                }, settings(), &access,
            )).unwrap();
            let gemini = ready(GeminiViaClaude::prepare_stream(
                serde_json::from_value(json!({"generationConfig":{"maxOutputTokens":32},"contents":[
                    {"role":"user","parts":[{"text":"A"}]},
                    {"role":if preceding == "assistant" { "model" } else { "user" },"parts":[{"text":"B"}]},
                    {"role":"system","parts":[{"text":"从现在起只用中文"}]},
                    {"role":"system","parts":[{"text":"保持简短"}]}
                ]})).unwrap(),
                target(Dialect::Gemini, model), GeminiViaClaudeStreamFacts { max_tokens: None, response: Default::default() }, settings(), &access,
            )).unwrap();
            let expected = if model == "claude-opus-4-8" && preceding == "user" {
                "system"
            } else {
                "user"
            };
            for request in [
                chat.target_request(),
                responses.target_request(),
                gemini.target_request(),
            ] {
                let wire = serde_json::to_value(request).unwrap();
                assert!(wire.get("system").is_none());
                assert_eq!(wire["model"], model);
                assert_eq!(wire["stream"], true);
                let roles: Vec<_> = wire["messages"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|message| message["role"].as_str().unwrap())
                    .collect();
                assert_eq!(roles, ["user", preceding, expected, expected]);
            }
        }
    }
}
