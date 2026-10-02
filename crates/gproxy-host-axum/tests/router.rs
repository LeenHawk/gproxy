#![cfg(not(target_arch = "wasm32"))]
//! The router, driven end to end with `tower::ServiceExt::oneshot`.
//!
//! Every test here builds the **real** router over a real in-memory instance
//! and sends a real `http::Request` through it. Nothing is stubbed: the
//! authenticator, admission, the sdk's resolver and the settlement path all
//! run, and only the upstream is scripted. That is the point — this crate's
//! whole job is the binding, and a test that called the handler function
//! directly would skip the part being tested.

mod support;

use gproxy_app::AppConfig;
use http::{Method, StatusCode};
use serde_json::json;
use support::{Host, Reply, get, keyed, post, with};

/// One provider, one permitted key. The most ordinary instance there is.
async fn instance() -> Host {
    let host = Host::new().await;
    let handle = host.handle();
    support::person(&handle, "alice", "user").await;
    support::api_key(&handle, "k-alice", "alice", None, None).await;
    support::provider(&handle, "p1", &["m1"]).await;
    support::allow(&handle, "perm-alice", "alice", None).await;
    support::credential(&handle, "c1", "p1", None, None, None).await;
    host.publish().await;
    host
}

#[tokio::test]
async fn healthz_answers_without_a_credential_and_names_the_revision() {
    let host = instance().await;
    let answer = host.send(get("/healthz")).await;
    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(answer.json()["status"], "ok");
    assert!(
        answer.json()["revision"].as_i64().unwrap() > 0,
        "the published revision, not a placeholder: {}",
        answer.text()
    );
}

#[tokio::test]
async fn a_data_plane_request_with_no_key_is_401_in_the_product_envelope() {
    let host = instance().await;
    let answer = host
        .send(post("/v1/messages", json!({"model": "test/m1"})))
        .await;
    assert_eq!(answer.status, StatusCode::UNAUTHORIZED);
    assert_eq!(answer.json()["error"]["code"], "unauthorized");
    // The reason is for the operator's log, never for an unauthenticated
    // caller: it is exactly what a prober is asking for.
    assert_eq!(answer.json()["error"]["message"], "unauthorized");
    assert!(
        host.client.urls().is_empty(),
        "nothing may reach an upstream before the caller is known"
    );
}

#[tokio::test]
async fn query_credentials_authenticate_but_never_reach_the_upstream() {
    let host = instance().await;
    host.client
        .script(vec![Reply::Http(StatusCode::OK, json!({"ok": true}))]);
    let answer = host
        .send(post(
            "/v1/messages?keep=a%2Fb&%6bey=k-alice&key=k-alice&api%5fkey=private&keep=two+words",
            json!({"model": "test/m1"}),
        ))
        .await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.text());
    assert_eq!(
        host.client.urls(),
        ["https://p1.example/v1/messages?keep=a%2Fb&keep=two+words"]
    );
    let answer = host
        .send(get(
            "/p1/backend-api/wham/usage?%6bey=k-alice&access_token=private&keep=a%2Fb",
        ))
        .await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.text());
    assert_eq!(answer.json()["query"], "keep=a%2Fb");
}

#[tokio::test]
async fn a_caller_with_no_permission_is_403_and_costs_no_upstream_call() {
    let host = Host::new().await;
    let handle = host.handle();
    support::person(&handle, "bob", "user").await;
    support::api_key(&handle, "k-bob", "bob", None, None).await;
    support::provider(&handle, "p1", &["m1"]).await;
    support::credential(&handle, "c1", "p1", None, None, None).await;
    // No permission row at all: the default is deny.
    host.publish().await;

    let answer = host
        .send(keyed(
            post("/v1/messages", json!({"model": "test/m1"})),
            "k-bob",
        ))
        .await;
    assert_eq!(answer.status, StatusCode::FORBIDDEN);
    assert_eq!(answer.json()["error"]["code"], "forbidden");
    assert!(host.client.urls().is_empty());
}

