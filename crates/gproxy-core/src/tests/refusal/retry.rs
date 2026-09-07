use serde_json::{Value, json};

use super::{enqueue, execute, message, setup};

#[test]
fn upstream_default_retries_only_the_recommended_model_and_redeems_credit() {
    for streaming in [false, true] {
        let (host, core) = setup(
            gproxy_channels::ClaudeCodeChannel,
            json!({"claude_fallback_mode":"default"}),
        );
        let mut refusal = message("claude-fable-5", "refusal", "", 100, 0);
        refusal["stop_details"]["recommended_model"] = json!("claude-opus-4-8");
        refusal["stop_details"]["fallback_credit_token"] = json!("opaque-test-credit");
        refusal["stop_details"]["fallback_has_prefill_claim"] = json!(false);
        enqueue(&host, refusal, streaming);
        enqueue(
            &host,
            message("claude-opus-4-8", "end_turn", "answer", 8, 5),
            streaming,
        );
        let (_, body) = execute(&host, &core, streaming);
        assert_eq!(body["model"], "claude-opus-4-8");
        assert_eq!(body["content"][0]["type"], "fallback");
        assert_eq!(body["content"][1]["text"], "answer");
        assert_eq!(body["usage"]["iterations"].as_array().unwrap().len(), 2);
        let state = host.state.lock().unwrap();
        assert_eq!(state.upstream_bodies.len(), 2);
        let first: Value = serde_json::from_slice(&state.upstream_bodies[0]).unwrap();
        let retry: Value = serde_json::from_slice(&state.upstream_bodies[1]).unwrap();
        assert_eq!(first["fallbacks"], "default");
        assert!(retry.get("fallbacks").is_none());
        assert_eq!(retry["fallback_credit_token"], "opaque-test-credit");
        for field in ["system", "messages", "thinking", "tools", "output_config"] {
            assert_eq!(first.get(field), retry.get(field), "{field}");
        }
        assert!(
            state.upstream_requests[0].0["anthropic-beta"]
                .to_str()
                .unwrap()
                .contains("server-side-fallback-2026-07-01")
        );
        assert!(
            !state.upstream_requests[1].0["anthropic-beta"]
                .to_str()
                .unwrap()
                .contains("server-side-fallback")
        );
        assert!(state.settlements[0].attempts[0].cost.is_zero());
        assert_eq!(state.settlements[0].attempts.len(), 2);
    }
}

#[test]
fn default_without_a_recommendation_preserves_the_refusal_and_does_not_guess_a_model() {
    for streaming in [false, true] {
        let (host, core) = setup(
            gproxy_channels::ClaudeApiChannel,
            json!({"claude_fallback_mode":"default"}),
        );
        enqueue(
            &host,
            message("claude-fable-5", "refusal", "", 100, 0),
            streaming,
        );
        let (_, body) = execute(&host, &core, streaming);
        assert_eq!(body["stop_reason"], "refusal");
        assert_eq!(host.state.lock().unwrap().upstream_bodies.len(), 1);
    }
}

fn gateway(channel: impl gproxy_channel_api::Channel + 'static, streaming: bool) {
    let (host, core) = setup(
        channel,
        json!({"base_url":"https://upstream.example", "claude_fallback_mode":"models", "claude_fallback_models":["claude-opus-4-8"]}),
    );
    enqueue(
        &host,
        message(
            "claude-fable-5",
            "refusal",
            if streaming { "" } else { "discarded partial" },
            10,
            if streaming { 0 } else { 3 },
        ),
        streaming,
    );
    enqueue(
        &host,
        message("claude-opus-4-8", "end_turn", "answer", 8, 5),
        streaming,
    );
    let (_, body) = execute(&host, &core, streaming);
    assert_eq!(body["model"], "claude-opus-4-8");
    assert!(!body.to_string().contains("discarded partial"));
    let state = host.state.lock().unwrap();
    assert_eq!(state.upstream_bodies.len(), 2);
    let first: Value = serde_json::from_slice(&state.upstream_bodies[0]).unwrap();
    assert!(first.get("fallbacks").is_none());
    assert_eq!(state.settlements[0].attempts.len(), 2);
    assert_eq!(state.settlements[0].attempts[0].cost.is_zero(), streaming);
}

#[test]
fn cloud_gateway_handles_early_stream_refusals_and_restarts_buffered_responses() {
    for streaming in [false, true] {
        gateway(gproxy_channels::AzureChannel, streaming);
        gateway(gproxy_channels::VercelChannel, streaming);
    }
}

#[test]
fn vertex_and_bedrock_retry_the_native_messages_endpoint() {
    for streaming in [false, true] {
        let (host, core) = setup(
            gproxy_channels::VertexChannel,
            json!({"claude_fallback_mode":"default"}),
        );
        host.state.lock().unwrap().credential.secret["project_id"] = json!("project-test");
        enqueue(
            &host,
            message("claude-fable-5", "refusal", "", 10, 0),
            streaming,
        );
        enqueue(
            &host,
            message("claude-opus-4-8", "end_turn", "answer", 8, 5),
            streaming,
        );
        execute(&host, &core, streaming);
        let state = host.state.lock().unwrap();
        assert!(state.upstream_requests[1].1.contains("claude-opus-4-8"));
        let body: Value = serde_json::from_slice(&state.upstream_bodies[1]).unwrap();
        assert!(body.get("model").is_none());
        assert_eq!(body["anthropic_version"], "vertex-2023-10-16");
    }
    let (host, core) = setup(
        gproxy_channels::AwsBedrockChannel,
        json!({"claude_fallback_mode":"default"}),
    );
    enqueue(
        &host,
        message("claude-fable-5", "refusal", "", 10, 0),
        false,
    );
    enqueue(
        &host,
        message("claude-opus-4-8", "end_turn", "answer", 8, 5),
        false,
    );
    execute(&host, &core, false);
    let state = host.state.lock().unwrap();
    assert!(state.upstream_requests[0].1.ends_with("/invoke"));
    assert!(
        state.upstream_requests[1]
            .1
            .ends_with("claude-opus-4-8/invoke")
    );
    for body in &state.upstream_bodies {
        let body: Value = serde_json::from_slice(body).unwrap();
        assert!(body.get("model").is_none());
        assert!(body.get("fallbacks").is_none());
        assert_eq!(body["anthropic_version"], "bedrock-2023-05-31");
        assert!(
            body["anthropic_beta"]
                .as_array()
                .unwrap()
                .iter()
                .any(|beta| beta == "fallback-credit-2026-07-01")
        );
    }
}
