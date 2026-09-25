//! The inbound session ladder, one case per row of `design/session-identity.md`.

use gproxy_core::SessionSource;
use gproxy_protocol::{Dialect, Operation, OperationKey};
use gproxy_sdk::session::{self, GATEWAY_SESSION_HEADER};
use http::HeaderMap;
use serde_json::{Value, json};

fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
    let mut map = HeaderMap::new();
    for (name, value) in pairs {
        map.insert(
            http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
            http::HeaderValue::from_str(value).unwrap(),
        );
    }
    map
}

fn key(dialect: Dialect) -> OperationKey {
    OperationKey {
        operation: Operation::GenerateContent,
        dialect,
    }
}

fn extract(
    pairs: &[(&str, &str)],
    body: Option<&Value>,
    dialect: Dialect,
) -> Option<(String, SessionSource, Option<String>)> {
    session::extract(&headers(pairs), body, key(dialect))
        .map(|identity| (identity.id, identity.source, identity.field))
}

#[tokio::test]
async fn the_gateway_header_wins_over_every_native_one() {
    let (id, source, field) = extract(
        &[
            ("thread-id", "codex-thread"),
            ("session-id", "codex-session"),
            ("x-claude-code-session-id", "claude"),
            (GATEWAY_SESSION_HEADER, "ours"),
        ],
        None,
        Dialect::OpenAi,
    )
    .expect("the gateway header is the first rung");
    assert_eq!(id, "ours");
    assert_eq!(source, SessionSource::Gateway);
    assert_eq!(field.as_deref(), Some(GATEWAY_SESSION_HEADER));
    // Not concatenated with the client's own id.
    assert!(!id.contains("codex"));
}

#[tokio::test]
async fn the_native_headers_are_consulted_in_order() {
    // A thread is a finer conversation than the session that contains it.
    let (id, source, _) = extract(
        &[("thread-id", "t-1"), ("session-id", "s-1")],
        None,
        Dialect::OpenAi,
    )
    .unwrap();
    assert_eq!((id.as_str(), source), ("t-1", SessionSource::CodexThread));

    for (header, value, expected) in [
        ("session-id", "s-1", SessionSource::CodexSession),
        (
            "x-claude-code-session-id",
            "cc-1",
            SessionSource::ClaudeCode,
        ),
        ("x-conversation-id", "wb-1", SessionSource::WorkBuddy),
        ("x-grok-session-id", "gb-1", SessionSource::GrokBuild),
    ] {
        let (id, source, field) = extract(&[(header, value)], None, Dialect::OpenAi).unwrap();
        assert_eq!((id.as_str(), source), (value, expected));
        assert_eq!(field.as_deref(), Some(header));
    }
}

#[tokio::test]
async fn a_blank_header_is_not_a_session() {
    assert_eq!(
        extract(
            &[("thread-id", "   "), ("session-id", "s-1")],
            None,
            Dialect::OpenAi
        )
        .map(|(id, ..)| id),
        Some("s-1".into()),
        "an empty value is absent, not an empty session id"
    );
}

#[tokio::test]
async fn responses_reads_client_metadata_thread_then_session() {
    let body = json!({"client_metadata": {"thread_id": "t-9", "session_id": "s-9"}});
    let (id, source, field) = extract(&[], Some(&body), Dialect::OpenAi).unwrap();
    assert_eq!((id.as_str(), source), ("t-9", SessionSource::CodexThread));
    assert_eq!(field.as_deref(), Some("client_metadata.thread_id"));

    let body = json!({"client_metadata": {"session_id": "s-9"}});
    let (id, source, field) = extract(&[], Some(&body), Dialect::OpenAi).unwrap();
    assert_eq!((id.as_str(), source), ("s-9", SessionSource::CodexSession));
    assert_eq!(field.as_deref(), Some("client_metadata.session_id"));
}

#[tokio::test]
async fn the_websocket_envelope_reads_the_responses_fields() {
    let body = json!({"client_metadata": {"thread_id": "t-ws"}});
    let (id, source, _) = extract(&[], Some(&body), Dialect::OpenAiResponsesWebSocket).unwrap();
    assert_eq!((id.as_str(), source), ("t-ws", SessionSource::CodexThread));
}

