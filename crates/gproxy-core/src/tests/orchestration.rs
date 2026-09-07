use bytes::Bytes;
use futures_util::StreamExt;
use gproxy_channel_api::{Channel, ChannelRegistry};
use serde_json::json;

use super::block_on;
use super::memory::MemoryHost;
use crate::boundary::{RequestCtx, ResponseBody, RoutingMode};
use crate::control::{FailoverBudget, Plan, ProviderRef, Target};
use crate::host::CredentialId;
use crate::{Core, InitError};

#[test]
fn continuation_channels_fail_loudly_without_host_state() {
    let registry =
        gproxy_channel_api::ChannelRegistry::new([
            Box::new(super::channel::NeedsContinuation) as Box<dyn Channel>
        ])
        .expect("registry");
    let error = match Core::new(MemoryHost::new(false), registry) {
        Ok(_) => panic!("missing continuation store was accepted"),
        Err(error) => error,
    };
    assert!(matches!(
        error,
        InitError::ContinuationsUnavailable {
            channel: "continuation-test"
        }
    ));
}

#[test]
fn claudeweb_new_and_resume_turns_transfer_one_scoped_stream() {
    let host = MemoryHost::with_continuations();
    let target = Target {
        provider: ProviderRef {
            id: 44,
            name: "claude-web".into(),
            channel: "claudeweb".into(),
            settings: json!({"base_url":"https://upstream.test"}),
            fingerprint: None,
            proxy_url: None,
            traffic_blacklist: Default::default(),
        },
        credential: CredentialId(7),
        upstream_model: "claude-opus-4-8".into(),
        tier: 0,
        rules: Default::default(),
    };
    let plan = Plan {
        targets: vec![target],
        budget: FailoverBudget { max_attempts: 1 },
    };
    {
        let mut state = host.state.lock().expect("state lock");
        state.credential.channel = "claudeweb".into();
        state.credential.secret = json!({
            "cookie":"session-cookie",
            "account_uuid":"org-1",
            "capabilities":["chat","pro"],
            "validated_at_ms":i64::MAX
        });
        state.plan = Some(plan);
    }
    let channels =
        ChannelRegistry::new([Box::new(gproxy_channels::ClaudeWebChannel) as Box<dyn Channel>])
            .expect("channel registry");
    let core = Core::new(host.clone(), channels).expect("continuation-capable core");

    let first = request(
        "web-first",
        json!({
            "model":"claude-opus-4-8",
            "stream":true,
            "messages":[{"role":"user","content":"use weather"}],
            "tools":[{"name":"weather","input_schema":{"type":"object"}}]
        }),
    );
    let outcome = block_on(core.execute(&host, first)).expect("new turn");
    let ResponseBody::Stream(mut body) = outcome.body else {
        panic!("new turn was not streaming")
    };
    let first = block_on(async move {
        let mut output = Vec::new();
        while let Some(chunk) = body.next().await {
            output.extend_from_slice(&chunk.expect("new turn chunk"));
        }
        String::from_utf8(output).expect("new turn UTF-8")
    });
    assert!(first.contains("message_stop"));
    assert_eq!(
        host.state.lock().expect("state lock").continuations.len(),
        1
    );

    let second = request(
        "web-resume",
        json!({
            "model":"claude-opus-4-8",
            "stream":true,
            "messages":[
                {"role":"assistant","content":[{"type":"tool_use","id":"toolu-web","name":"weather","input":{}}]},
                {"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu-web","content":"sunny"}]}
            ]
        }),
    );
    let outcome = block_on(core.execute(&host, second)).expect("resume turn");
    let ResponseBody::Stream(mut body) = outcome.body else {
        panic!("resume turn was not streaming")
    };
    let second = block_on(async move {
        let mut output = Vec::new();
        while let Some(chunk) = body.next().await {
            output.extend_from_slice(&chunk.expect("resume chunk"));
        }
        String::from_utf8(output).expect("resume UTF-8")
    });
    assert!(second.contains("Sunny"));
    let state = host.state.lock().expect("state lock");
    assert!(state.continuations.is_empty());
    assert_eq!(state.captures.len(), 3);
    assert!(
        state
            .captures
            .iter()
            .all(|capture| capture.provider_id == Some(44))
    );
}

fn request(id: &str, body: serde_json::Value) -> RequestCtx {
    RequestCtx {
        request_id: id.into(),
        client_ip: None,
        method: http::Method::POST,
        path: "/v1/messages".into(),
        query: None,
        headers: http::HeaderMap::new(),
        body: Bytes::from(body.to_string()),
        upgrade: false,
        force_model_refresh: false,
        mode: RoutingMode::Aggregated,
    }
}
