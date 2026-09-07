use bytes::Bytes;
use http::StatusCode;
use serde_json::{Value, json};

use super::{enqueue, execute, message, setup};

fn reject(host: &super::MemoryHost, message: &str) {
    host.state.lock().unwrap().scripted.push_back((
        StatusCode::BAD_REQUEST,
        vec![Bytes::from(
            json!({"error":{"type":"invalid_request_error","message":message}}).to_string(),
        )],
    ));
}

fn refusal() -> Value {
    let mut response = message("claude-fable-5", "refusal", "partial  ", 20, 2);
    response["stop_details"]["fallback_credit_token"] = json!("opaque-test-credit");
    response["stop_details"]["fallback_has_prefill_claim"] = json!(true);
    response
}

#[test]
fn credit_continuation_echoes_content_and_keeps_prompt_fields_and_headers() {
    for streaming in [false, true] {
        let (host, core) = setup(
            gproxy_channels::AzureChannel,
            json!({"base_url":"https://upstream.example", "claude_fallback_mode":"models", "claude_fallback_models":["claude-opus-4-8"]}),
        );
        enqueue(&host, refusal(), streaming);
        enqueue(
            &host,
            message("claude-opus-4-8", "end_turn", " continued", 10, 3),
            streaming,
        );
        let (_, body) = execute(&host, &core, streaming);
        assert_eq!(body["content"][0]["text"], "partial  ");
        assert_eq!(body["content"][1]["type"], "fallback");
        assert_eq!(body["content"][2]["text"], " continued");
        let state = host.state.lock().unwrap();
        let first: Value = serde_json::from_slice(&state.upstream_bodies[0]).unwrap();
        let retry: Value = serde_json::from_slice(&state.upstream_bodies[1]).unwrap();
        assert_eq!(retry["messages"][0], first["messages"][0]);
        assert_eq!(retry["messages"][1]["content"][0]["text"], "partial");
        assert_eq!(
            state.upstream_requests[0].0["anthropic-beta"],
            state.upstream_requests[1].0["anthropic-beta"]
        );
        assert_eq!(state.settlements[0].attempts.len(), 2);
    }
}

#[test]
fn rejected_credit_shapes_degrade_but_transient_redemption_retries_unchanged() {
    for transient in [false, true] {
        let (host, core) = setup(
            gproxy_channels::AzureChannel,
            json!({"base_url":"https://upstream.example", "claude_fallback_mode":"models", "claude_fallback_models":["claude-opus-4-8"]}),
        );
        enqueue(&host, refusal(), false);
        if transient {
            reject(&host, "redemption temporarily unavailable");
        } else {
            reject(&host, "request body does not match continuation");
            reject(&host, "fallback_credit_token is invalid");
        }
        enqueue(
            &host,
            message("claude-opus-4-8", "end_turn", "answer", 10, 3),
            false,
        );
        let (status, _) = execute(&host, &core, false);
        assert!(status.is_success());
        let state = host.state.lock().unwrap();
        let bodies = state
            .upstream_bodies
            .iter()
            .map(|body| serde_json::from_slice::<Value>(body).unwrap())
            .collect::<Vec<_>>();
        if transient {
            assert_eq!(bodies[1], bodies[2]);
            assert_eq!(state.wait_calls, 1);
        } else {
            assert_eq!(bodies[1]["messages"].as_array().unwrap().len(), 2);
            assert_eq!(bodies[2]["messages"].as_array().unwrap().len(), 1);
            assert!(bodies[2].get("fallback_credit_token").is_some());
            assert!(bodies[3].get("fallback_credit_token").is_none());
        }
        assert!(state.settlements[0].attempts[1].cost.is_zero());
    }
}

#[test]
fn completed_server_tools_never_fall_through_to_a_tokenless_retry() {
    let (host, core) = setup(
        gproxy_channels::AzureChannel,
        json!({"base_url":"https://upstream.example", "claude_fallback_mode":"models", "claude_fallback_models":["claude-opus-4-8"]}),
    );
    let mut response = refusal();
    response["content"] = json!([
        {"type":"server_tool_use","id":"srv_1","name":"web_search","input":{}},
        {"type":"web_search_tool_result","tool_use_id":"srv_1","content":[]}
    ]);
    enqueue(&host, response, false);
    reject(&host, "request body does not match continuation");
    reject(
        &host,
        "fallback_credit_token must be redeemed by continuing the partial response",
    );
    let (status, _) = execute(&host, &core, false);
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let state = host.state.lock().unwrap();
    assert_eq!(state.upstream_bodies.len(), 3);
    let last: Value = serde_json::from_slice(&state.upstream_bodies[2]).unwrap();
    assert!(last.get("fallback_credit_token").is_some());
    assert!(!state.settlements[0].attempts[0].cost.is_zero());
}

#[test]
fn followups_stay_on_the_accepted_model_in_the_same_conversation() {
    let (host, core) = setup(
        gproxy_channels::AzureChannel,
        json!({"base_url":"https://upstream.example", "claude_fallback_mode":"default"}),
    );
    enqueue(
        &host,
        message("claude-fable-5", "refusal", "", 20, 0),
        false,
    );
    enqueue(
        &host,
        message("claude-opus-4-8", "end_turn", "answer", 10, 3),
        false,
    );
    execute(&host, &core, false);
    enqueue(
        &host,
        message("claude-opus-4-8", "end_turn", "next", 10, 3),
        false,
    );
    execute(&host, &core, false);
    let state = host.state.lock().unwrap();
    assert_eq!(state.upstream_bodies.len(), 3);
    let followup: Value = serde_json::from_slice(&state.upstream_bodies[2]).unwrap();
    assert_eq!(followup["model"], "claude-opus-4-8");
}