#[tokio::test]
async fn claude_parses_the_session_out_of_the_encoded_user_id() {
    // `user_id` is a JSON-encoded string; the session is a field inside it.
    let encoded = json!({"account_uuid": "acct-1", "session_id": "sess-7"}).to_string();
    let body = json!({"metadata": {"user_id": encoded}});
    let (id, source, field) = extract(&[], Some(&body), Dialect::Claude).unwrap();
    assert_eq!((id.as_str(), source), ("sess-7", SessionSource::ClaudeCode));
    assert_eq!(field.as_deref(), Some("metadata.user_id.session_id"));

    // The whole value is an account identifier and is never the session.
    let body = json!({"metadata": {"user_id": "user_abc123"}});
    assert_eq!(
        session::from_body(Dialect::Claude, &body).map(|s| s.id),
        None,
        "an opaque user id carries no session"
    );
    let body = json!({"metadata": {"user_id": json!({"account_uuid": "a"}).to_string()}});
    assert_eq!(
        session::from_body(Dialect::Claude, &body).map(|s| s.id),
        None
    );
}

#[tokio::test]
async fn gemini_reads_both_spellings_of_the_wrapper_field() {
    let body = json!({"request": {"session_id": "g-1"}});
    let (id, source, field) = extract(&[], Some(&body), Dialect::Gemini).unwrap();
    assert_eq!((id.as_str(), source), ("g-1", SessionSource::GeminiCli));
    assert_eq!(field.as_deref(), Some("request.session_id"));

    let body = json!({"request": {"sessionId": "a-1"}});
    let (id, source, field) = extract(&[], Some(&body), Dialect::Gemini).unwrap();
    assert_eq!((id.as_str(), source), ("a-1", SessionSource::Antigravity));
    assert_eq!(field.as_deref(), Some("request.sessionId"));
}

#[tokio::test]
async fn a_body_field_of_another_dialect_is_not_read() {
    // Codex's field in a Claude body, and Claude's in a Responses body.
    let body = json!({"client_metadata": {"thread_id": "t-1"}});
    assert!(session::from_body(Dialect::Claude, &body).is_none());
    let body = json!({"metadata": {"user_id": json!({"session_id": "s"}).to_string()}});
    assert!(session::from_body(Dialect::OpenAi, &body).is_none());
    // And a business payload that merely contains the words is not searched.
    let body = json!({"input": [{"role": "user", "content": [{"session_id": "not-a-session"}]}]});
    assert!(session::from_body(Dialect::OpenAi, &body).is_none());
}

#[tokio::test]
async fn request_and_turn_identifiers_are_never_sessions() {
    // Every one of these is explicitly excluded by the design.
    let excluded = headers(&[
        ("x-grok-req-id", "r-1"),
        ("x-grok-turn-idx", "3"),
        ("x-request-id", "r-2"),
        ("x-conversation-request-id", "r-3"),
        ("x-conversation-message-id", "r-4"),
    ]);
    assert!(session::from_headers(&excluded).is_none());

    let body = json!({
        "user_prompt_id": "p-1",
        "prompt_cache_key": "c-1",
        "previous_response_id": "resp_1",
        "requestId": "r-5",
    });
    assert!(session::from_body(Dialect::OpenAi, &body).is_none());
    assert!(session::from_body(Dialect::Gemini, &body).is_none());
    assert!(
        session::extract(&excluded, Some(&body), key(Dialect::OpenAi)).is_none(),
        "not even as a fingerprint: none of this is conversation text"
    );
}

#[tokio::test]
async fn the_fingerprint_survives_another_turn_but_not_another_prompt() {
    let first = json!({
        "instructions": "You are a careful assistant.",
        "input": [{"role": "user", "content": "hello"}],
    });
    let second = json!({
        "instructions": "You are a careful assistant.",
        "input": [
            {"role": "user", "content": "hello"},
            {"role": "assistant", "content": "hi"},
            {"role": "user", "content": "and again"},
        ],
    });
    let a = session::fingerprint(Dialect::OpenAi, &first).unwrap();
    let b = session::fingerprint(Dialect::OpenAi, &second).unwrap();
    assert_eq!(a.id, b.id, "an added turn is the same conversation");
    assert_eq!(a.source, SessionSource::ConversationFingerprint);
    assert_eq!(a.id.len(), 32);

    let other =
        json!({"instructions": "You are a careful assistant.", "input": "different question"});
    assert_ne!(
        a.id,
        session::fingerprint(Dialect::OpenAi, &other).unwrap().id
    );
}

#[tokio::test]
async fn the_fingerprint_reads_each_dialect_s_own_prefix() {
    let claude = json!({
        "system": [{"type": "text", "text": "Be brief."}],
        "messages": [{"role": "user", "content": "hi"}],
    });
    let gemini = json!({
        "request": {
            "systemInstruction": {"parts": [{"text": "Be brief."}]},
            "contents": [{"role": "user", "parts": [{"text": "hi"}]}],
        }
    });
    let chat = json!({
        "messages": [
            {"role": "system", "content": "Be brief."},
            {"role": "user", "content": "hi"},
        ]
    });
    let claude_id = session::fingerprint(Dialect::Claude, &claude).unwrap().id;
    let gemini_id = session::fingerprint(Dialect::Gemini, &gemini).unwrap().id;
    let chat_id = session::fingerprint(Dialect::OpenAiChat, &chat).unwrap().id;
    assert_eq!(claude_id.len(), 32);
    // The same instruction text in three dialects is three conversations.
    assert_ne!(claude_id, gemini_id);
    assert_ne!(claude_id, chat_id);
    assert_ne!(gemini_id, chat_id);
}