#[tokio::test]
async fn a_permitted_call_reaches_the_upstream_and_writes_a_usage_row() {
    use gproxy_store::entity::usage::usage_record;
    use sea_orm::EntityTrait;

    let host = instance().await;
    host.client
        .script(vec![Reply::Http(StatusCode::OK, json!({"ok": true}))]);

    let answer = host
        .send(keyed(
            post("/v1/messages", json!({"model": "test/m1"})),
            "k-alice",
        ))
        .await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.text());
    assert_eq!(answer.json(), json!({"ok": true}));
    assert_eq!(host.client.urls(), ["https://p1.example/v1/messages"]);

    // The settlement runs as the response body ends, which `send` has already
    // drained; the usage row is therefore durable by now.
    let rows: Vec<usage_record::Model> = host
        .app
        .gproxy()
        .store()
        .usage_records()
        .query(usage_record::Entity::find())
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "one physical call, one usage row");
    assert_eq!(rows[0].user_id.as_deref(), Some("alice"));
    assert_eq!(rows[0].api_key_id.as_deref(), Some("k-alice"));
    assert_eq!(rows[0].model, "m1");
}

#[tokio::test]
async fn the_upstream_is_told_the_path_of_its_own_api_not_the_mount() {
    let host = instance().await;
    host.client
        .script(vec![Reply::Http(StatusCode::OK, json!({"ok": true}))]);
    let answer = host
        .send(keyed(
            post("/p1/v1/messages", json!({"model": "m1"})),
            "k-alice",
        ))
        .await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.text());
    assert_eq!(
        host.client.urls(),
        ["https://p1.example/v1/messages"],
        "the provider prefix is this gateway's, not the upstream's"
    );
}

// ------------------------------------------------------------- the mount --

#[tokio::test]
async fn a_provider_mount_narrows_to_that_provider() {
    let host = Host::new().await;
    let handle = host.handle();
    support::person(&handle, "alice", "user").await;
    support::api_key(&handle, "k-alice", "alice", None, None).await;
    support::provider(&handle, "p1", &["m1"]).await;
    support::provider(&handle, "p2", &["m1"]).await;
    support::allow(&handle, "perm", "alice", None).await;
    support::credential(&handle, "c1", "p1", None, None, None).await;
    support::credential(&handle, "c2", "p2", None, None, None).await;
    host.publish().await;

    host.client
        .script(vec![Reply::Http(StatusCode::OK, json!({"from": "p2"}))]);
    // The mount prefixes the model, which is the sdk's `provider/model` form.
    let answer = host
        .send(keyed(
            post("/p2/v1/messages", json!({"model": "m1"})),
            "k-alice",
        ))
        .await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.text());
    assert_eq!(
        host.client.urls(),
        ["https://p2.example/v1/messages"],
        "the mount chose the provider, not the balancer"
    );
}

#[tokio::test]
async fn a_namespace_mount_resolves_the_exposed_name() {
    let host = Host::new().await;
    let handle = host.handle();
    support::person(&handle, "alice", "user").await;
    support::api_key(&handle, "k-alice", "alice", None, None).await;
    support::provider(&handle, "p1", &["m1"]).await;
    support::allow(&handle, "perm", "alice", None).await;
    support::credential(&handle, "c1", "p1", None, None, None).await;
    // `acme/fast` is an exposed name, so `acme` becomes a namespace mount.
    support::exposed(&handle, "acme/fast", "p1", "m1").await;
    host.publish().await;

    host.client
        .script(vec![Reply::Http(StatusCode::OK, json!({"ok": true}))]);
    let answer = host
        .send(keyed(
            post("/acme/v1/messages", json!({"model": "fast"})),
            "k-alice",
        ))
        .await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.text());
    assert_eq!(host.client.urls(), ["https://p1.example/v1/messages"]);

    // And the same name spelled in full on the aggregated mount.
    host.client
        .script(vec![Reply::Http(StatusCode::OK, json!({"ok": true}))]);
    let answer = host
        .send(keyed(
            post("/v1/messages", json!({"model": "acme/fast"})),
            "k-alice",
        ))
        .await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.text());
}

#[tokio::test]
async fn a_first_segment_that_is_a_provider_is_not_stripped_from_a_foreign_path() {
    // The rule the mount grammar exists for: a provider named `backend-api`
    // must not eat `/backend-api/codex/responses`, which is a real upstream
    // path and not a mount at all.
    let host = Host::new().await;
    let handle = host.handle();
    support::person(&handle, "alice", "user").await;
    support::api_key(&handle, "k-alice", "alice", None, None).await;
    support::provider(&handle, "backend-api", &["m1"]).await;
    support::allow(&handle, "perm", "alice", None).await;
    support::credential(&handle, "c1", "backend-api", None, None, None).await;
    host.publish().await;

    let answer = host
        .send(keyed(
            post("/backend-api/codex/responses", json!({"model": "m1"})),
            "k-alice",
        ))
        .await;
    // Not stripped, so it is not a declared surface either: a 404 the operator
    // can see, rather than a request sent to the wrong upstream.
    assert_eq!(answer.status, StatusCode::NOT_FOUND, "{}", answer.text());
    assert_eq!(answer.json()["error"]["code"], "not_found");
    assert!(host.client.urls().is_empty());
}

