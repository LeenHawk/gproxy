//! The engine end to end: a request in, a fake upstream on a local port, the
//! answer out. Nothing here reaches a real provider.

mod support;

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use gproxy_lite::{Engine, KeyValue, MemoryState};
use gproxy_protocol::capability::{
    CapabilityError, CapabilityFuture, CapabilityLimits, CasResult, StateEntry, StateWrite, Version,
};
use serde_json::{Value, json};
use support::{
    ACCESS_KEY, Answer, Seen, Upstream, engine, json as read_json, memory, request, text,
};

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

fn sse(event: Option<&str>, data: Value) -> String {
    match event {
        Some(event) => format!("event: {event}\ndata: {data}\n\n"),
        None => format!("data: {data}\n\n"),
    }
}

/// OpenAI Chat Completions and Claude Messages, buffered and streamed.
fn model_api(seen: &Seen) -> Answer {
    let streaming = seen.body["stream"] == true;
    match (seen.path.as_str(), streaming) {
        ("/v1/chat/completions", false) => Answer::Json(
            200,
            json!({
                "id": "chatcmpl-1", "object": "chat.completion", "created": 1,
                "model": seen.body["model"],
                "choices": [{"index": 0, "finish_reason": "stop",
                    "message": {"role": "assistant", "content": "Hello from OpenAI"}}],
                "usage": {"prompt_tokens": 1, "completion_tokens": 2, "total_tokens": 3}
            }),
        ),
        ("/v1/chat/completions", true) => {
            let chunk = |delta: Value, finish: Value| {
                sse(
                    None,
                    json!({
                        "id": "chatcmpl-1", "object": "chat.completion.chunk", "created": 1,
                        "model": "gpt-test",
                        "choices": [{"index": 0, "delta": delta, "finish_reason": finish}]
                    }),
                )
            };
            Answer::Events(vec![
                chunk(
                    json!({"role": "assistant", "content": "Hello"}),
                    Value::Null,
                ),
                chunk(json!({"content": " from OpenAI"}), Value::Null),
                chunk(json!({}), json!("stop")),
                "data: [DONE]\n\n".to_owned(),
            ])
        }
        ("/v1/messages", false) => Answer::Json(
            200,
            json!({
                "id": "msg_1", "type": "message", "role": "assistant", "model": "claude-test",
                "content": [{"type": "text", "text": "Hello from Claude"}],
                "stop_reason": "end_turn", "stop_sequence": null,
                "usage": {"input_tokens": 3, "output_tokens": 4}
            }),
        ),
        ("/v1/messages", true) => Answer::Events(vec![
            sse(
                Some("message_start"),
                json!({"type": "message_start", "message": {
                "id": "msg_1", "type": "message", "role": "assistant", "model": "claude-test",
                "content": [], "stop_reason": null, "stop_sequence": null,
                "usage": {"input_tokens": 3, "output_tokens": 0}}}),
            ),
            sse(
                Some("content_block_start"),
                json!({"type": "content_block_start", "index": 0,
                "content_block": {"type": "text", "text": ""}}),
            ),
            sse(
                Some("content_block_delta"),
                json!({"type": "content_block_delta", "index": 0,
                "delta": {"type": "text_delta", "text": "Hello from Claude"}}),
            ),
            sse(
                Some("content_block_stop"),
                json!({"type": "content_block_stop", "index": 0}),
            ),
            sse(
                Some("message_delta"),
                json!({"type": "message_delta",
                "delta": {"stop_reason": "end_turn", "stop_sequence": null},
                "usage": {"output_tokens": 4}}),
            ),
            sse(Some("message_stop"), json!({"type": "message_stop"})),
        ]),
        _ => Answer::Json(404, json!({"error": {"message": "no such path"}})),
    }
}

/// An OpenAI-compatible provider `oa` and a Claude-compatible provider `cl`.
fn two_providers(openai: &Upstream, claude: &Upstream) -> Value {
    json!({
        "access_key": ACCESS_KEY,
        "providers": [
            {"name": "oa", "channel": "custom", "base_url": openai.url,
             "config": {"dialects": ["openai_chat"]}, "credential": "openai-upstream-key"},
            {"name": "cl", "channel": "custom", "base_url": claude.url,
             "config": {"dialects": ["claude"]}, "credential": "claude-upstream-key"}
        ]
    })
}

