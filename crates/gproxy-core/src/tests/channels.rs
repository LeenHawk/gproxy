mod routes;

use bytes::Bytes;
use futures_util::StreamExt;
use gproxy_channel_api::{Channel, QuotaWindow};
use http::Method;
use rust_decimal::Decimal;
use serde_json::json;

use super::memory::MemoryHost;
use super::{block_on, request};
use crate::boundary::ResponseBody;
use crate::control::{FailoverBudget, Plan, ProviderRef, Target};
use crate::host::CredentialId;
use crate::{Core, InitError};

#[test]
fn transformed_claude_attempts_settle_native_usage_before_relay() -> Result<(), InitError> {
    for stream in [false, true] {
        let host = MemoryHost::new(false);
        host.state.lock().expect("state lock").credential.channel = "claudecode".into();
        host.state.lock().expect("state lock").plan = Some(Plan {
            targets: vec![claude_target()],
            budget: FailoverBudget { max_attempts: 1 },
        });
        let core = claude_core(&host)?;
        let mut request = request(
            false,
            if stream {
                "claude-stream"
            } else {
                "claude-buffer"
            },
        );
        request.path = "/v1/chat/completions".into();
        request.body = Bytes::from(format!(
            r#"{{"model":"alias","max_tokens":32,"stream":{stream},"messages":[{{"role":"user","content":"hi"}}]}}"#
        ));
        let outcome = block_on(core.execute(&host, request)).expect("transformed Claude attempt");
        match outcome.body {
            ResponseBody::Full(body) => {
                let body: serde_json::Value = serde_json::from_slice(&body).expect("chat response");
                assert_eq!(body["choices"][0]["message"]["content"], "ok");
            }
            ResponseBody::Stream(mut body) => {
                let bytes = block_on(async {
                    let mut bytes = Vec::new();
                    while let Some(chunk) = body.next().await {
                        bytes.extend_from_slice(&chunk.expect("stream frame"));
                    }
                    bytes
                });
                let text = String::from_utf8(bytes).expect("SSE text");
                assert!(text.contains("chat.completion.chunk"));
                assert!(text.contains("ok"));
            }
            ResponseBody::WebSocket(_) => panic!("HTTP transform returned a websocket"),
        }
        let state = host.state.lock().expect("state lock");
        assert_eq!(state.settlements.len(), 1);
        assert_eq!(state.settlements[0].provider_id, 4);
        assert_eq!(state.settlements[0].usage.input_tokens, 10);
        assert_eq!(state.settlements[0].usage.output_tokens, 5);
    }
    Ok(())
}

#[test]
fn claude_file_surface_funnels_create_and_lists_owned_binding() -> Result<(), InitError> {
    let host = MemoryHost::new(false);
    {
        let mut state = host.state.lock().expect("state lock");
        state.credential.channel = "claudecode".into();
        state.plan = Some(Plan {
            targets: vec![claude_target()],
            budget: FailoverBudget { max_attempts: 1 },
        });
    }
    let core = claude_core(&host)?;
    let mut create = request(false, "claude-file-create");
    create.path = "/v1/files".into();
    create.body = Bytes::new();
    let created = block_on(core.execute(&host, create)).expect("Claude file create");
    let ResponseBody::Full(created) = created.body else {
        panic!("Claude file create was not buffered");
    };
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&created).unwrap()["id"],
        "file-1"
    );
    assert!(
        host.state
            .lock()
            .expect("state lock")
            .bindings
            .contains_key(&(4, 1, "claude:file".into(), "file-1".into()))
    );

    let mut list = request(false, "claude-file-list");
    list.method = Method::GET;
    list.path = "/v1/files".into();
    list.body = Bytes::new();
    let listed = block_on(core.execute(&host, list)).expect("Claude file list");
    let ResponseBody::Full(listed) = listed.body else {
        panic!("Claude file list was not buffered");
    };
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&listed).unwrap()["data"][0]["id"],
        "file-1"
    );
    let state = host.state.lock().expect("state lock");
    assert_eq!(state.admission_finishes.len(), state.admit_calls);
    assert_eq!(state.captures.len(), 3);
    Ok(())
}

