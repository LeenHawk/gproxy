use gproxy_channel::BaseChannel;
use gproxy_core::{
    CoreData, CredentialStrategy, ProviderData, RewriteRuleData, RewriteRuleSetData,
    rewrite::{
        Phase, RewriteContext, StreamRewriter, apply_body, apply_headers, apply_query,
        compile_rule, select_rules,
    },
};
use gproxy_protocol::{Dialect, Operation, OperationKey, connection::StreamFraming};
use gproxy_store::entity::upstream::{
    provider, provider_rewrite_rule_set, rewrite_rule, rewrite_rule_set,
};
use http::{HeaderMap, HeaderValue};
use serde_json::json;
use std::sync::Arc;

struct Rule {
    target: rewrite_rule::RewriteTarget,
    name: Option<&'static str>,
    paths: Option<serde_json::Value>,
    pattern: &'static str,
    replacement: &'static str,
    phase: &'static str,
    event: Option<&'static str>,
    model: Option<&'static str>,
    header: Option<&'static str>,
    operations: Option<serde_json::Value>,
}
impl Default for Rule {
    fn default() -> Self {
        Self {
            target: rewrite_rule::RewriteTarget::Body,
            name: None,
            paths: None,
            pattern: "x",
            replacement: "y",
            phase: "both",
            event: None,
            model: None,
            header: None,
            operations: None,
        }
    }
}
fn rule(id: &str, spec: Rule) -> Arc<RewriteRuleData> {
    Arc::new(
        compile_rule(Arc::new(rewrite_rule::Model {
            action: "replace".into(),
            id: id.into(),
            rule_set_id: "set".into(),
            phase: spec.phase.into(),
            target: spec.target,
            target_name: spec.name.map(str::to_owned),
            paths: spec.paths,
            pattern: spec.pattern.into(),
            replacement: spec.replacement.into(),
            filter_operation_keys: spec.operations,
            filter_model_pattern: spec.model.map(str::to_owned),
            filter_header_pattern: spec.header.map(str::to_owned),
            filter_event_pattern: spec.event.map(str::to_owned),
            sort_order: 0,
            enabled: true,
            created_at_ms: 0,
            updated_at_ms: 0,
        }))
        .unwrap(),
    )
}
fn body(
    paths: Option<serde_json::Value>,
    pattern: &'static str,
    replacement: &'static str,
) -> Rule {
    Rule {
        paths,
        pattern,
        replacement,
        ..Rule::default()
    }
}

#[test]
fn path_rules_select_strings_in_order_and_untouched_bodies_stay_identical() {
    let first = rule(
        "1",
        body(
            Some(json!(["tools.*.name", "tool_choice.name"])),
            "^mcp_(.*)$",
            "mcp__$1",
        ),
    );
    let second = rule("2", body(Some(json!(["tools.*.name"])), "__", "::"));
    let text_rule = rule("3", body(None, "\"temperature\":1", "\"temperature\":0"));
    let rules = [first, second, text_rule];
    let input = json!({"tools":[{"name":"mcp_a"},{"name":"plain"},{"name":42}],"tool_choice":{"name":"mcp_b"},"temperature":1});
    let out = apply_body(&rules, &serde_json::to_vec(&input).unwrap())
        .unwrap()
        .unwrap();
    let out: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(out["tools"][0]["name"], "mcp::a");
    assert_eq!(out["tools"][1]["name"], "plain");
    assert_eq!(out["tools"][2]["name"], 42);
    assert_eq!(
        out["tool_choice"]["name"], "mcp__b",
        "second rule only touches tools.*.name"
    );
    assert_eq!(out["temperature"], 0);

    let untouched = br#"{ "spaced" : "body", "tools": [] }"#;
    assert!(apply_body(&rules, untouched).unwrap().is_none());
    let formatted = "{ \"z\" : 1.50 ,\n  \"tools\" : [ { \"name\" : \"mcp_x\\u0041\" , \"n\":1e3 } ] , \"temperature\":1 }";
    let out = apply_body(&rules, formatted.as_bytes()).unwrap().unwrap();
    assert_eq!(
        String::from_utf8(out).unwrap(),
        "{ \"z\" : 1.50 ,\n  \"tools\" : [ { \"name\" : \"mcp::xA\" , \"n\":1e3 } ] , \"temperature\":0 }",
        "only selected strings change; order, spacing and numbers are untouched"
    );
    assert!(apply_body(&rules, b"not json at all").unwrap().is_none());
    assert!(apply_body(&rules, &[0xff, 0xfe]).is_err());
}