#[tokio::test]
async fn an_unknown_first_segment_stays_on_the_aggregated_mount() {
    let host = instance().await;
    host.client
        .script(vec![Reply::Http(StatusCode::OK, json!({"ok": true}))]);
    // `nope` is neither a namespace nor a provider, so the path is left whole
    // — and `/nope/v1/messages` is not a declared surface.
    let answer = host
        .send(keyed(
            post("/nope/v1/messages", json!({"model": "test/m1"})),
            "k-alice",
        ))
        .await;
    assert_eq!(answer.status, StatusCode::NOT_FOUND, "{}", answer.text());
    assert!(host.client.urls().is_empty());
}

// ------------------------------------------------------- vendor services --

#[tokio::test]
async fn a_channels_service_route_is_served_on_a_provider_mount() {
    let host = instance().await;
    // The channel declares `GET /backend-api/wham/usage`. It is nothing like
    // the model API's paths, so the mount grammar has to recognise it as a
    // surface before it strips the provider prefix in front of it.
    let answer = host
        .send(keyed(get("/p1/backend-api/wham/usage"), "k-alice"))
        .await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.text());
    assert_eq!(answer.json()["view"], "caller");
    assert_eq!(
        answer.json()["path"],
        "/backend-api/wham/usage",
        "the channel sees the vendor path, not the mounted one"
    );
    assert!(
        host.client.urls().is_empty(),
        "this service answers locally; nothing was forwarded"
    );
}

#[tokio::test]
async fn a_service_route_needs_a_mount_that_names_a_provider() {
    let host = instance().await;
    // The aggregated mount names no channel, so there is nothing to match the
    // route against and the path is nobody's.
    let answer = host
        .send(keyed(get("/backend-api/wham/usage"), "k-alice"))
        .await;
    assert_eq!(answer.status, StatusCode::NOT_FOUND, "{}", answer.text());
}

#[tokio::test]
async fn a_view_an_ordinary_member_may_not_ask_for_is_refused() {
    let host = instance().await;
    let answer = host
        .send(with(
            keyed(get("/p1/backend-api/wham/usage"), "k-alice"),
            "x-gproxy-view",
            "pool",
        ))
        .await;
    assert_eq!(answer.status, StatusCode::FORBIDDEN, "{}", answer.text());

    // And a view name this host does not know is a 400 from the binding.
    let answer = host
        .send(with(
            keyed(get("/p1/backend-api/wham/usage"), "k-alice"),
            "x-gproxy-view",
            "everything",
        ))
        .await;
    assert_eq!(answer.status, StatusCode::BAD_REQUEST, "{}", answer.text());
    assert_eq!(answer.json()["error"]["code"], "invalid_request");
}

#[tokio::test]
async fn a_service_route_still_needs_a_credential() {
    let host = instance().await;
    let answer = host.send(get("/p1/backend-api/wham/usage")).await;
    assert_eq!(answer.status, StatusCode::UNAUTHORIZED);
}

// ------------------------------------------------------------- publication --
#[tokio::test]
async fn an_unknown_publication_is_404() {
    let host = instance().await;
    let answer = host.send(get("/publications/does-not-exist")).await;
    assert_eq!(answer.status, StatusCode::NOT_FOUND);
    assert_eq!(answer.json()["error"]["code"], "not_found");
    // Deliberately unauthenticated: the id is the capability. A 401 here would
    // break the only thing publication exists for.
    let answer = host
        .send(keyed(get("/publications/does-not-exist"), "k-alice"))
        .await;
    assert_eq!(answer.status, StatusCode::NOT_FOUND);
}

// --------------------------------------------------------------- console --