#[test]
fn codex_forced_stream_collects_or_relays_and_settles_terminal_usage() -> Result<(), InitError> {
    for stream in [false, true] {
        let host = MemoryHost::new(false);
        host.state.lock().expect("state lock").credential.channel = "codex".into();
        host.state.lock().expect("state lock").plan = Some(Plan {
            targets: vec![codex_target()],
            budget: FailoverBudget { max_attempts: 1 },
        });
        let core = codex_core(&host)?;
        let mut request = request(
            false,
            if stream {
                "codex-stream"
            } else {
                "codex-buffer"
            },
        );
        request.path = if stream {
            "/v1/responses".into()
        } else {
            "/api/codex/responses".into()
        };
        request.body = Bytes::from(format!(
            r#"{{"model":"alias","stream":{stream},"input":"hi","future_request":true}}"#
        ));
        let outcome = block_on(core.execute(&host, request)).expect("Codex response");
        match outcome.body {
            ResponseBody::Full(body) => {
                let body: serde_json::Value =
                    serde_json::from_slice(&body).expect("Responses body");
                assert_eq!(body["output_text"], "ok");
            }
            ResponseBody::Stream(mut body) => {
                let text = block_on(async {
                    let mut bytes = Vec::new();
                    while let Some(chunk) = body.next().await {
                        bytes.extend_from_slice(&chunk.expect("Codex stream frame"));
                    }
                    String::from_utf8(bytes).expect("Responses SSE")
                });
                assert!(text.contains("response.completed"));
                assert!(text.contains("ok"));
            }
            ResponseBody::WebSocket(_) => panic!("Codex HTTP returned a websocket"),
        }
        let state = host.state.lock().expect("state lock");
        assert_eq!(state.settlements.len(), 1);
        assert_eq!(state.settlements[0].usage.input_tokens, 10);
        assert_eq!(state.settlements[0].usage.output_tokens, 5);
        assert_eq!(
            state.settlements[0].usage.metrics["reasoning_tokens"],
            rust_decimal::Decimal::from(2)
        );
        if stream {
            assert!(state.captures[0].body.is_none());
        } else {
            assert!(state.captures[0].body.as_ref().is_some_and(|body| {
                String::from_utf8_lossy(body).contains("event: response.completed")
            }));
        }
    }
    Ok(())
}

#[test]
fn codex_usage_reports_selected_credential_cycles_and_reset_state() -> Result<(), InitError> {
    let host = MemoryHost::new(false);
    let before = unix_now();
    let primary_reset = before + 3_600;
    let secondary_reset = before + 7_200;
    {
        let mut state = host.state.lock().expect("state lock");
        state.credential.channel = "codex".into();
        state.plan = Some(Plan {
            targets: vec![codex_target()],
            budget: FailoverBudget { max_attempts: 1 },
        });
        state.quota_windows = vec![
            QuotaWindow {
                key: "primary".into(),
                period_start: Some(primary_reset - 18_000),
                reset_at: Some(primary_reset),
                used_percent: Some(Decimal::from(40)),
                upstream_used: Some(Decimal::from(4)),
                upstream_limit: Some(Decimal::from(10)),
            },
            QuotaWindow {
                key: "five-hour".into(),
                period_start: Some(primary_reset - 18_000),
                reset_at: Some(primary_reset),
                used_percent: Some(Decimal::from(125)),
                upstream_used: Some(Decimal::from(5)),
                upstream_limit: Some(Decimal::from(4)),
            },
            QuotaWindow {
                key: "secondary".into(),
                period_start: Some(secondary_reset - 604_800),
                reset_at: Some(secondary_reset),
                used_percent: None,
                upstream_used: None,
                upstream_limit: None,
            },
        ];
    }
    let core = codex_core(&host)?;
    let mut usage_request = request(false, "codex-usage");
    usage_request.method = Method::GET;
    usage_request.path = "/api/codex/usage".into();
    usage_request.body = Bytes::new();
    let outcome = block_on(core.execute(&host, usage_request)).expect("Codex usage");
    let ResponseBody::Full(body) = outcome.body else {
        panic!("Codex usage was not buffered");
    };
    let body: serde_json::Value = serde_json::from_slice(&body).expect("Codex usage JSON");
    let rate = &body["rate_limit"];
    assert_eq!(rate["allowed"], false);
    assert_eq!(rate["limit_reached"], true);
    assert_eq!(rate["primary_window"]["used_percent"], 100);
    assert_eq!(rate["primary_window"]["limit_window_seconds"], 18_000);
    assert_eq!(rate["primary_window"]["reset_at"], primary_reset);
    assert!(rate["secondary_window"].get("used_percent").is_none());
    assert_eq!(rate["secondary_window"]["reset_at"], secondary_reset);
    let reset_after = rate["primary_window"]["reset_after_seconds"]
        .as_i64()
        .expect("reset after seconds");
    assert!((primary_reset - unix_now()..=primary_reset - before).contains(&reset_after));
    assert_eq!(
        body["rate_limit_reached_type"],
        json!({"type":"rate_limit_reached"})
    );
    assert!(body.get("local_usage").is_some());
    for absent in ["credits", "spend_control", "additional_rate_limits"] {
        assert!(body.get(absent).is_none());
    }
    host.state.lock().expect("state lock").quota_windows.clear();
    let mut request = request(false, "codex-usage-absent");
    request.method = Method::GET;
    request.path = "/api/codex/usage".into();
    request.body = Bytes::new();
    let outcome = block_on(core.execute(&host, request)).expect("Codex usage without cycles");
    let ResponseBody::Full(body) = outcome.body else {
        panic!("Codex usage without cycles was not buffered");
    };
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(body.get("rate_limit").is_none());
    assert!(body.get("rate_limit_reached_type").is_none());
    Ok(())
}

