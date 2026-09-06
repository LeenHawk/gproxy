use bytes::Bytes;
use gproxy_channel_api::Channel;
use http::{HeaderMap, Method, StatusCode};
use serde_json::json;

use super::block_on;
use super::memory::MemoryHost;
use crate::boundary::{RequestCtx, RoutingMode};
use crate::control::{FailoverBudget, Plan, ProviderRef, Target};
use crate::host::CredentialId;
use crate::{Core, InitError};

#[test]
fn explicit_session_pins_winner_and_rebinds_after_failover() -> Result<(), InitError> {
    let host = MemoryHost::new(false);
    let core = core(&host)?;

    set_plan(&host, [target(7), target(8)]);
    execute(&core, &host, request("a", "question", false));
    assert_eq!(take_loaded(&host), vec![CredentialId(7)]);

    set_plan(&host, [target(8), target(7)]);
    execute(&core, &host, request("a", "question", true));
    assert_eq!(take_loaded(&host), vec![CredentialId(7)]);

    execute(&core, &host, request("b", "question", false));
    assert_eq!(take_loaded(&host), vec![CredentialId(8)]);
    let affinities = host
        .state
        .lock()
        .expect("state lock")
        .resolved_affinities
        .clone();
    assert_eq!(affinities[0], affinities[1]);
    assert_ne!(affinities[1], affinities[2]);

    set_plan(&host, [target(7), target(8)]);
    host.state.lock().expect("state lock").statuses =
        [StatusCode::TOO_MANY_REQUESTS, StatusCode::OK].into();
    execute(&core, &host, request("a", "question", true));
    assert_eq!(take_loaded(&host), vec![CredentialId(7), CredentialId(8)]);

    execute(&core, &host, request("a", "question", true));
    assert_eq!(take_loaded(&host), vec![CredentialId(8)]);
    assert!(
        host.state
            .lock()
            .expect("state lock")
            .cache_ttls
            .values()
            .all(|ttl| *ttl == 3_600)
    );
    Ok(())
}

#[test]
fn claude_session_header_drives_affinity_and_override_header_is_not_forwarded()
-> Result<(), InitError> {
    let host = MemoryHost::new(false);
    let core = core(&host)?;
    set_plan(&host, [target(7), target(8)]);

    let mut first = claude_request("claude-session", "hello", false);
    first
        .headers
        .insert("x-gproxy-session-id", "operator-session".parse().unwrap());
    execute(&core, &host, first);
    assert_eq!(take_loaded(&host), vec![CredentialId(7)]);
    assert!(
        host.state.lock().expect("state lock").upstream_requests[0]
            .0
            .get("x-gproxy-session-id")
            .is_none()
    );

    set_plan(&host, [target(8), target(7)]);
    let mut second = claude_request("other-native-session", "hello", true);
    second
        .headers
        .insert("x-gproxy-session-id", "operator-session".parse().unwrap());
    execute(&core, &host, second);
    assert_eq!(take_loaded(&host), vec![CredentialId(7)]);

    set_plan(&host, [target(8), target(7)]);
    execute(
        &core,
        &host,
        claude_request("claude-native-only", "hello", false),
    );
    assert_eq!(take_loaded(&host), vec![CredentialId(8)]);
    Ok(())
}

#[test]
fn conversation_head_fingerprint_ignores_appended_turns() -> Result<(), InitError> {
    let host = MemoryHost::new(false);
    let core = core(&host)?;
    set_plan(&host, [target(7), target(8)]);
    execute(&core, &host, request("", "same head", false));
    assert_eq!(take_loaded(&host), vec![CredentialId(7)]);

    set_plan(&host, [target(8), target(7)]);
    execute(&core, &host, request("", "same head", true));
    assert_eq!(take_loaded(&host), vec![CredentialId(7)]);

    execute(&core, &host, request("", "different head", false));
    assert_eq!(take_loaded(&host), vec![CredentialId(8)]);
    Ok(())
}

#[test]
fn openai_native_session_header_overrides_conversation_changes() -> Result<(), InitError> {
    let host = MemoryHost::new(false);
    let core = core(&host)?;
    set_plan(&host, [target(7), target(8)]);
    execute(
        &core,
        &host,
        openai_session_request("codex-session", "first question"),
    );
    assert_eq!(take_loaded(&host), vec![CredentialId(7)]);

    set_plan(&host, [target(8), target(7)]);
    execute(
        &core,
        &host,
        openai_session_request("codex-session", "unrelated new head"),
    );
    assert_eq!(take_loaded(&host), vec![CredentialId(7)]);

    execute(
        &core,
        &host,
        openai_session_request("other-session", "unrelated new head"),
    );
    assert_eq!(take_loaded(&host), vec![CredentialId(8)]);
    Ok(())
}

