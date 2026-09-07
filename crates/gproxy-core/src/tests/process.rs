use std::sync::Arc;

use bytes::Bytes;
use futures_util::StreamExt;
use gproxy_channel_api::{StreamDecoder, StreamEnd};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, StreamFraming};
use http::HeaderMap;
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

#[test]
fn cache_rules_target_flat_wire_blocks_and_keep_provider_limits() {
    let claude = apply_cache_rule(
        ContentGenerationKind::ClaudeMessages,
        json!({
            "messages":[
                {"role":"user","content":[{"type":"text","text":"first"}]},
                {"role":"assistant","content":[
                    {"type":"text","text":"second"},
                    {"type":"text","text":"third"}
                ]}
            ]
        }),
        "message",
        Some(1),
        Some("5m"),
    );
    assert_eq!(
        claude["messages"][0]["content"][0]["cache_control"]["ttl"],
        "5m"
    );
    assert!(
        claude["messages"][1]["content"][1]
            .get("cache_control")
            .is_none()
    );

    let responses = apply_cache_rule(
        ContentGenerationKind::OpenAiResponses,
        json!({"instructions":"stable","input":"hello"}),
        "system",
        None,
        Some("30m"),
    );
    assert_eq!(responses["instructions"], "stable");
    assert_eq!(responses["input"][0]["role"], "developer");
    assert_eq!(
        responses["input"][0]["content"][0]["prompt_cache_breakpoint"]["mode"],
        "explicit"
    );
    assert_eq!(responses["input"][1]["content"][0]["text"], "hello");
    assert_eq!(responses["prompt_cache_options"]["ttl"], "30m");

    let chat = apply_cache_rule(
        ContentGenerationKind::OpenAiChat,
        json!({
            "messages":[{"role":"user","content":[
                {"type":"text","text":"first"},
                {"type":"text","text":"second"}
            ]}]
        }),
        "message",
        Some(1),
        None,
    );
    assert_eq!(
        chat["messages"][0]["content"][0]["prompt_cache_breakpoint"]["mode"],
        "explicit"
    );
    assert!(
        chat["messages"][0]["content"][1]
            .get("prompt_cache_breakpoint")
            .is_none()
    );

    let capped = apply_cache_rule(
        ContentGenerationKind::ClaudeMessages,
        json!({
            "system":[
                {"type":"text","text":"a","cache_control":{"type":"ephemeral"}},
                {"type":"text","text":"b","cache_control":{"type":"ephemeral"}},
                {"type":"text","text":"c","cache_control":{"type":"ephemeral"}},
                {"type":"text","text":"d","cache_control":{"type":"ephemeral"}}
            ],
            "messages":[{"role":"user","content":[{"type":"text","text":"unmarked"}]}]
        }),
        "message",
        None,
        None,
    );
    assert!(
        capped["messages"][0]["content"][0]
            .get("cache_control")
            .is_none()
    );
}