#[test]
fn codex_sparse_text_repairs_before_chat_and_claude_transforms() -> Result<(), InitError> {
    for (path, body, marker) in [
        (
            "/v1/chat/completions",
            r#"{"model":"alias","stream":true,"messages":[{"role":"user","content":"sparse_test"}]}"#,
            "\"content\":\"sparse_test\"",
        ),
        (
            "/v1/messages",
            r#"{"model":"alias","max_tokens":32,"stream":true,"messages":[{"role":"user","content":"sparse_test"}]}"#,
            "\"text\":\"sparse_test\"",
        ),
    ] {
        let host = MemoryHost::new(false);
        host.state.lock().expect("state lock").credential.channel = "codex".into();
        host.state.lock().expect("state lock").plan = Some(Plan {
            targets: vec![codex_target()],
            budget: FailoverBudget { max_attempts: 1 },
        });
        let core = codex_core(&host)?;
        let mut request = request(false, "codex-sparse-transform");
        request.path = path.into();
        request.body = Bytes::from(body);
        let outcome = block_on(core.execute(&host, request)).expect("sparse Codex response");
        let ResponseBody::Stream(mut stream) = outcome.body else {
            panic!("sparse Codex response was not streaming");
        };
        let text = block_on(async {
            let mut bytes = Vec::new();
            while let Some(frame) = stream.next().await {
                bytes.extend_from_slice(&frame.expect("sparse transformed frame"));
            }
            String::from_utf8(bytes).expect("sparse transformed UTF-8")
        });
        assert_eq!(text.matches(marker).count(), 1, "{text}");
    }

    let host = MemoryHost::new(false);
    host.state.lock().expect("state lock").credential.channel = "codex".into();
    host.state.lock().expect("state lock").plan = Some(Plan {
        targets: vec![codex_target()],
        budget: FailoverBudget { max_attempts: 1 },
    });
    let core = codex_core(&host)?;
    let mut request = request(false, "codex-sparse-buffered");
    request.path = "/v1/chat/completions".into();
    request.body = Bytes::from_static(
        br#"{"model":"alias","stream":false,"messages":[{"role":"user","content":"sparse_test"}]}"#,
    );
    let outcome = block_on(core.execute(&host, request)).expect("buffered sparse Codex response");
    let ResponseBody::Full(body) = outcome.body else {
        panic!("buffered sparse Codex response was not collected");
    };
    let body: serde_json::Value = serde_json::from_slice(&body).expect("buffered Chat response");
    assert_eq!(body["choices"][0]["message"]["content"], "sparse_test");
    Ok(())
}