fn chat(model: &str, stream: bool) -> Value {
    json!({"model": model, "stream": stream, "messages": [{"role": "user", "content": "Hi"}]})
}

#[tokio::test]
async fn a_request_without_the_access_key_reaches_nothing() {
    let upstream = Upstream::start(model_api).await;
    let engine = engine(two_providers(&upstream, &upstream), memory()).await;
    for key in [Some("wrong-key"), None] {
        let mut wrong = request(
            "POST",
            "/v1/chat/completions",
            Some(chat("oa/gpt-test", false)),
        );
        wrong.headers_mut().remove("authorization");
        if let Some(key) = key {
            wrong
                .headers_mut()
                .insert("authorization", format!("Bearer {key}").parse().unwrap());
        }
        let (status, body) = read_json(engine.handle(wrong).await).await;
        assert_eq!(status, 401);
        assert_eq!(body["error"]["type"], "authentication_error");
    }
    // The management paths are behind the same key.
    let mut status = request("GET", "/lite/status", None);
    status.headers_mut().remove("authorization");
    assert_eq!(engine.handle(status).await.status, 401);
    assert!(upstream.seen().is_empty());
}

#[tokio::test]
async fn the_upstreams_own_dialect_passes_through() {
    let upstream = Upstream::start(model_api).await;
    let engine = engine(two_providers(&upstream, &upstream), memory()).await;

    // `provider/model` in the model field.
    let response = engine
        .handle(request(
            "POST",
            "/v1/chat/completions",
            Some(chat("oa/gpt-test", false)),
        ))
        .await;
    let (status, body) = read_json(response).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        body["choices"][0]["message"]["content"],
        "Hello from OpenAI"
    );

    // A provider path, with the model as the upstream names it.
    let response = engine
        .handle(request(
            "POST",
            "/oa/v1/chat/completions",
            Some(chat("gpt-test", false)),
        ))
        .await;
    assert_eq!(response.status, 200);

    let seen = upstream.seen();
    assert_eq!(seen.len(), 2);
    for request in &seen {
        assert_eq!(request.path, "/v1/chat/completions");
        assert_eq!(request.body["model"], "gpt-test");
        // The upstream's key, never the caller's.
        assert_eq!(
            request.header("authorization"),
            Some("Bearer openai-upstream-key")
        );
    }

    // Two providers and no way to tell which: refused, not guessed.
    let response = engine
        .handle(request(
            "POST",
            "/v1/chat/completions",
            Some(chat("gpt-test", false)),
        ))
        .await;
    assert_eq!(response.status, 400);
}

#[tokio::test]
async fn another_dialect_is_converted_both_ways() {
    let upstream = Upstream::start(model_api).await;
    let engine = engine(two_providers(&upstream, &upstream), memory()).await;
    let response = engine
        .handle(request(
            "POST",
            "/v1/chat/completions",
            Some(chat("cl/claude-test", false)),
        ))
        .await;
    let (status, body) = read_json(response).await;
    assert_eq!(status, 200, "{body}");
    // The client asked in Chat Completions and is answered in it.
    assert_eq!(body["object"], "chat.completion");
    assert_eq!(
        body["choices"][0]["message"]["content"],
        "Hello from Claude"
    );
    assert_eq!(body["choices"][0]["finish_reason"], "stop");

    // The upstream was asked in Claude Messages.
    let seen = upstream.seen();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].path, "/v1/messages");
    assert_eq!(seen[0].header("x-api-key"), Some("claude-upstream-key"));
    assert_eq!(seen[0].body["model"], "claude-test");
    assert_eq!(seen[0].body["messages"][0]["role"], "user");
    assert!(seen[0].body["max_tokens"].is_number());
}