#[test]
fn header_rules_keep_repeated_values_and_reject_injection() {
    let tag = rule(
        "h",
        Rule {
            target: rewrite_rule::RewriteTarget::Header,
            name: Some("X-Client-Tag"),
            pattern: "^old$",
            replacement: "new",
            ..Rule::default()
        },
    );
    let mut headers = HeaderMap::new();
    headers.append("x-client-tag", HeaderValue::from_static("old"));
    headers.append("x-client-tag", HeaderValue::from_static("keep"));
    headers.append("x-client-tag", HeaderValue::from_static("old"));
    headers.insert("other", HeaderValue::from_static("old"));
    assert!(apply_headers(std::slice::from_ref(&tag), &mut headers).unwrap());
    let values: Vec<_> = headers
        .get_all("x-client-tag")
        .iter()
        .map(|v| v.to_str().unwrap())
        .collect();
    assert_eq!(values, ["new", "keep", "new"]);
    assert_eq!(headers["other"], "old");
    assert!(!apply_headers(std::slice::from_ref(&tag), &mut headers).unwrap());

    let inject = rule(
        "i",
        Rule {
            target: rewrite_rule::RewriteTarget::Header,
            name: Some("x-client-tag"),
            pattern: "new",
            replacement: "a\r\nevil: b",
            ..Rule::default()
        },
    );
    assert!(apply_headers(std::slice::from_ref(&inject), &mut headers).is_err());
    let mut binary = HeaderMap::new();
    binary.insert("x-client-tag", HeaderValue::from_bytes(&[0xff]).unwrap());
    assert!(apply_headers(std::slice::from_ref(&tag), &mut binary).is_err());
}

#[test]
fn query_rules_preserve_order_repeats_and_raw_untouched_segments() {
    let version = rule(
        "q",
        Rule {
            target: rewrite_rule::RewriteTarget::Query,
            name: Some("api-version"),
            pattern: "^preview$",
            replacement: "stable & sound",
            phase: "request",
            ..Rule::default()
        },
    );
    let query = "b=%2A&api-version=preview&flag&api%2Dversion=preview&a=1+2&api-version=ga";
    let out = apply_query(std::slice::from_ref(&version), Some(query))
        .unwrap()
        .unwrap();
    assert_eq!(
        out,
        "b=%2A&api-version=stable+%26+sound&flag&api%2Dversion=stable+%26+sound&a=1+2&api-version=ga"
    );
    assert!(
        apply_query(std::slice::from_ref(&version), Some("api-version=ga"))
            .unwrap()
            .is_none()
    );
    assert!(
        apply_query(std::slice::from_ref(&version), None)
            .unwrap()
            .is_none()
    );
}

#[test]
fn sse_units_keep_metadata_comments_done_and_split_frames() {
    let greeting = rule(
        "s",
        Rule {
            pattern: "hello",
            replacement: "bye",
            event: Some("^message$"),
            ..Rule::default()
        },
    );
    let typed = rule(
        "t",
        Rule {
            paths: Some(json!(["delta.text"])),
            pattern: "secret",
            replacement: "[redacted]",
            event: Some("content_block_delta"),
            ..Rule::default()
        },
    );
    let mut rewriter =
        StreamRewriter::new(vec![greeting.clone(), typed], StreamFraming::Sse, 1 << 20);
    let stream = concat!(
        ": keep-alive\n\n",
        "id: 7\nretry: 500\nevent: message\ndata: hello\ndata: hello again\n\n",
        "event: other\ndata: hello\n\n",
        "data: {\"type\":\"content_block_delta\",\"delta\":{\"text\":\"my secret\"}}\n\n",
        "data: [DONE]\n\n",
        "data: trailing hello"
    );
    let (head, tail) = stream.split_at(30);
    let mut out = rewriter.push(head.as_bytes()).unwrap();
    out.extend(rewriter.push(tail.as_bytes()).unwrap());
    out.extend(rewriter.finish().unwrap());
    assert_eq!(
        String::from_utf8(out).unwrap(),
        concat!(
            ": keep-alive\n\n",
            "id: 7\nretry: 500\nevent: message\ndata: bye\ndata: bye again\n\n",
            "event: other\ndata: hello\n\n",
            "data: {\"type\":\"content_block_delta\",\"delta\":{\"text\":\"my [redacted]\"}}\n\n",
            "data: [DONE]\n\n",
            "data: trailing hello"
        )
    );
    let mut crlf = StreamRewriter::new(vec![greeting.clone()], StreamFraming::Sse, 1 << 20);
    let out = crlf.push(b"event: message\r\ndata: hello\r\n\r\n").unwrap();
    assert_eq!(out, b"event: message\r\ndata: bye\r\n\r\n");
    let mut tiny = StreamRewriter::new(vec![greeting], StreamFraming::Sse, 8);
    assert!(tiny.push(b"data: this frame never ends").is_err());
}

