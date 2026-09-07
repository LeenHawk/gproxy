use rust_decimal::Decimal;
use serde_json::json;

use super::{enqueue, execute, message, setup};

fn price(input: u64, output: u64) -> crate::Pricing {
    crate::Pricing {
        input_per_million: Decimal::from(input),
        output_per_million: Decimal::from(output),
        cached_input_per_million: Some(Decimal::new(3, 1)),
        service_tier: None,
        tiers: Vec::new(),
        metric_rates: Default::default(),
        conditional_metric_rates: Default::default(),
    }
}

#[test]
fn server_iterations_use_each_model_rate_without_double_counting_top_level_usage() {
    for streaming in [false, true] {
        let (host, core) = setup(gproxy_channels::ClaudeApiChannel, json!({}));
        {
            let mut state = host.state.lock().unwrap();
            state
                .model_prices
                .insert("claude-fable-5".into(), price(2, 4));
            state
                .model_prices
                .insert("claude-opus-5".into(), price(5, 10));
            state
                .model_prices
                .insert("claude-opus-4-8".into(), price(3, 6));
        }
        let mut response = message("claude-opus-4-8", "end_turn", "answer", 8, 5);
        response["usage"]["iterations"] = json!([
            {"type":"message","model":"claude-fable-5","input_tokens":100,"output_tokens":0},
            {"type":"message","model":"claude-opus-5","input_tokens":10,"output_tokens":3},
            {"type":"fallback_message","model":"claude-opus-4-8","input_tokens":8,"output_tokens":5}
        ]);
        enqueue(&host, response, streaming);
        let (status, body) = execute(&host, &core, streaming);
        assert!(status.is_success());
        assert_eq!(body["usage"]["input_tokens"], 8);
        let state = host.state.lock().unwrap();
        let settled = &state.settlements[0];
        assert_eq!(settled.attempts.len(), 3);
        assert_eq!(settled.attempts[0].cost, Decimal::ZERO);
        assert_eq!(settled.attempts[1].cost, Decimal::new(80, 6));
        assert_eq!(settled.attempts[2].cost, Decimal::new(54, 6));
        assert_eq!(settled.cost, Decimal::new(134, 6));
        assert_eq!(settled.usage.input_tokens, 8);
        assert_eq!(settled.upstream_model, "claude-opus-4-8");
        assert_eq!(state.admission_finishes.len(), 1);
    }
}

#[test]
fn refusal_is_free_only_before_output_and_empty_end_turn_remains_billable() {
    for streaming in [false, true] {
        for (reason, text, output, expected) in [
            ("refusal", "", 0, 0),
            ("refusal", "partial", 3, 16),
            ("end_turn", "", 0, 10),
        ] {
            let (host, core) = setup(gproxy_channels::ClaudeApiChannel, json!({}));
            enqueue(
                &host,
                message("claude-fable-5", reason, text, 10, output),
                streaming,
            );
            let (_, body) = execute(&host, &core, streaming);
            assert_eq!(body["stop_reason"], reason);
            let state = host.state.lock().unwrap();
            assert_eq!(state.settlements[0].cost, Decimal::new(expected, 6));
        }
    }
}
