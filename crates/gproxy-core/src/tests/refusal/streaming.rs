use bytes::Bytes;
use futures_util::{FutureExt, StreamExt};
use serde_json::json;

use super::{block_on, enqueue, execute, message, request, setup};
use crate::ResponseBody;

#[test]
fn text_arrives_before_upstream_eof_on_both_initial_and_fallback_attempts() {
    for fallback in [false, true] {
        let (host, core) = setup(
            gproxy_channels::AzureChannel,
            json!({
                "base_url":"https://upstream.example", "claude_fallback_mode":"models", "claude_fallback_models":["claude-opus-4-8"]
            }),
        );
        if fallback {
            enqueue(&host, message("claude-fable-5", "refusal", "", 10, 0), true);
        }
        enqueue(
            &host,
            message(
                if fallback {
                    "claude-opus-4-8"
                } else {
                    "claude-fable-5"
                },
                "end_turn",
                "live answer",
                8,
                5,
            ),
            true,
        );
        host.state.lock().unwrap().scripted_pending_at = Some(if fallback { 2 } else { 1 });
        let mut input = request(true, "live-fallback");
        input.path = "/v1/messages".into();
        input.body = Bytes::from(json!({"model":"claude-fable-5","max_tokens":128,"stream":true,"messages":[{"role":"user","content":"hello"}]}).to_string());
        let result = block_on(core.execute(&host, input)).unwrap();
        let ResponseBody::Stream(mut stream) = result.body else {
            panic!("expected live stream")
        };
        let mut received = String::new();
        for _ in 0..8 {
            let chunk = stream
                .next()
                .now_or_never()
                .expect("output must not wait for upstream EOF")
                .unwrap()
                .unwrap();
            received.push_str(&String::from_utf8_lossy(&chunk));
            if received.contains("live answer") {
                break;
            }
        }
        assert!(received.contains("live answer"));
        assert_eq!(received.matches("\"type\":\"message_start\"").count(), 1);
        if fallback {
            assert!(received.contains("\"type\":\"fallback\""));
        }
    }
}

#[test]
fn a_late_refusal_without_continuation_never_replays_already_delivered_text() {
    let (host, core) = setup(
        gproxy_channels::AzureChannel,
        json!({
            "base_url":"https://upstream.example", "claude_fallback_mode":"models", "claude_fallback_models":["claude-opus-4-8"]
        }),
    );
    enqueue(
        &host,
        message("claude-fable-5", "refusal", "already streamed", 10, 3),
        true,
    );
    let (_, body) = execute(&host, &core, true);
    assert_eq!(body["stop_reason"], "refusal");
    assert_eq!(body["content"][0]["text"], "already streamed");
    let state = host.state.lock().unwrap();
    assert_eq!(state.upstream_bodies.len(), 1);
    assert!(!state.settlements[0].cost.is_zero());
}
