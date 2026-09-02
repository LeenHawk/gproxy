use bytes::Bytes;
use gproxy_channel_api::Channel;
use serde_json::json;

use super::memory::MemoryHost;
use super::{block_on, request};
use crate::boundary::{ResponseBody, RoutingMode};
use crate::control::{FailoverBudget, Plan, ProviderRef, Target};
use crate::host::CredentialId;
use crate::{Core, InitError};

#[test]
fn scoped_claudeweb_models_come_only_from_the_current_credential() -> Result<(), InitError> {
    let host = MemoryHost::with_session_spawner();
    {
        let mut state = host.state.lock().expect("state lock");
        state.credential.channel = "claudeweb".into();
        state.credential.secret = json!({
            "cookie": "sessionKey=test",
            "account_uuid": "account-test",
            "validated_at_ms": unix_now_ms(),
            "claude_ai_bootstrap_models_config": {
                "models": [{ "model_id": "claude-web-model", "label": "Claude Web Model" }]
            }
        });
        state.plan = Some(plan(target("claudeweb", "claude-provider")));
    }
    let core = channel_core(&host, gproxy_channels::ClaudeWebChannel)?;

    let list = block_on(core.execute(&host, model_request("/v1/models", "claude-provider")))
        .expect("scoped local model list");
    let models = body(list);
    assert_eq!(models["data"].as_array().unwrap().len(), 1);
    assert_eq!(models["data"][0]["id"], "claude-web-model");
    assert_eq!(models["data"][0]["display_name"], "Claude Web Model");

    let missing =
        block_on(core.execute(&host, model_request("/v1/models/alias", "claude-provider")))
            .expect("scoped local get-model response");
    assert_eq!(missing.status, http::StatusCode::NOT_FOUND);
    assert!(
        host.state
            .lock()
            .expect("state lock")
            .upstream_requests
            .is_empty()
    );
    Ok(())
}

#[test]
fn scoped_local_models_without_a_catalog_return_empty_instead_of_global_fallback()
-> Result<(), InitError> {
    let host = MemoryHost::with_session_spawner();
    {
        let mut state = host.state.lock().expect("state lock");
        state.credential.channel = "claudeweb".into();
        state.credential.secret = json!({
            "cookie": "sessionKey=test",
            "account_uuid": "account-test",
            "validated_at_ms": unix_now_ms()
        });
        state.plan = Some(plan(target("claudeweb", "claude-provider")));
    }
    let core = channel_core(&host, gproxy_channels::ClaudeWebChannel)?;
    let outcome = block_on(core.execute(&host, model_request("/v1/models", "claude-provider")))
        .expect("empty scoped local model list");
    assert_eq!(body(outcome)["data"], json!([]));
    Ok(())
}

#[test]
fn scoped_local_get_uses_the_channel_list_instead_of_global_models() -> Result<(), InitError> {
    let host = MemoryHost::new(false);
    {
        let mut state = host.state.lock().expect("state lock");
        state.credential.channel = "cline".into();
        state.credential.secret = json!({ "api_key": "test" });
        state.plan = Some(plan(target("cline", "cline-provider")));
    }
    let core = channel_core(&host, gproxy_channels::ClineChannel)?;
    let outcome = block_on(core.execute(
        &host,
        model_request("/v1/models/fresh-model", "cline-provider"),
    ))
    .expect("scoped local get-model response");
    assert_eq!(outcome.status, http::StatusCode::OK);
    assert_eq!(body(outcome)["id"], "fresh-model");
    let missing =
        block_on(core.execute(&host, model_request("/v1/models/alias", "cline-provider")))
            .expect("missing scoped local model response");
    assert_eq!(missing.status, http::StatusCode::NOT_FOUND);
    let state = host.state.lock().expect("state lock");
    assert_eq!(state.upstream_requests.len(), 2);
    assert!(
        state.upstream_requests[0]
            .1
            .ends_with("/ai/cline/recommended-models")
    );
    Ok(())
}

#[test]
fn scoped_vertex_express_models_use_its_bundled_catalog() -> Result<(), InitError> {
    let host = MemoryHost::new(false);
    {
        let mut state = host.state.lock().expect("state lock");
        state.credential.channel = "vertexexpress".into();
        state.credential.secret = json!({ "api_key": "test" });
        state.plan = Some(plan(target("vertexexpress", "vertex-provider")));
    }
    let core = channel_core(&host, gproxy_channels::VertexExpressChannel)?;
    let outcome = block_on(core.execute(&host, model_request("/v1/models", "vertex-provider")))
        .expect("Vertex Express bundled model list");
    let models = body(outcome);
    assert_eq!(models["data"].as_array().unwrap().len(), 12);
    assert!(models["data"].as_array().unwrap().iter().any(|model| {
        model["id"] == "gemini-3.1-pro-preview" && model["context_window"] == 1_048_576
    }));
    Ok(())
}

fn channel_core<C: Channel + 'static>(
    host: &MemoryHost,
    channel: C,
) -> Result<Core<MemoryHost>, InitError> {
    let channels =
        gproxy_channel_api::ChannelRegistry::new([Box::new(channel) as Box<dyn Channel>])
            .expect("channel registry");
    Core::new(host.clone(), channels)
}

fn target(channel: &str, provider: &str) -> Target {
    Target {
        provider: ProviderRef {
            id: 3,
            name: provider.into(),
            channel: channel.into(),
            settings: json!({}),
            fingerprint: None,
            proxy_url: None,
            traffic_blacklist: Default::default(),
        },
        credential: CredentialId(7),
        upstream_model: "unused".into(),
        tier: 0,
        rules: Default::default(),
    }
}

fn plan(target: Target) -> Plan {
    Plan {
        targets: vec![target],
        budget: FailoverBudget { max_attempts: 1 },
    }
}

fn model_request(path: &str, provider: &str) -> crate::RequestCtx {
    let mut request = request(false, "scoped-local-models");
    request.method = http::Method::GET;
    request.path = path.into();
    request.body = Bytes::new();
    request.mode = RoutingMode::Scoped {
        provider: provider.into(),
    };
    request
}

fn body(outcome: crate::ExecOutcome) -> serde_json::Value {
    let ResponseBody::Full(body) = outcome.body else {
        panic!("local model response was not buffered")
    };
    serde_json::from_slice(&body).expect("model response JSON")
}

fn unix_now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock")
        .as_millis()
        .try_into()
        .expect("Unix milliseconds fit in i64")
}
