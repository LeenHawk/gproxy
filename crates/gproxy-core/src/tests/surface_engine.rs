use futures_util::StreamExt;
use gproxy_channel_api::{Binding, Channel};
use http::Method;

use super::memory::MemoryHost;
use super::surface_harness::{execute, outcome, plan, target};

mod forward_retry;

#[test]
fn surfaces_scope_affinity_assemble_services_and_require_bindings() {
    let host = MemoryHost::new(false);
    {
        let mut state = host.state.lock().expect("state lock");
        state.bindings.insert(
            (3, 1, "task".into(), "bound".into()),
            Binding {
                provider_id: 3,
                owner_user_id: 1,
                kind: "task".into(),
                id: "bound".into(),
                credential: crate::CredentialId(7),
                summary: serde_json::json!({}),
                created_at_unix: 0,
            },
        );
        state.plan = Some(plan(vec![target(4, 8), target(3, 7)]));
    }
    let core = super::core(&host).expect("core");
    require_send(core.execute(&host, super::request(false, "send-check")));
    let body = execute(
        &core,
        &host,
        Method::GET,
        "/surface/tasks/bound",
        None,
        None,
    )
    .expect("bound surface");
    assert_eq!(body["credential"], 7);
    assert_eq!(body["provider"], 3);

    let forward = outcome(
        &core,
        &host,
        Method::GET,
        "/surface/forward/bound",
        None,
        None,
        false,
    )
    .expect("forward surface");
    let crate::ResponseBody::Stream(mut stream) = forward.body else {
        panic!("forward response was not streamed");
    };
    super::block_on(async {
        assert!(stream.next().await.expect("forward body").is_ok());
        assert!(stream.next().await.is_none());
    });
    assert!(matches!(
        outcome(
            &core,
            &host,
            Method::GET,
            "/surface/socket/bound",
            None,
            None,
            false,
        ),
        Err(crate::CoreError::Unsupported)
    ));
    let (finishes_before_socket, captures_before_socket) = {
        let state = host.state.lock().expect("state lock");
        (state.admission_finishes.len(), state.captures.len())
    };
    let socket = outcome(
        &core,
        &host,
        Method::GET,
        "/surface/socket/bound",
        None,
        None,
        true,
    )
    .expect("websocket surface");
    let crate::ResponseBody::WebSocket(mut socket) = socket.body else {
        panic!("websocket action returned an HTTP body");
    };
    let frame = super::block_on(socket.recv())
        .expect("socket receive")
        .expect("close frame");
    assert!(matches!(frame, crate::WsFrame::Close(Some(1000))));
    {
        let state = host.state.lock().expect("state lock");
        assert_eq!(state.admission_finishes.len(), finishes_before_socket + 1);
        assert_eq!(state.captures.len(), captures_before_socket + 1);
    }

    host.state.lock().expect("state lock").plan = Some(plan(vec![target(3, 7), target(3, 8)]));
    let first = execute(&core, &host, Method::GET, "/surface/header", None, None)
        .expect("first header pin");
    assert_eq!(first["slot"], 7);
    host.state.lock().expect("state lock").plan = Some(plan(vec![target(3, 8), target(3, 7)]));
    let pinned = execute(
        &core,
        &host,
        Method::GET,
        "/surface/header",
        Some(("x-session", "generated")),
        None,
    )
    .expect("cached header pin");
    assert_eq!(pinned["slot"], 7);

    {
        let mut state = host.state.lock().expect("state lock");
        state.caller_user_id = 9;
        state.caller_key_id = 10;
    }
    let other_user = execute(
        &core,
        &host,
        Method::GET,
        "/surface/header",
        Some(("x-session", "generated")),
        None,
    )
    .expect("caller-scoped header pin");
    assert_eq!(other_user["slot"], 8);
    execute(
        &core,
        &host,
        Method::POST,
        "/surface/body",
        None,
        Some(br#"{"server_id":"server"}"#),
    )
    .expect("body pin");

    let denied = execute(
        &core,
        &host,
        Method::GET,
        "/surface/tasks/bound",
        None,
        None,
    );
    assert!(matches!(denied, Err(crate::CoreError::UnknownRoute(_))));

    host.state.lock().expect("state lock").plan = Some(plan(vec![target(3, 7)]));
    execute(&core, &host, Method::GET, "/surface/invoke", None, None)
        .expect("funneled surface invoke");
    let alias = execute(
        &core,
        &host,
        Method::POST,
        "/surface/alias",
        None,
        Some(br#"{"model":"alias","stream":false}"#),
    )
    .expect("operation alias");
    assert_eq!(alias["result"], "ok");

    let state = host.state.lock().expect("state lock");
    assert_eq!(
        state
            .cache
            .keys()
            .filter(|key| key.starts_with("gproxy:surface:3:"))
            .count(),
        3
    );
    assert_eq!(
        state
            .cache
            .keys()
            .filter(|key| key.starts_with("gproxy:session-affinity:v1:"))
            .count(),
        1
    );
    assert!(state.cache_ttls.iter().all(|(key, ttl)| {
        if key.starts_with("gproxy:session-affinity:v1:") {
            *ttl == 3_600
        } else {
            *ttl == 60
        }
    }));
    assert_eq!(state.admission_finishes.len(), state.admit_calls);
    assert_eq!(state.admission_finishes.last(), Some(&true));
    assert_eq!(state.authorizations.len(), 3);
    assert_eq!(state.resolved_models.last(), Some(&Some("alias".into())));
    assert_eq!(state.socket_opens, 1);
    assert!(state.socket_closed);
    drop(state);

    let host = MemoryHost::without_bindings();
    let channels =
        gproxy_channel_api::ChannelRegistry::new([Box::new(host.clone()) as Box<dyn Channel>])
            .expect("channel registry");
    assert!(matches!(
        crate::Core::new(host, channels),
        Err(crate::InitError::SurfacesWithoutBindings { channel: "memory" })
    ));
}

#[test]
fn response_token_authenticates_and_pins_surface_websocket() {
    let host = MemoryHost::new(false);
    host.state.lock().expect("state lock").plan = Some(plan(vec![target(3, 7), target(3, 8)]));
    let core = super::core(&host).expect("core");
    let created = execute(
        &core,
        &host,
        Method::POST,
        "/surface/token",
        None,
        Some(b"{}"),
    )
    .expect("remote token");
    assert_eq!(created["remote_token"], "remote-secret");
    host.state.lock().expect("state lock").plan = Some(plan(vec![target(3, 8), target(3, 7)]));
    let refreshed = execute(
        &core,
        &host,
        Method::POST,
        "/surface/token/refresh",
        None,
        Some(br#"{"server_id":"remote-server"}"#),
    )
    .expect("remote token refresh pin");
    assert_eq!(refreshed["slot"], 7);
    let environment = execute(
        &core,
        &host,
        Method::GET,
        "/surface/environment/remote-environment",
        None,
        None,
    )
    .expect("remote environment pin");
    assert_eq!(environment["slot"], 7);
    let server = execute(
        &core,
        &host,
        Method::POST,
        "/surface/body",
        Some(("x-server-id", "remote-server")),
        None,
    )
    .expect("remote server pin");
    assert_eq!(server["slot"], 7);
    {
        let mut state = host.state.lock().expect("state lock");
        assert_eq!(state.auth_calls, 4);
        state.caller_user_id = 99;
        state.caller_key_id = 100;
        state.socket_closed = false;
    }

    let socket = outcome(
        &core,
        &host,
        Method::GET,
        "/surface/token/socket",
        Some(("authorization", "Bearer remote-secret")),
        None,
        true,
    )
    .expect("token websocket");
    let crate::ResponseBody::WebSocket(mut socket) = socket.body else {
        panic!("token route did not return a websocket");
    };
    let frame = super::block_on(socket.recv())
        .expect("socket receive")
        .expect("close frame");
    assert!(matches!(frame, crate::WsFrame::Close(Some(1000))));
    let state = host.state.lock().expect("state lock");
    assert_eq!(state.auth_calls, 4);
    assert_eq!(state.admit_calls, 5);
    assert_eq!(state.socket_opens, 1);
}

fn require_send(_: impl Send) {}
