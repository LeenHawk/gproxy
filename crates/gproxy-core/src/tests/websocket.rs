use bytes::Bytes;
use gproxy_channel_api::Channel;
use http::Method;
use serde_json::{Value, json};

use super::memory::MemoryHost;
use super::{block_on, request};
use crate::control::{FailoverBudget, Plan, ProviderRef, Target};
use crate::host::CredentialId;
use crate::{Core, InitError, ResponseBody};

#[test]
fn codex_remote_server_requires_websocket_upgrade_before_forwarding() -> Result<(), InitError> {
    let host = MemoryHost::new(false);
    host.state.lock().expect("state lock").credential.channel = "codex".into();
    host.state.lock().expect("state lock").plan = Some(Plan {
        targets: vec![target("codex", 5)],
        budget: FailoverBudget { max_attempts: 1 },
    });
    let core = core(&host, Box::new(gproxy_channels::CodexChannel))?;
    let mut request = request(false, "codex-remote-without-upgrade");
    request.method = Method::GET;
    request.path = "/api/codex/remote/control/server".into();
    request.body = Bytes::new();
    let outcome = block_on(core.execute(&host, request)).expect("upgrade-required response");
    assert_eq!(outcome.status, http::StatusCode::UPGRADE_REQUIRED);
    assert!(
        host.state
            .lock()
            .expect("state lock")
            .authorizations
            .is_empty()
    );
    Ok(())
}

#[test]
fn responses_socket_reuses_upstream_and_settles_injected_responses() -> Result<(), InitError> {
    let host = MemoryHost::new(false);
    {
        let mut state = host.state.lock().expect("state lock");
        state.credential.channel = "openai".into();
        state.credential.kind = "api_key".into();
        state.credential.secret = json!({"api_key":"upstream-secret"});
        state.credential.version = 4;
        state.plan = Some(Plan {
            targets: vec![target("openai", 6), target("openai", 7)],
            budget: FailoverBudget { max_attempts: 2 },
        });
        state.socket_statuses = [500, 101].into();
        state.socket_frames = [
            gproxy_channel_api::WsFrame::Text(response_event("response.created", 0)),
            gproxy_channel_api::WsFrame::Text(
                json!({"type":"response.inject.created","sequence_number":1,"response_id":"resp_ws"}).to_string(),
            ),
            gproxy_channel_api::WsFrame::Text(response_event("response.completed", 2)),
        ]
        .into();
    }
    let core = core(&host, Box::new(gproxy_channels::OpenAiChannel))?;
    let mut request = request(false, "responses-native-ws");
    request.method = Method::GET;
    request.body = Bytes::new();
    request.upgrade = true;
    let outcome = block_on(core.execute(&host, request)).expect("websocket upgrade");
    let ResponseBody::WebSocket(mut socket) = outcome.body else {
        panic!("Responses request did not upgrade")
    };
    block_on(socket.send(gproxy_channel_api::WsFrame::Text(
        json!({"type":"response.create","model":"upstream-model","input":"hi"}).to_string(),
    )))
    .unwrap();
    let Some(gproxy_channel_api::WsFrame::Text(created)) = block_on(socket.recv()).unwrap() else {
        panic!("missing response.created")
    };
    assert_eq!(
        serde_json::from_str::<Value>(&created).unwrap()["type"],
        "response.created"
    );
    block_on(
        socket.send(gproxy_channel_api::WsFrame::Text(
            json!({"type":"response.inject","response_id":"resp_ws","input":[
                {"type":"function_call_output","call_id":"call_1","output":"ok"}
            ]})
            .to_string(),
        )),
    )
    .unwrap();
    let _ = block_on(socket.recv()).unwrap();
    let _ = block_on(socket.recv()).unwrap();
    host.state.lock().expect("state lock").socket_frames = [
        gproxy_channel_api::WsFrame::Text(response_event("response.created", 3)),
        gproxy_channel_api::WsFrame::Text(response_event("response.completed", 4)),
    ]
    .into();
    block_on(socket.send(gproxy_channel_api::WsFrame::Text(
        json!({"type":"response.create","model":"upstream-model","input":"again"}).to_string(),
    )))
    .unwrap();
    let _ = block_on(socket.recv()).unwrap();
    let _ = block_on(socket.recv()).unwrap();
    let state = host.state.lock().expect("state lock");
    assert_eq!(state.socket_opens, 2);
    assert_eq!(state.socket_sent.len(), 3);
    assert_eq!(state.settlements.len(), 2);
    assert_eq!(state.settlements[0].usage.input_tokens, 3);
    assert_eq!(state.settlements[0].usage.output_tokens, 2);
    Ok(())
}

fn core(host: &MemoryHost, channel: Box<dyn Channel>) -> Result<Core<MemoryHost>, InitError> {
    let channels = gproxy_channel_api::ChannelRegistry::new([channel]).expect("registry");
    Core::new(host.clone(), channels)
}

fn target(channel: &str, id: i64) -> Target {
    Target {
        provider: ProviderRef {
            id,
            name: format!("{channel}-provider"),
            channel: channel.into(),
            settings: json!({}),
            fingerprint: None,
            proxy_url: None,
            traffic_blacklist: Default::default(),
        },
        credential: CredentialId(7),
        upstream_model: "upstream-model".into(),
        tier: 0,
        rules: Default::default(),
    }
}

fn response_event(kind: &str, sequence_number: u64) -> String {
    json!({
        "type":kind,"sequence_number":sequence_number,
        "response":{"id":"resp_ws","object":"response","created_at":0,
            "status":"completed","model":"upstream-model","output":[],
            "usage":{"input_tokens":3,"output_tokens":2,"total_tokens":5}}
    })
    .to_string()
}