#[tokio::test]
async fn a_body_without_a_stable_prefix_has_no_fingerprint() {
    for (dialect, body) in [
        (
            Dialect::OpenAi,
            json!({"instructions": "shared system", "input": []}),
        ),
        (
            Dialect::Claude,
            json!({"system": "shared system", "messages": []}),
        ),
        (Dialect::OpenAiChat, json!({"messages": []})),
        (Dialect::Gemini, json!({"contents": []})),
    ] {
        assert!(
            session::fingerprint(dialect, &body).is_none(),
            "{dialect:?} has nothing stable to hash"
        );
    }
}

#[tokio::test]
async fn a_native_header_beats_the_body_and_the_body_beats_the_fingerprint() {
    let body = json!({
        "instructions": "Be brief.",
        "client_metadata": {"thread_id": "t-body"},
    });
    let (id, source, _) =
        extract(&[("thread-id", "t-header")], Some(&body), Dialect::OpenAi).unwrap();
    assert_eq!(
        (id.as_str(), source),
        ("t-header", SessionSource::CodexThread)
    );

    let (id, source, _) = extract(&[], Some(&body), Dialect::OpenAi).unwrap();
    assert_eq!(
        (id.as_str(), source),
        ("t-body", SessionSource::CodexThread)
    );

    let bare = json!({"instructions": "Be brief.", "input": "hi"});
    let (_, source, _) = extract(&[], Some(&bare), Dialect::OpenAi).unwrap();
    assert_eq!(source, SessionSource::ConversationFingerprint);
}

#[tokio::test]
async fn the_gateway_header_is_stripped_and_nothing_else_is() {
    let mut map = headers(&[
        (GATEWAY_SESSION_HEADER, "ours"),
        ("thread-id", "t-1"),
        ("authorization", "Bearer x"),
    ]);
    session::strip_gateway_header(&mut map);
    assert!(map.get(GATEWAY_SESSION_HEADER).is_none());
    assert!(
        map.get("thread-id").is_some(),
        "native fields are the channel's business"
    );
    assert!(map.get("authorization").is_some());
}

#[test]
fn legacy_headers_and_opencode_remain_supported() {
    for (header, source) in [
        ("x-opencode-session", SessionSource::OpenCode),
        ("x-session-id", SessionSource::Generic),
        ("x-session-affinity", SessionSource::Generic),
        ("session_id", SessionSource::ClaudeCode),
    ] {
        let (id, actual, _) = extract(&[(header, "session")], None, Dialect::OpenAi).unwrap();
        assert_eq!(id, "session");
        assert_eq!(actual, source);
    }
    assert_eq!(
        extract(
            &[
                ("x-opencode-session", "opencode"),
                ("session-id", "generic")
            ],
            None,
            Dialect::OpenAi
        )
        .unwrap()
        .0,
        "opencode"
    );
}

#[test]
fn fingerprint_includes_first_user_and_preceding_tool_items_without_instructions() {
    let first = json!({"input": [
        {"type":"function_call_output", "call_id":"c", "output":"context"},
        {"role":"user", "content":[{"type":"input_text", "text":"question"}]}
    ]});
    let mut next = first.clone();
    next["input"]
        .as_array_mut()
        .unwrap()
        .push(json!({"role":"assistant", "content":"answer"}));
    let id = session::fingerprint(Dialect::OpenAi, &first).unwrap().id;
    assert_eq!(
        id,
        session::fingerprint(Dialect::OpenAiResponsesWebSocket, &next)
            .unwrap()
            .id
    );
    next["input"][0]["output"] = json!("other context");
    assert_ne!(id, session::fingerprint(Dialect::OpenAi, &next).unwrap().id);
    next = first.clone();
    next["input"][1]["content"][0]["text"] = json!("other question");
    assert_ne!(id, session::fingerprint(Dialect::OpenAi, &next).unwrap().id);
    assert!(session::fingerprint(Dialect::OpenAi, &json!({"input":"hello"})).is_some());
}

#[test]
fn large_shared_system_prompts_do_not_hide_different_first_users() {
    let mut body = json!({"instructions":"x".repeat(40_000), "input":"question a"});
    let a = session::fingerprint(Dialect::OpenAi, &body).unwrap().id;
    body["input"] = json!("question b");
    assert_ne!(a, session::fingerprint(Dialect::OpenAi, &body).unwrap().id);
}
