use futures_util::StreamExt;
use gproxy_channel_api::{Binding, Channel};
use http::Method;

use super::memory::MemoryHost;
use super::surface_harness::{execute, outcome, plan, target};

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

    let state = host.state.lock().expect("state lock");
    assert_eq!(state.cache.len(), 3);
    assert!(state.cache.keys().all(|key| {
        key.starts_with("gproxy:surface:3:") && !key.contains("session") && !key.contains("server")
    }));
    assert!(state.cache_ttls.values().all(|ttl| *ttl == 60));
    assert_eq!(state.admission_finishes.len(), state.admit_calls);
    assert_eq!(state.admission_finishes.last(), Some(&true));
    assert_eq!(state.authorizations.len(), 2);
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

fn require_send(_: impl Send) {}
