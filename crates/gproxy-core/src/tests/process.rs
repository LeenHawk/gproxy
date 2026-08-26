use std::sync::Arc;

use bytes::Bytes;
use gproxy_channel_api::{StreamDecoder, StreamEnd};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, StreamFraming};
use serde_json::{Value, json};

use super::memory::MemoryHost;
use super::{block_on, core, request, target};
use crate::ResponseBody;
use crate::control::{FailoverBudget, Plan};
use crate::process::{RuleModels, RuleSpec};
use crate::routing::RoutingRuleSpec;

#[test]
fn process_rules_run_on_provider_native_request_in_rank_order() {
    let host = MemoryHost::new(false);
    let core = core(&host).expect("core");
    let mut selected = target();
    selected.rules.routing = routing();
    selected.rules.process = Arc::from(
        crate::process::compile_all(&[
            spec(
                1,
                "cache_breakpoint",
                json!({"target":"system","ttl":"1h"}),
                0,
            ),
            spec(
                2,
                "system_text",
                json!({"text":"operator policy","position":"prepend"}),
                10,
            ),
        ])
        .expect("compiled rules"),
    );
    host.state.lock().expect("state lock").plan = Some(plan(selected));

    block_on(core.execute(&host, rule_request(false, "rank"))).expect("execute");
    let state = host.state.lock().expect("state lock");
    let native: Value = serde_json::from_slice(state.upstream_bodies.last().expect("request body"))
        .expect("native request json");
    assert_eq!(native["system"][0]["text"], "operator policy");
    assert_eq!(native["system"][0]["cache_control"]["ttl"], "1h");
    drop(state);

    let mut unmodified = target();
    unmodified.rules.routing = routing();
    host.state.lock().expect("state lock").plan = Some(plan(unmodified));
    block_on(core.execute(&host, rule_request(false, "detached"))).expect("execute detached");
    let state = host.state.lock().expect("state lock");
    let native: Value = serde_json::from_slice(state.upstream_bodies.last().expect("request body"))
        .expect("native request json");
    assert!(native.get("system").is_none());
}

#[test]
fn transform_phase_controls_native_request_and_response() {
    for (phase, request_model) in [("both", "processed-model"), ("response", "upstream-model")] {
        let host = MemoryHost::new(false);
        let core = core(&host).expect("core");
        let mut selected = target();
        selected.rules.routing = routing();
        selected.rules.process = Arc::from(
            crate::process::compile_all(&[spec(
                1,
                "transform",
                json!({
                    "phase": phase,
                    "locate": {"path":"model"},
                    "actions": [{"op":"replace_text","with":"processed-model"}]
                }),
                0,
            )])
            .expect("compiled transform"),
        );
        host.state.lock().expect("state lock").plan = Some(plan(selected));
        let outcome = block_on(core.execute(&host, rule_request(false, phase))).expect("execute");
        let state = host.state.lock().expect("state lock");
        let native: Value =
            serde_json::from_slice(state.upstream_bodies.last().expect("request body"))
                .expect("native request json");
        assert_eq!(native["model"], request_model);
        drop(state);
        let ResponseBody::Full(body) = outcome.body else {
            panic!("buffered response expected")
        };
        let outward: Value = serde_json::from_slice(&body).expect("outward response json");
        assert_eq!(outward["model"], "processed-model");
    }
}

#[test]
fn streaming_response_rule_releases_the_first_frame_before_finish() {
    let rules: Arc<[_]> = Arc::from(
        crate::process::compile_all(&[spec(
            1,
            "transform",
            json!({
                "phase":"response",
                "locate":{"path":"delta"},
                "actions":[{"op":"replace_text","with":"changed"}]
            }),
            0,
        )])
        .expect("compiled transform"),
    );
    let key = OperationKey::content(
        Operation::StreamGenerateContent,
        ContentGenerationKind::OpenAiResponses,
    );
    let mut decoder = crate::process::ResponseRuleDecoder::new(
        None,
        rules,
        key,
        StreamFraming::Sse,
        RuleModels::new("upstream-model", None),
        Default::default(),
    )
    .expect("response decoder");
    let frames = decoder
        .push(Bytes::from_static(b"data: {\"delta\":\"original\"}\n\n"))
        .expect("first frame");
    assert_eq!(frames.len(), 1);
    assert!(
        std::str::from_utf8(&frames[0].0)
            .expect("utf8")
            .contains("changed")
    );
    assert!(
        decoder
            .finish(StreamEnd::Complete)
            .expect("finish")
            .frames
            .is_empty()
    );
}

#[test]
fn alternate_route_model_matches_without_changing_primary_semantics() {
    let rules = crate::process::compile_all(&[RuleSpec {
        filter_model_pattern: Some("client-*".into()),
        ..spec(
            1,
            "rewrite",
            json!({"path":"matched","action":"set","value_json":true}),
            0,
        )
    }])
    .expect("compiled rule");
    let key = OperationKey::content(
        Operation::GenerateContent,
        ContentGenerationKind::OpenAiResponses,
    );
    let original = Bytes::from_static(br#"{"matched":false}"#);
    let unchanged = crate::process::apply_request(
        &rules,
        key,
        RuleModels::new("upstream-model", None),
        &Default::default(),
        original.clone(),
    );
    assert_eq!(unchanged.body.as_ptr(), original.as_ptr());
    let matched = crate::process::apply_request(
        &rules,
        key,
        RuleModels::new("upstream-model", Some("client-alias")),
        &Default::default(),
        original,
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&matched.body).unwrap()["matched"],
        true
    );
}

fn spec(id: i64, kind: &str, config: Value, sort_order: i64) -> RuleSpec {
    RuleSpec {
        id,
        kind: kind.into(),
        config,
        filter_model_pattern: None,
        filter_operations: None,
        filter_header_pattern: None,
        sort_order,
        enabled: true,
    }
}

fn rule_request(stream: bool, id: &str) -> crate::RequestCtx {
    let mut request = request(stream, id);
    request.body = Bytes::from(
        json!({
            "model": "client-alias",
            "input": "hello",
            "max_output_tokens": 32,
            "stream": stream
        })
        .to_string(),
    );
    request
}

fn routing() -> Arc<[crate::routing::CompiledRoutingRule]> {
    Arc::from(
        crate::routing::compile_all(&[RoutingRuleSpec {
            id: 1,
            operation: "generate_content".into(),
            kind: "openai_responses".into(),
            implementation: "transform_to".into(),
            dest_operation: Some("generate_content".into()),
            dest_kind: Some("claude_messages".into()),
            sort_order: 0,
            enabled: true,
        }])
        .expect("compiled routing"),
    )
}

fn plan(target: crate::Target) -> Plan {
    Plan {
        targets: vec![target],
        budget: FailoverBudget { max_attempts: 1 },
    }
}