#[tokio::test]
async fn a_stream_is_relayed_and_a_converted_stream_is_the_clients_dialect() {
    let upstream = Upstream::start(model_api).await;
    let engine = engine(two_providers(&upstream, &upstream), memory()).await;

    let response = engine
        .handle(request(
            "POST",
            "/v1/chat/completions",
            Some(chat("oa/gpt-test", true)),
        ))
        .await;
    assert_eq!(response.headers["content-type"], "text/event-stream");
    let (status, body) = text(response).await;
    assert_eq!(status, 200);
    assert!(body.contains(r#""content":" from OpenAI""#), "{body}");
    assert!(body.trim_end().ends_with("data: [DONE]"), "{body}");

    let response = engine
        .handle(request(
            "POST",
            "/v1/chat/completions",
            Some(chat("cl/claude-test", true)),
        ))
        .await;
    assert_eq!(response.headers["content-type"], "text/event-stream");
    let (status, body) = text(response).await;
    assert_eq!(status, 200);
    // Claude's events arrive as Chat Completions chunks.
    assert!(body.contains("chat.completion.chunk"), "{body}");
    assert!(body.contains("Hello from Claude"), "{body}");
    assert!(!body.contains("content_block_delta"), "{body}");
    assert!(body.trim_end().ends_with("data: [DONE]"), "{body}");
    assert_eq!(upstream.seen()[1].body["stream"], true);
}

/// A token endpoint in front of `api`. The sign-in issues a token that
/// expires at `signed_in_until_ms`; every renewal issues the next one, good
/// for an hour.
fn oauth_with(
    signed_in_until_ms: i64,
    api: impl Fn(&Seen) -> Answer + Send + Sync + 'static,
) -> impl Fn(&Seen) -> Answer + Send + Sync + 'static {
    let issued = AtomicUsize::new(0);
    move |seen| {
        if seen.path != "/oauth/token" {
            return api(seen);
        }
        let number = issued.fetch_add(1, Ordering::SeqCst) + 1;
        let expires_at_ms = match seen.body["grant_type"].as_str() {
            Some("authorization_code") if seen.body["code"] == "good-code" => signed_in_until_ms,
            Some("refresh_token") => now_ms() + 3_600_000,
            _ => return Answer::Json(400, json!({"error": "invalid_grant"})),
        };
        Answer::Json(
            200,
            json!({"access_token": format!("access-{number}"), "expires_at_ms": expires_at_ms}),
        )
    }
}

/// The first token, from the sign-in, is already expired.
fn oauth_api() -> impl Fn(&Seen) -> Answer + Send + Sync + 'static {
    oauth_with(1, model_api)
}

fn oauth_provider(upstream: &Upstream) -> Value {
    json!({
        "access_key": ACCESS_KEY,
        "providers": [{"name": "acct", "channel": "fake_oauth", "base_url": upstream.url}]
    })
}

async fn start_login(engine: &Engine, provider: &str) -> (String, String) {
    let response = engine
        .handle(request(
            "POST",
            "/lite/login/start",
            Some(json!({"provider": provider})),
        ))
        .await;
    let (status, started) = read_json(response).await;
    assert_eq!(status, 200, "{started}");
    assert_eq!(started["mode"], "authorization_code");
    let url = started["authorize_url"].as_str().unwrap();
    let state = url
        .split("state=")
        .nth(1)
        .unwrap()
        .split('&')
        .next()
        .unwrap();
    (
        started["session"].as_str().unwrap().to_owned(),
        state.to_owned(),
    )
}

async fn credential_state(engine: &Engine) -> Value {
    let (_, status) = read_json(engine.handle(request("GET", "/lite/status", None)).await).await;
    status["providers"][0]["credential"].clone()
}