fn apply_cache_rule(
    kind: ContentGenerationKind,
    body: Value,
    target: &str,
    index: Option<i64>,
    ttl: Option<&str>,
) -> Value {
    let rules = crate::process::compile_all(&[spec(
        99,
        "cache_breakpoint",
        json!({"target":target,"index":index,"ttl":ttl}),
        0,
    )])
    .unwrap();
    let mutation = crate::process::apply_request(
        &rules,
        OperationKey::content(Operation::GenerateContent, kind),
        RuleModels::new("model", None),
        &HeaderMap::new(),
        Bytes::from(serde_json::to_vec(&body).unwrap()),
    );
    serde_json::from_slice(&mutation.body).unwrap()
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

#[test]
fn streaming_client_routed_onto_the_buffered_sibling_gets_a_synthesized_stream() {
    let host = MemoryHost::new(false);
    let core = core(&host).expect("core");
    let mut selected = target();
    selected.rules.routing = synthesizing_routing();
    host.state.lock().expect("state lock").plan = Some(plan(selected));

    let outcome = block_on(core.execute(&host, rule_request(true, "synth"))).expect("execute");
    let state = host.state.lock().expect("state lock");
    let (_, url) = state.upstream_requests.last().expect("upstream request");
    assert!(url.ends_with("/v1/messages"), "{url}");
    let native: Value = serde_json::from_slice(state.upstream_bodies.last().expect("request body"))
        .expect("native request json");
    assert_ne!(native.get("stream").and_then(Value::as_bool), Some(true));
    drop(state);

    assert_eq!(
        outcome
            .headers
            .get(http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("text/event-stream")
    );
    let ResponseBody::Stream(mut stream) = outcome.body else {
        panic!("synthesized stream expected")
    };
    let text = block_on(async {
        let mut bytes = Vec::new();
        while let Some(frame) = stream.next().await {
            bytes.extend_from_slice(&frame.expect("synthesized frame"));
        }
        String::from_utf8(bytes).expect("utf8")
    });
    assert!(text.contains("event: response.completed"), "{text}");
    assert!(text.contains("\"ok\""), "{text}");
}

#[test]
fn detached_synthesized_stream_opens_before_the_upstream_answers_and_settles() {
    for (status, expected) in [
        (http::StatusCode::OK, "event: response.completed"),
        (http::StatusCode::INTERNAL_SERVER_ERROR, "upstream_error"),
    ] {
        let host = MemoryHost::with_session_spawner();
        let core = core(&host).expect("core");
        let mut selected = target();
        selected.rules.routing = synthesizing_routing();
        {
            let mut state = host.state.lock().expect("state lock");
            state.plan = Some(plan(selected));
            state.statuses.push_back(status);
        }
        let outcome =
            block_on(core.execute(&host, rule_request(true, "detached"))).expect("execute");
        assert_eq!(outcome.status, http::StatusCode::OK);
        assert_eq!(
            outcome
                .headers
                .get(http::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some("text/event-stream")
        );
        let ResponseBody::Stream(mut stream) = outcome.body else {
            panic!("detached stream expected")
        };
        let text = block_on(async {
            let mut bytes = Vec::new();
            while let Some(frame) = stream.next().await {
                bytes.extend_from_slice(&frame.expect("synthesized frame"));
            }
            String::from_utf8(bytes).expect("utf8")
        });
        assert!(text.contains(expected), "{text}");
        let state = host.state.lock().expect("state lock");
        assert_eq!(state.settlements.len(), 1, "{text}");
    }
}

fn synthesizing_routing() -> Arc<[crate::routing::CompiledRoutingRule]> {
    Arc::from(
        crate::routing::compile_all(&[RoutingRuleSpec {
            id: 1,
            operation: "stream_generate_content".into(),
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

/// Magic markers are shaped after the process rules, in that order, because a rule
/// can put one into the body. Running the cache pass first leaves a rule-inserted
/// marker in the prompt as literal text and charges full price for it.
#[test]
fn magic_markers_a_rule_inserts_are_still_shaped() {
    let host = MemoryHost::new(false);
    let core = core(&host).expect("core");
    let mut selected = target();
    selected.provider.settings = json!({"enable_claude_magic_cache": true});
    selected.rules.process = Arc::from(
        crate::process::compile_all(&[spec(
            7,
            "transform",
            json!({
                "phase": "request",
                "locate": {"path":"messages.0.content.0.text"},
                "actions": [{"op":"replace_text","with":"prefix GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_49VA1S5V19GR4G89W2V695G9W9GV52W95V198WV5W2FC9DF"}]
            }),
            0,
        )])
        .expect("compiled transform"),
    );
    host.state.lock().expect("state lock").plan = Some(plan(selected));
    let mut request = request(false, "magic");
    request.path = "/v1/messages".into();
    request.body = Bytes::from(
        json!({"model":"alias","max_tokens":16,
               "messages":[{"role":"user","content":[{"type":"text","text":"plain"}]}]})
        .to_string(),
    );
    block_on(core.execute(&host, request)).expect("execute");
    let state = host.state.lock().expect("state lock");
    let native: Value = serde_json::from_slice(state.upstream_bodies.last().expect("request body"))
        .expect("native request json");
    let block = &native["messages"][0]["content"][0];
    assert_eq!(block["text"], "prefix");
    assert_eq!(block["cache_control"]["type"], "ephemeral");
}
