use crate::control::FailoverBudget;
use bytes::Bytes;
use gproxy_channel_api::{Channel, ChannelRegistry, CredentialId, WsFrame};
use http::{HeaderMap, Method};
use rust_decimal::Decimal;
use serde_json::json;

use super::super::{block_on, memory::MemoryHost, target};
use crate::host::Host;
use crate::{Core, CoreError, Plan, RequestCtx, ResponseBody, RoutingMode};

fn setup() -> (MemoryHost, Core<MemoryHost>) {
    let host = MemoryHost::with_session_spawner();
    let mut selected = target();
    selected.provider.channel = "openai".into();
    {
        let mut state = host.state.lock().unwrap();
        state.credential.channel = "openai".into();
        state.credential.secret = json!({"api_key":"test-key"});
        state.plan = Some(Plan {
            targets: vec![selected],
            budget: FailoverBudget { max_attempts: 1 },
        });
        state.socket_frames = [
            WsFrame::Text(r#"{"type":"session.created","session":{"type":"realtime","model":"actual","audio":{"input":{"transcription":{"model":"transcription-model"}}}}}"#.into()),
            WsFrame::Text(r#"{"type":"response.done","response":{"id":"r1","usage":{"input_tokens":1000000,"output_tokens":1000000,"total_tokens":2000000}}}"#.into()),
            WsFrame::Text(r#"{"type":"response.done","response":{"id":"r1","usage":{"input_tokens":1000000,"output_tokens":1000000,"total_tokens":2000000}}}"#.into()),
            WsFrame::Text(r#"{"type":"conversation.item.input_audio_transcription.completed","item_id":"i1","transcript":"ok","usage":{"type":"tokens","input_tokens":1000000,"output_tokens":0,"total_tokens":1000000}}"#.into()),
            WsFrame::Close(Some(1000)),
        ].into();
    }
    let registry =
        ChannelRegistry::new([Box::new(gproxy_channels::OpenAiChannel) as Box<dyn Channel>])
            .unwrap();
    let core = Core::new(host.clone(), registry).unwrap();
    (host, core)
}

fn request(query: &str) -> RequestCtx {
    RequestCtx {
        request_id: "direct-realtime".into(),
        client_ip: None,
        method: Method::GET,
        path: "/v1/realtime".into(),
        query: Some(query.into()),
        headers: HeaderMap::new(),
        body: Bytes::new(),
        upgrade: true,
        mode: RoutingMode::Aggregated,
    }
}

#[test]
fn direct_socket_settles_once_on_peer_close_client_close_or_drop() {
    for ending in ["peer", "client", "drop"] {
        let (host, core) = setup();
        let outcome = block_on(core.execute(&host, request("model=public%2Fmodel"))).unwrap();
        let ResponseBody::WebSocket(mut socket) = outcome.body else {
            panic!("websocket response")
        };
        assert!(host.state.lock().unwrap().settlements.is_empty());
        match ending {
            "peer" => while block_on(socket.recv()).unwrap().is_some() {},
            "client" => block_on(socket.send(WsFrame::Close(Some(1000)))).unwrap(),
            _ => {}
        }
        drop(socket);
        let state = host.state.lock().unwrap();
        assert_eq!(state.resolved_models[0].as_deref(), Some("public/model"));
        assert_eq!(state.settlements.len(), 1, "{ending}");
        assert_eq!(state.settlements[0].cost, Decimal::from(6));
        assert_eq!(state.settlements[0].usage.input_tokens, 2_000_000);
        assert_eq!(state.settlements[0].upstream_model, "actual");
        assert_eq!(state.admission_finishes, [true]);
        assert_eq!(state.captures.len(), 1);
    }
}

#[test]
fn sideband_requires_owned_call_binding_and_does_not_bill_again() {
    let (host, core) = setup();
    let error = block_on(core.execute(&host, request("call_id=rtc_owned"))).unwrap_err();
    assert!(matches!(error, CoreError::UnknownRoute(_)));
    assert_eq!(host.state.lock().unwrap().socket_opens, 0);
    block_on(host.bindings().unwrap().save(
        target().provider.id,
        1,
        "realtime_call",
        "rtc_owned",
        CredentialId(7),
        json!({"id":"rtc_owned","model":"bound-model"}),
    ))
    .unwrap();
    let sideband = request("call_id=rtc_owned&model=spoofed-model");
    let classified = crate::execution::request::classify(&sideband).unwrap();
    let mut plan = host.state.lock().unwrap().plan.clone().unwrap();
    block_on(crate::execution::resource::restore_realtime_model(
        &core,
        &mut plan,
        &classified,
        1,
    ))
    .unwrap();
    assert_eq!(plan.targets[0].upstream_model, "bound-model");
    let mut foreign = plan.clone();
    assert!(
        block_on(crate::execution::resource::restore_realtime_model(
            &core,
            &mut foreign,
            &classified,
            2
        ))
        .is_err()
    );
    let outcome = block_on(core.execute(&host, request("call_id=rtc_owned"))).unwrap();
    let ResponseBody::WebSocket(mut socket) = outcome.body else {
        panic!("websocket response")
    };
    while block_on(socket.recv()).unwrap().is_some() {}
    assert!(host.state.lock().unwrap().settlements.is_empty());
}

#[test]
fn malformed_usage_closes_socket_and_keeps_prior_totals() {
    let (host, core) = setup();
    host.state.lock().unwrap().socket_frames.insert(
        2,
        WsFrame::Text(r#"{"type":"response.done","response":{"id":"bad"}}"#.into()),
    );
    let outcome = block_on(core.execute(&host, request("model=alias"))).unwrap();
    let ResponseBody::WebSocket(mut socket) = outcome.body else {
        panic!("websocket response")
    };
    assert!(block_on(socket.recv()).is_ok());
    assert!(block_on(socket.recv()).is_ok());
    assert!(block_on(socket.recv()).is_err());
    let state = host.state.lock().unwrap();
    assert_eq!(state.settlements.len(), 1);
    assert_eq!(state.settlements[0].cost, Decimal::from(3));
    assert_eq!(state.settlements[0].ended, crate::Ended::Interrupted);
    assert!(state.socket_closed);
}

#[test]
fn handshake_failure_skips_dead_credentials_and_uses_the_same_failover_budget() {
    let (host, core) = setup();
    {
        let mut state = host.state.lock().unwrap();
        state.socket_statuses = [401, 101].into();
        let plan = state.plan.as_mut().unwrap();
        let mut next = plan.targets[0].clone();
        plan.targets.push(next.clone());
        next.credential = CredentialId(8);
        plan.targets.push(next);
        plan.budget.max_attempts = 3;
    }
    let outcome = block_on(core.execute(&host, request("model=alias"))).unwrap();
    drop(outcome);
    let state = host.state.lock().unwrap();
    assert_eq!(state.socket_opens, 2);
    assert_eq!(state.settlements.len(), 1);
    assert_eq!(state.settlements[0].credential_id, CredentialId(8));
    assert_eq!(state.settlements[0].cost, Decimal::from(6));
}