#[tokio::test]
async fn signing_in_takes_the_pasted_callback_url_or_the_bare_code() {
    let upstream = Upstream::start(oauth_api()).await;

    // The whole callback URL, as copied from the address bar.
    let by_url = engine(oauth_provider(&upstream), memory()).await;
    assert_eq!(credential_state(&by_url).await, "none");
    let unsigned = by_url
        .handle(request(
            "POST",
            "/v1/chat/completions",
            Some(chat("gpt-test", false)),
        ))
        .await;
    assert_eq!(unsigned.status, 503);
    let (session, state) = start_login(&by_url, "acct").await;
    let callback = format!("http://localhost:1455/callback?code=good-code&state={state}");
    let response = by_url
        .handle(request(
            "POST",
            "/lite/login/complete",
            Some(json!({"session": session, "callback_url": callback})),
        ))
        .await;
    let (status, body) = read_json(response).await;
    assert_eq!((status, &body["status"]), (200, &json!("ready")), "{body}");
    assert_eq!(credential_state(&by_url).await, "signed_in");
    // The session is spent.
    let again = by_url
        .handle(request(
            "POST",
            "/lite/login/complete",
            Some(json!({"session": session, "callback_url": callback})),
        ))
        .await;
    assert_eq!(again.status, 410);

    // The bare code.
    let by_code = engine(oauth_provider(&upstream), memory()).await;
    let (session, _) = start_login(&by_code, "acct").await;
    let response = by_code
        .handle(request(
            "POST",
            "/lite/login/complete",
            Some(json!({"session": session, "code": "good-code"})),
        ))
        .await;
    assert_eq!(response.status, 200);
    assert_eq!(credential_state(&by_code).await, "signed_in");

    // The exchange carried the verifier whose challenge the URL showed.
    let exchanges: Vec<_> = upstream
        .seen()
        .into_iter()
        .filter(|seen| seen.path == "/oauth/token")
        .collect();
    assert_eq!(exchanges.len(), 2);
    assert!(exchanges.iter().all(|seen| {
        seen.body["code_verifier"]
            .as_str()
            .is_some_and(|v| v.len() >= 43)
            && seen.body["redirect_uri"] == "http://localhost:1455/callback"
    }));

    // A callback answering another login is refused and ends the session.
    let forged = engine(oauth_provider(&upstream), memory()).await;
    let (session, _) = start_login(&forged, "acct").await;
    let complete = json!({"session": session, "callback_url": "?code=good-code&state=not-ours"});
    let response = forged
        .handle(request(
            "POST",
            "/lite/login/complete",
            Some(complete.clone()),
        ))
        .await;
    assert_eq!(response.status, 400);
    let response = forged
        .handle(request("POST", "/lite/login/complete", Some(complete)))
        .await;
    assert_eq!(response.status, 410);
    assert_eq!(credential_state(&forged).await, "none");
}

async fn sign_in(engine: &Engine, provider: &str) {
    let (session, _) = start_login(engine, provider).await;
    let response = engine
        .handle(request(
            "POST",
            "/lite/login/complete",
            Some(json!({"session": session, "code": "good-code"})),
        ))
        .await;
    assert_eq!(response.status, 200);
}

async fn signed_in(upstream: &Upstream, state: Arc<dyn KeyValue>) -> Engine {
    let engine = engine(oauth_provider(upstream), state).await;
    sign_in(&engine, "acct").await;
    engine
}

async fn chat_status(engine: &Engine) -> u16 {
    let response = engine
        .handle(request(
            "POST",
            "/v1/chat/completions",
            Some(chat("gpt-test", false)),
        ))
        .await;
    response.status.as_u16()
}

/// How often the token endpoint was asked to renew.
fn renewals(upstream: &Upstream) -> usize {
    upstream
        .seen()
        .iter()
        .filter(|seen| seen.path == "/oauth/token" && seen.body["grant_type"] == "refresh_token")
        .count()
}