#[test]
fn json_array_and_ndjson_units_are_rewritten_individually() {
    let redact = rule("r", body(Some(json!(["text"])), "secret", "***"));
    let mut array = StreamRewriter::new(vec![redact.clone()], StreamFraming::JsonArray, 1 << 20);
    let mut out = array
        .push(b"[ {\"text\":\"a secret\", \"k\":\"]\"}")
        .unwrap();
    out.extend(
        array
            .push(b" ,\n{\"text\":\"plain\"},\n7, \"str,\" ]tail")
            .unwrap(),
    );
    out.extend(array.finish().unwrap());
    assert_eq!(
        String::from_utf8(out).unwrap(),
        "[{\"text\":\"a ***\", \"k\":\"]\"},{\"text\":\"plain\"},7,\"str,\"]tail"
    );

    let mut ndjson = StreamRewriter::new(vec![redact], StreamFraming::NdJson, 1 << 20);
    let mut out = ndjson
        .push(b"{\"text\":\"secret\"}\r\n{\"text\":\"ok\"}\n\n{\"te")
        .unwrap();
    out.extend(ndjson.push(b"xt\":\"secret\"}\n").unwrap());
    out.extend(ndjson.finish().unwrap());
    assert_eq!(
        String::from_utf8(out).unwrap(),
        "{\"text\":\"***\"}\r\n{\"text\":\"ok\"}\n\n{\"text\":\"***\"}\n"
    );
}

struct Channel;
impl BaseChannel for Channel {
    fn id(&self) -> &'static str {
        "test"
    }
}

fn snapshot(rules: Vec<Arc<RewriteRuleData>>) -> (CoreData, ProviderData) {
    let set = RewriteRuleSetData {
        entity: Arc::new(rewrite_rule_set::Model {
            id: "set".into(),
            name: "set".into(),
            description: None,
            enabled: true,
            created_at_ms: 0,
            updated_at_ms: 0,
        }),
        rules,
    };
    let mut data = CoreData::default();
    data.rewrite_rule_sets.insert("set".into(), Arc::new(set));
    let provider = ProviderData {
        entity: Arc::new(provider::Model {
            id: "p".into(),
            name: "p".into(),
            display_name: None,
            channel: "test".into(),
            base_url: None,
            connection_profile_id: None,
            proxy: None,
            config: json!({}),
            enabled: true,
            created_at_ms: 0,
        }),
        channel: Arc::new(Channel),
        credential_ids: vec![],
        models: vec![],
        operation_rules: vec![],
        operation_urls: Default::default(),
        rewrite_rule_sets: vec![Arc::new(provider_rewrite_rule_set::Model {
            id: "a".into(),
            provider_id: "p".into(),
            rule_set_id: "set".into(),
            sort_order: 0,
            enabled: true,
            created_at_ms: 0,
            updated_at_ms: 0,
        })],
        credential_strategy: CredentialStrategy::RoundRobin,
        session_affinity: false,
    };
    (data, provider)
}

#[test]
fn selection_filters_by_phase_operation_model_and_inbound_headers() {
    let (data, provider) = snapshot(vec![
        rule(
            "req",
            Rule {
                phase: "request",
                ..Rule::default()
            },
        ),
        rule(
            "resp",
            Rule {
                phase: "response",
                ..Rule::default()
            },
        ),
        rule(
            "op",
            Rule {
                operations: Some(json!([{"operation": "generate_content", "dialect": "claude"}])),
                ..Rule::default()
            },
        ),
        rule(
            "model",
            Rule {
                model: Some("claude-*"),
                ..Rule::default()
            },
        ),
        rule(
            "hdr",
            Rule {
                header: Some("^x-debug: on$"),
                ..Rule::default()
            },
        ),
        rule(
            "q",
            Rule {
                target: rewrite_rule::RewriteTarget::Query,
                name: Some("v"),
                phase: "request",
                ..Rule::default()
            },
        ),
    ]);
    let mut headers = HeaderMap::new();
    headers.insert("X-Debug", HeaderValue::from_static("ON"));
    let ids = |selected: &gproxy_core::rewrite::SelectedRules| {
        selected
            .body
            .iter()
            .chain(&selected.headers)
            .chain(&selected.query)
            .map(|r| r.entity.id.clone())
            .collect::<Vec<_>>()
    };
    let claude = RewriteContext {
        operation: OperationKey {
            operation: Operation::GenerateContent,
            dialect: Dialect::Claude,
        },
        upstream_model: Some("claude-sonnet-4"),
        requested_model: None,
        request_headers: &headers,
    };
    assert_eq!(
        ids(&select_rules(&data, &provider, Phase::Request, &claude)),
        ["req", "op", "model", "hdr", "q"]
    );
    assert_eq!(
        ids(&select_rules(&data, &provider, Phase::Response, &claude)),
        ["resp", "op", "model", "hdr"]
    );
    let gemini = RewriteContext {
        operation: OperationKey {
            operation: Operation::ListModels,
            dialect: Dialect::Gemini,
        },
        upstream_model: Some("gemini-2.5"),
        requested_model: Some("claude-alias"),
        request_headers: &HeaderMap::new(),
    };
    assert_eq!(
        ids(&select_rules(&data, &provider, Phase::Response, &gemini)),
        ["resp", "model"]
    );
    assert!(
        !select_rules(
            &data,
            &provider,
            Phase::Response,
            &RewriteContext {
                requested_model: None,
                upstream_model: Some("gpt"),
                ..gemini
            }
        )
        .is_empty()
    );
}