#[tokio::test]
async fn a_build_with_no_console_answers_404_rather_than_a_blank_page() {
    // Whether a bundle exists is a property of the build, not of the source:
    // in a debug build `rust_embed` reads `assets/web` from disk, so this is
    // served for anyone who has run the console's build and absent for
    // everyone else. Asserting one of those would make `cargo test` fail for
    // whoever built the console last, over a change that had nothing to do
    // with it. So ask the crate what this build has, and assert the rule.
    let embedded =
        gproxy_host_axum::console::Console::from_config(&gproxy_app::config::ConsoleConfig {
            enabled: true,
            path: None,
        })
        .is_enabled();

    let host = instance().await;
    let answer = host.send(get("/console")).await;
    if embedded {
        assert_eq!(answer.status, StatusCode::OK);
        assert!(answer.text().contains("<html"), "{}", answer.text());
    } else {
        assert_eq!(answer.status, StatusCode::NOT_FOUND);
        assert!(answer.text().contains("console"), "{}", answer.text());
    }
}

#[tokio::test]
async fn the_console_switch_is_honoured() {
    let host = Host::with_config(AppConfig {
        console: gproxy_app::config::ConsoleConfig {
            enabled: false,
            path: None,
        },
        ..AppConfig::default()
    })
    .await;
    host.publish().await;
    let answer = host.send(get("/console/keys")).await;
    assert_eq!(answer.status, StatusCode::NOT_FOUND);
}

// ------------------------------------------------------------------ cors --

#[tokio::test]
async fn a_preflight_from_a_configured_origin_is_answered_here() {
    let host = Host::with_config(AppConfig {
        cors_origins: vec!["https://console.example.com".into()],
        ..AppConfig::default()
    })
    .await;
    host.publish().await;

    let request = with(
        with(
            support::request(Method::OPTIONS, "/v1/messages"),
            "origin",
            "https://console.example.com",
        ),
        "access-control-request-method",
        "POST",
    );
    let answer = host.send(request).await;
    assert_eq!(answer.status, StatusCode::NO_CONTENT);
    assert_eq!(
        answer.header("access-control-allow-origin"),
        Some("https://console.example.com")
    );
    assert!(
        host.client.urls().is_empty(),
        "a preflight asks this instance what it accepts; it is never forwarded"
    );
}

#[tokio::test]
async fn a_foreign_origin_gets_no_cors_headers() {
    let host = Host::with_config(AppConfig {
        cors_origins: vec!["https://console.example.com".into()],
        ..AppConfig::default()
    })
    .await;
    host.publish().await;

    let request = with(
        with(
            support::request(Method::OPTIONS, "/v1/messages"),
            "origin",
            "https://evil.example",
        ),
        "access-control-request-method",
        "POST",
    );
    let answer = host.send(request).await;
    assert!(answer.header("access-control-allow-origin").is_none());
}

// ---------------------------------------------------------- body decoding --

#[tokio::test]
async fn a_zstd_body_is_decoded_before_the_model_is_read() {
    use ruzstd::encoding::{CompressionLevel, compress_to_vec};

    let host = instance().await;
    host.client
        .script(vec![Reply::Http(StatusCode::OK, json!({"ok": true}))]);

    let body = serde_json::to_vec(&json!({"model": "test/m1"})).unwrap();
    let compressed = compress_to_vec(body.as_slice(), CompressionLevel::Fastest);
    let request = http::Request::builder()
        .method(Method::POST)
        .uri("/v1/messages")
        .header("authorization", "Bearer k-alice")
        .header("content-type", "application/json")
        .header("content-encoding", "zstd")
        .body(axum::body::Body::from(compressed))
        .unwrap();

    let answer = host.send(request).await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.text());
    assert_eq!(host.client.urls(), ["https://p1.example/v1/messages"]);
}