fn bearers(upstream: &Upstream) -> Vec<String> {
    upstream
        .seen()
        .iter()
        .filter(|seen| seen.path == "/v1/chat/completions")
        .map(|seen| seen.header("authorization").unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn an_expired_token_is_renewed_before_it_is_used_and_kept() {
    let upstream = Upstream::start(oauth_api()).await;
    let state = memory();
    let engine = signed_in(&upstream, state.clone()).await;
    for _ in 0..2 {
        let response = engine
            .handle(request(
                "POST",
                "/v1/chat/completions",
                Some(chat("gpt-test", false)),
            ))
            .await;
        assert_eq!(response.status, 200);
    }
    // `access-1` came expired from the sign-in and was never sent; one
    // renewal served both requests.
    assert_eq!(bearers(&upstream), ["Bearer access-2", "Bearer access-2"]);

    // Another instance over the same store starts from the renewed token.
    let other = support::engine(oauth_provider(&upstream), state).await;
    let response = other
        .handle(request(
            "POST",
            "/v1/chat/completions",
            Some(chat("gpt-test", false)),
        ))
        .await;
    assert_eq!(response.status, 200);
    assert_eq!(bearers(&upstream).last().unwrap(), "Bearer access-2");
}

/// A store in which a sign-in on another instance lands just before the
/// first write to the credential.
struct Raced {
    store: Arc<MemoryState>,
    rival: Mutex<Option<Engine>>,
}

impl KeyValue for Raced {
    fn get<'a>(
        &'a self,
        key: &'a str,
    ) -> CapabilityFuture<'a, Result<Option<StateEntry>, CapabilityError>> {
        self.store.get(key)
    }

    fn compare_exchange<'a>(
        &'a self,
        key: &'a str,
        expected: Option<Version>,
        replacement: Option<StateWrite>,
    ) -> CapabilityFuture<'a, Result<CasResult, CapabilityError>> {
        Box::pin(async move {
            let rival = key
                .starts_with("credential/")
                .then(|| self.rival.lock().unwrap().take())
                .flatten();
            if let Some(rival) = rival {
                sign_in(&rival, "acct").await;
            }
            self.store
                .compare_exchange(key, expected, replacement)
                .await
        })
    }

    fn limits(&self) -> CapabilityLimits {
        self.store.limits()
    }
}

#[tokio::test]
async fn a_renewal_that_loses_the_exchange_uses_what_was_stored() {
    let upstream = Upstream::start(oauth_api()).await;
    let store = Arc::new(MemoryState::default());
    let rival = signed_in(&upstream, store.clone()).await;
    let raced = Arc::new(Raced {
        store,
        rival: Mutex::new(Some(rival)),
    });
    let loser = engine(oauth_provider(&upstream), raced).await;
    assert_eq!(chat_status(&loser).await, 200);
    // The loser renewed `access-1` and was issued `access-2`; the second
    // sign-in stored `access-3` first, and that is what was sent.
    assert_eq!(bearers(&upstream), ["Bearer access-3"]);
}

#[tokio::test]
async fn the_schedule_renews_what_is_due_and_one_failure_does_not_stop_the_rest() {
    let good = Upstream::start(oauth_api()).await;
    // Signs in, then refuses every renewal for good.
    let revoked = Upstream::start(|seen: &Seen| match seen.body["grant_type"].as_str() {
        Some("authorization_code") => {
            Answer::Json(200, json!({"access_token": "access-1", "expires_at_ms": 1}))
        }
        _ => Answer::Json(400, json!({"error": "invalid_grant"})),
    })
    .await;
    let config = json!({
        "access_key": ACCESS_KEY,
        "providers": [
            {"name": "revoked", "channel": "fake_oauth", "base_url": revoked.url},
            {"name": "acct", "channel": "fake_oauth", "base_url": good.url},
            {"name": "unsigned", "channel": "fake_oauth", "base_url": good.url},
            {"name": "key", "channel": "custom", "base_url": good.url,
             "config": {"dialects": ["openai_chat"]}, "credential": "an-api-key"}
        ]
    });
    let engine = engine(config, memory()).await;
    sign_in(&engine, "revoked").await;
    sign_in(&engine, "acct").await;

    let mut unauthorized = request("POST", "/lite/renew", None);
    unauthorized.headers_mut().remove("authorization");
    assert_eq!(engine.handle(unauthorized).await.status, 401);
    assert_eq!(renewals(&good) + renewals(&revoked), 0);

    let renew = || async {
        let (status, body) =
            read_json(engine.handle(request("POST", "/lite/renew", None)).await).await;
        assert_eq!(status, 200, "{body}");
        body["renewals"].clone()
    };
    let first = renew().await;
    assert_eq!(first[0]["provider"], "revoked");
    assert_eq!(first[0]["outcome"], "failed");
    assert!(
        first[0]["reason"]
            .as_str()
            .unwrap()
            .contains("invalid_grant"),
        "{first}"
    );
    // An API key is not listed: there is nothing to renew.
    assert_eq!(
        first.as_array().unwrap()[1..],
        [
            json!({"provider": "acct", "outcome": "renewed"}),
            json!({"provider": "unsigned", "outcome": "no_credential"}),
        ]
    );

    // The renewed token is fresh; the refused one is not presented again.
    let second = renew().await;
    assert_eq!(second[0]["outcome"], "failed");
    assert_eq!(second[1]["outcome"], "not_due");
    assert_eq!((renewals(&good), renewals(&revoked)), (1, 1));
    let (_, status) = read_json(engine.handle(request("GET", "/lite/status", None)).await).await;
    assert_eq!(status["providers"][0]["credential"], "dead");

    // A request finds the token the schedule stored.
    let response = engine
        .handle(request(
            "POST",
            "/acct/v1/chat/completions",
            Some(chat("gpt-test", false)),
        ))
        .await;
    assert_eq!(response.status, 200);
    assert_eq!(bearers(&good), ["Bearer access-2"]);
    assert_eq!(renewals(&good), 1);
}