#[test]
fn opencode_session_survives_turns_retry_targets_and_ingress_filtering() -> Result<(), InitError> {
    let host = MemoryHost::new(false);
    let channels =
        gproxy_channel_api::ChannelRegistry::new([
            Box::new(gproxy_channels::OpenCodeChannel) as Box<dyn Channel>
        ])
        .expect("channel registry");
    let core = Core::new(host.clone(), channels)?;
    let mut primary = target(7);
    primary.provider.channel = "opencode".into();
    primary.provider.settings = json!({"tier":"go"});
    let mut secondary = primary.clone();
    secondary.credential = CredentialId(8);
    host.state.lock().expect("state lock").credential.channel = "opencode".into();
    set_plan(&host, [primary.clone(), secondary.clone()]);
    let run = |request| {
        execute(&core, &host, request);
        host.state
            .lock()
            .expect("state lock")
            .upstream_requests
            .last()
            .unwrap()
            .0["x-opencode-session"]
            .clone()
    };
    let first = run(request("", "question", false));
    assert_eq!(first, run(request("", "question", true)));
    assert_ne!(first, run(request("", "another conversation", false)));
    host.state.lock().expect("state lock").caller_user_id += 1;
    assert_ne!(first, run(request("", "question", false)));

    let explicit = |head| {
        let mut request = request("", head, false);
        request
            .headers
            .insert("x-opencode-session", "client-conversation".parse().unwrap());
        request
    };
    assert_eq!(run(explicit("question")), "client-conversation");
    assert_eq!(run(explicit("different head")), "client-conversation");

    for header in [
        "session-id",
        "x-session-id",
        "thread-id",
        "x-session-affinity",
    ] {
        let mut first = request("", "question", false);
        first
            .headers
            .insert(header, "native-session".parse().unwrap());
        let session = run(first);
        let mut next = claude_request("", "changed head", true);
        next.headers
            .insert(header, "native-session".parse().unwrap());
        assert_eq!(session, run(next));
    }

    let fixed = run(request("gateway-session", "question", false));
    set_plan(&host, [secondary, primary]);
    host.state.lock().expect("state lock").statuses =
        [StatusCode::TOO_MANY_REQUESTS, StatusCode::OK].into();
    assert_eq!(fixed, run(request("gateway-session", "changed head", true)));
    let state = host.state.lock().expect("state lock");
    for (headers, _) in state.upstream_requests.iter().rev().take(2) {
        assert_eq!(headers["x-opencode-session"], fixed);
        assert!(!headers.contains_key("x-gproxy-session-id"));
    }
    Ok(())
}

fn core(host: &MemoryHost) -> Result<Core<MemoryHost>, InitError> {
    let channels =
        gproxy_channel_api::ChannelRegistry::new([Box::new(host.clone()) as Box<dyn Channel>])
            .expect("channel registry");
    Core::new(host.clone(), channels)
}

fn set_plan(host: &MemoryHost, targets: [Target; 2]) {
    host.state.lock().expect("state lock").plan = Some(Plan {
        targets: targets.into(),
        budget: FailoverBudget { max_attempts: 2 },
    });
}

fn execute(core: &Core<MemoryHost>, host: &MemoryHost, request: RequestCtx) {
    let outcome = block_on(core.execute(host, request)).expect("session request");
    assert_eq!(outcome.status, StatusCode::OK);
}

fn take_loaded(host: &MemoryHost) -> Vec<CredentialId> {
    let mut loaded = std::mem::take(&mut host.state.lock().expect("state lock").loaded_credentials);
    loaded.dedup();
    loaded
}

fn request(session: &str, first_user: &str, with_tail: bool) -> RequestCtx {
    let mut input = vec![json!({"role":"user","content":first_user})];
    if with_tail {
        input.extend([
            json!({"role":"assistant","content":[{"type":"output_text","text":"answer"}]}),
            json!({"role":"user","content":"follow up"}),
        ]);
    }
    let mut headers = HeaderMap::new();
    if !session.is_empty() {
        headers.insert("x-gproxy-session-id", session.parse().unwrap());
    }
    RequestCtx {
        request_id: format!("request-{session}-{with_tail}"),
        client_ip: None,
        method: Method::POST,
        path: "/v1/responses".into(),
        query: None,
        headers,
        body: Bytes::from(
            serde_json::to_vec(&json!({
                "model": "alias",
                "input": input,
                "stream": false
            }))
            .unwrap(),
        ),
        upgrade: false,
        force_model_refresh: false,
        mode: RoutingMode::Aggregated,
    }
}

fn openai_session_request(session: &str, first_user: &str) -> RequestCtx {
    let mut request = request("", first_user, false);
    request
        .headers
        .insert("session-id", session.parse().expect("session-id"));
    request
}

fn claude_request(session: &str, first_user: &str, with_tail: bool) -> RequestCtx {
    let mut messages = vec![json!({"role":"user","content":first_user})];
    if with_tail {
        messages.extend([
            json!({"role":"assistant","content":"answer"}),
            json!({"role":"user","content":"follow up"}),
        ]);
    }
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-claude-code-session-id",
        session.parse().expect("session header"),
    );
    headers.insert("anthropic-version", "2023-06-01".parse().unwrap());
    RequestCtx {
        request_id: format!("request-{session}-{with_tail}"),
        client_ip: None,
        method: Method::POST,
        path: "/v1/messages".into(),
        query: None,
        headers,
        body: Bytes::from(
            serde_json::to_vec(&json!({
                "model": "alias",
                "messages": messages,
                "max_tokens": 32,
                "stream": false
            }))
            .unwrap(),
        ),
        upgrade: false,
        force_model_refresh: false,
        mode: RoutingMode::Aggregated,
    }
}

fn target(credential: i64) -> Target {
    Target {
        provider: ProviderRef {
            id: 3,
            name: "provider".into(),
            channel: "memory".into(),
            settings: json!({}),
            fingerprint: None,
            proxy_url: None,
            traffic_blacklist: Default::default(),
        },
        credential: CredentialId(credential),
        upstream_model: "upstream-model".into(),
        tier: 0,
        rules: Default::default(),
    }
}