#[test]
fn codex_private_model_catalog_shapes_to_public_models_without_losing_rest() -> Result<(), InitError>
{
    let host = MemoryHost::new(false);
    host.state.lock().expect("state lock").credential.channel = "codex".into();
    let core = codex_core(&host)?;
    let mut request = request(false, "codex-models");
    request.method = Method::GET;
    request.path = "/v1/models".into();
    request.body = Bytes::new();
    let outcome = block_on(core.invoke(&host, &codex_target(), request)).expect("Codex models");
    let ResponseBody::Full(body) = outcome.body else {
        panic!("Codex models were not buffered");
    };
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["data"][0]["id"], "gpt-test");
    assert_eq!(body["data"][0]["context_window"], 256000);
    assert_eq!(body["data"][0]["future_catalog"], "kept");
    assert_eq!(body["future_list"], "kept");
    Ok(())
}

#[test]
fn codex_memory_alias_resolves_admits_and_settles_estimated_usage() -> Result<(), InitError> {
    let host = MemoryHost::new(false);
    {
        let mut state = host.state.lock().expect("state lock");
        state.credential.channel = "codex".into();
        state.plan = Some(Plan {
            targets: vec![codex_target()],
            budget: FailoverBudget { max_attempts: 1 },
        });
    }
    let core = codex_core(&host)?;
    let mut request = request(false, "codex-memory-summary");
    request.path = "/api/codex/memories/trace_summarize".into();
    request.body = Bytes::from_static(
        br#"{"model":"alias","traces":[{"id":"trace-1","metadata":{"source_path":"/tmp/trace.jsonl"},"items":[{"type":"message","content":"remember this"}]}]}"#,
    );
    let outcome = block_on(core.execute(&host, request)).expect("Codex memory summary");
    let ResponseBody::Full(body) = outcome.body else {
        panic!("Codex memory summary was not buffered");
    };
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&body).unwrap()["result"],
        "ok"
    );

    let state = host.state.lock().expect("state lock");
    assert_eq!(state.resolved_models, [Some("alias".into())]);
    assert_eq!(state.admit_calls, 1);
    assert_eq!(state.admission_finishes, [true]);
    assert_eq!(state.authorizations.len(), 1);
    assert_eq!(state.settlements.len(), 1);
    assert_eq!(
        state.settlements[0].source,
        crate::usage::UsageSource::Estimated
    );
    assert!(state.settlements[0].usage.input_tokens > 0);
    assert!(state.settlements[0].usage.output_tokens > 0);
    Ok(())
}

fn claude_core(host: &MemoryHost) -> Result<Core<MemoryHost>, InitError> {
    let channels =
        gproxy_channel_api::ChannelRegistry::new([
            Box::new(gproxy_channels::ClaudeCodeChannel) as Box<dyn Channel>
        ])
        .expect("channel registry");
    Core::new(host.clone(), channels)
}

fn claude_target() -> Target {
    Target {
        provider: ProviderRef {
            id: 4,
            name: "claude-provider".into(),
            channel: "claudecode".into(),
            settings: json!({}),
            fingerprint: None,
            proxy_url: None,
            traffic_blacklist: Default::default(),
        },
        credential: CredentialId(7),
        upstream_model: "claude-test".into(),
        tier: 0,
        rules: Default::default(),
    }
}

fn codex_core(host: &MemoryHost) -> Result<Core<MemoryHost>, InitError> {
    let channels = gproxy_channel_api::ChannelRegistry::new([
        Box::new(gproxy_channels::CodexChannel) as Box<dyn Channel>,
    ])
    .expect("channel registry");
    Core::new(host.clone(), channels)
}

fn codex_target() -> Target {
    Target {
        provider: ProviderRef {
            id: 5,
            name: "codex-provider".into(),
            channel: "codex".into(),
            settings: json!({}),
            fingerprint: None,
            proxy_url: None,
            traffic_blacklist: Default::default(),
        },
        credential: CredentialId(7),
        upstream_model: "gpt-test".into(),
        tier: 0,
        rules: Default::default(),
    }
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock")
        .as_secs()
        .try_into()
        .expect("Unix seconds fit in i64")
}