#[tokio::test]
async fn two_renewals_at_once_spend_the_refresh_token_once() {
    let upstream = Upstream::start(oauth_api()).await;
    let state = memory();
    let one = signed_in(&upstream, state.clone()).await;
    let other = engine(oauth_provider(&upstream), state).await;
    // Both read the expired token before either has renewed it.
    let (first, second) = tokio::join!(chat_status(&one), chat_status(&other));
    assert_eq!((first, second), (200, 200));
    assert_eq!(renewals(&upstream), 1);
    assert_eq!(bearers(&upstream), ["Bearer access-2", "Bearer access-2"]);
}

#[tokio::test]
async fn a_lease_nobody_released_expires() {
    let upstream = Upstream::start(oauth_api()).await;
    let state = memory();
    let engine = signed_in(&upstream, state.clone()).await;
    // What a renewal that never finished leaves behind: the lease, in the
    // engine's stored form (its deadline, then the value), with 600 ms left.
    let deadline = now_ms() + 600;
    state
        .compare_exchange(
            "lease/credential/acct",
            None,
            Some(StateWrite {
                payload: deadline.to_be_bytes().to_vec().into(),
                expires_at: None,
            }),
        )
        .await
        .unwrap();
    let started = std::time::Instant::now();
    assert_eq!(chat_status(&engine).await, 200);
    // It waited for the lease instead of renewing beside it, then took it.
    assert!(started.elapsed() >= std::time::Duration::from_millis(500));
    assert_eq!(renewals(&upstream), 1);
    assert_eq!(bearers(&upstream), ["Bearer access-2"]);
}

fn unauthorized() -> Answer {
    Answer::Json(401, json!({"error": {"message": "token revoked"}}))
}

#[tokio::test]
async fn a_401_renews_once_and_sends_once_more() {
    let in_an_hour = now_ms() + 3_600_000;
    // The upstream no longer accepts the token it issued at sign-in.
    let upstream = Upstream::start(oauth_with(in_an_hour, |seen| {
        match seen.header("authorization") {
            Some("Bearer access-1") => unauthorized(),
            _ => model_api(seen),
        }
    }))
    .await;
    let engine = signed_in(&upstream, memory()).await;
    assert_eq!(chat_status(&engine).await, 200);
    assert_eq!(bearers(&upstream), ["Bearer access-1", "Bearer access-2"]);
    assert_eq!(renewals(&upstream), 1);

    // An upstream that refuses the renewed token too is the caller's answer.
    let upstream = Upstream::start(oauth_with(in_an_hour, |_| unauthorized())).await;
    let engine = signed_in(&upstream, memory()).await;
    assert_eq!(chat_status(&engine).await, 401);
    assert_eq!(bearers(&upstream), ["Bearer access-1", "Bearer access-2"]);
    assert_eq!(renewals(&upstream), 1);
}