#[tokio::test]
async fn an_encoding_this_host_does_not_decode_is_refused() {
    let host = instance().await;
    let request = http::Request::builder()
        .method(Method::POST)
        .uri("/v1/messages")
        .header("authorization", "Bearer k-alice")
        .header("content-encoding", "br")
        .body(axum::body::Body::from("{}"))
        .unwrap();
    let answer = host.send(request).await;
    assert_eq!(answer.status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
}

// ------------------------------------------------------------- body caps --

#[tokio::test]
async fn a_body_is_held_to_its_cap_and_only_an_upload_gets_the_larger_one() {
    let host = instance().await;
    host.handle()
        .manage()
        .settings()
        .update(gproxy_sdk::dto::SettingsPatch {
            instance: Some(gproxy_sdk::dto::InstanceSettingsPatch {
                max_request_body_bytes: Some(1024),
                max_upload_body_bytes: Some(4096),
                ..Default::default()
            }),
            logging: None,
        })
        .await
        .unwrap();
    host.publish().await;
    let padding = "x".repeat(2048);

    let chat = post(
        "/v1/chat/completions",
        json!({ "model": "m1", "messages": [{ "role": "user", "content": padding }] }),
    );
    let answer = host.send(keyed(chat, "k-alice")).await;
    assert_eq!(answer.status, StatusCode::PAYLOAD_TOO_LARGE);
    // Refused before authentication: the cap is what bounds an anonymous body.
    let chat = post(
        "/v1/chat/completions",
        json!({ "model": "m1", "messages": [{ "role": "user", "content": padding }] }),
    );
    assert_eq!(host.send(chat).await.status, StatusCode::PAYLOAD_TOO_LARGE);

    // A file upload the same size is inside its own cap and goes on to be
    // answered by whatever else it meets.
    host.client
        .script(vec![Reply::Http(StatusCode::OK, json!({ "id": "file-1" }))]);
    let upload = keyed(post("/v1/files", json!({ "file": padding })), "k-alice");
    assert_ne!(
        host.send(upload).await.status,
        StatusCode::PAYLOAD_TOO_LARGE
    );
    let upload = keyed(
        post("/v1/files", json!({ "file": "x".repeat(8192) })),
        "k-alice",
    );
    assert_eq!(
        host.send(upload).await.status,
        StatusCode::PAYLOAD_TOO_LARGE
    );

    // Sign-in is read before there is anyone to answer for it.
    let login = post(
        "/portal/api/login",
        json!({ "name": "alice", "password": "x".repeat(128 * 1024) }),
    );
    assert_eq!(host.send(login).await.status, StatusCode::PAYLOAD_TOO_LARGE);
}

// ------------------------------------------------------------ settlement --

/// Over a real connection, as a client sees it. Hyper stops polling a body the
/// moment it has written the bytes `content-length` declared, so a response
/// that waited for its end-of-stream to settle was never settled as finished:
/// the body was dropped, and the drop is what a client hanging up looks like.
#[tokio::test(flavor = "multi_thread")]
async fn a_buffered_answer_over_a_socket_settles_as_completed() {
    use gproxy_store::entity::usage::usage_record;
    use sea_orm::EntityTrait;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let host = instance().await;
    host.client
        .script(vec![Reply::Http(StatusCode::OK, json!({ "ok": true }))]);
    let bound = host.bind().await;
    let mut socket = tokio::net::TcpStream::connect(bound.address).await.unwrap();
    let body = json!({ "model": "test/m1" }).to_string();
    socket
        .write_all(
            format!(
                "POST /v1/messages HTTP/1.1\r\nhost: gproxy.local\r\n\
                 authorization: Bearer k-alice\r\ncontent-type: application/json\r\n\
                 content-length: {}\r\n\r\n{body}",
                body.len()
            )
            .as_bytes(),
        )
        .await
        .unwrap();
    // Read exactly one response and keep the connection open, as a
    // keep-alive client does.
    let mut received = Vec::new();
    let mut buffer = [0; 4096];
    loop {
        let read = socket.read(&mut buffer).await.unwrap();
        received.extend_from_slice(&buffer[..read]);
        let text = String::from_utf8_lossy(&received);
        if let Some((head, rest)) = text.split_once("\r\n\r\n") {
            let length: usize = head
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .map(|v| v.trim().parse().unwrap())
                })
                .expect("a buffered answer declares its length");
            if rest.len() >= length {
                break;
            }
        }
    }
    let text = String::from_utf8_lossy(&received).into_owned();
    assert!(text.starts_with("HTTP/1.1 200"), "{text}");

    let mut settled = Vec::new();
    for _ in 0..200 {
        settled = host
            .app
            .gproxy()
            .store()
            .usage_records()
            .query(usage_record::Entity::find())
            .await
            .unwrap();
        if !settled.is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(settled.len(), 1, "the request is recorded");
    let state = settled[0].state.as_deref().unwrap_or_default().to_owned();
    assert_eq!(
        state, "completed",
        "an answer the client received in full is not a cancelled one"
    );
    drop(socket);
}
