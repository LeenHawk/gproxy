mod bindings;
mod channel;
mod memory;
mod services;
mod surface;
mod surface_engine;
mod surface_harness;

use std::future::Future;
use std::task::{Context, Poll, Waker};

use bytes::Bytes;
use futures_util::StreamExt;
use gproxy_channel_api::{Channel, Disposition};
use http::{HeaderMap, Method, StatusCode};
use rust_decimal::Decimal;
use serde_json::json;

use self::memory::MemoryHost;
use crate::boundary::{RequestCtx, ResponseBody, RoutingMode};
use crate::control::{FailoverBudget, Plan};
use crate::control::{ProviderRef, Target};
use crate::error::CoreError;
use crate::host::CredentialId;
use crate::usage::{Ended, UsageSource};
use crate::{Core, InitError};

#[test]
fn invoke_refreshes_with_version_guard_and_finishes_the_funnel() -> Result<(), InitError> {
    for (conflict, expected_token) in [(false, "Bearer fresh"), (true, "Bearer peer")] {
        let host = MemoryHost::new(conflict);
        let core = core(&host)?;
        let outcome =
            block_on(core.invoke(&host, &target(), request(false, &conflict.to_string())))
                .expect("invoke");
        assert_eq!(outcome.status, StatusCode::OK);
        assert_eq!(outcome.disposition, Disposition::Success);
        assert!(matches!(outcome.body, ResponseBody::Full(_)));

        let state = host.state.lock().expect("state lock");
        assert_eq!(state.lease_calls, 1);
        assert_eq!(state.rotations, [4]);
        assert_eq!(state.credential.version, 5);
        assert_eq!(state.authorizations, [expected_token]);
        assert_eq!(state.settlements.len(), 1);
        let settlement = &state.settlements[0];
        assert_eq!(settlement.usage.input_tokens, 10);
        assert_eq!(settlement.usage.output_tokens, 5);
        assert_eq!(settlement.cost, Decimal::new(2, 5));
        assert_eq!(settlement.source, UsageSource::Upstream);
        assert_eq!(settlement.ended, Ended::Complete);
        assert_eq!(state.captures.len(), 1);
        assert_eq!(state.captures[0].0, Some(StatusCode::OK));
        assert!(state.captures[0].1.is_some());
    }
    Ok(())
}

#[test]
fn streaming_invoke_settles_inline_before_eof_without_a_spawner() -> Result<(), InitError> {
    let host = MemoryHost::new(false);
    let core = core(&host)?;
    let outcome = block_on(core.invoke(&host, &target(), request(true, "stream"))).expect("invoke");
    let ResponseBody::Stream(mut stream) = outcome.body else {
        panic!("streaming request returned a buffered body");
    };
    block_on(async {
        assert!(stream.next().await.expect("body frame").is_ok());
        assert_eq!(
            stream
                .next()
                .await
                .expect("decoder tail")
                .expect("tail frame"),
            Bytes::from_static(b"tail")
        );
        assert!(stream.next().await.is_none());
    });

    let state = host.state.lock().expect("state lock");
    assert_eq!(state.settlements.len(), 1);
    assert_eq!(state.settlements[0].ended, Ended::Complete);
    assert_eq!(state.captures.len(), 1);
    Ok(())
}

#[test]
fn media_stream_detection_covers_json_values_and_multipart_flags() {
    let cases = [
        (
            "/v1/audio/speech",
            "application/json",
            Bytes::from_static(br#"{"stream_format":"sse"}"#),
        ),
        (
            "/v1/audio/transcriptions",
            "multipart/form-data; boundary=x",
            Bytes::from_static(
                b"--x\r\nContent-Disposition: form-data; name=\"stream\"\r\n\r\ntrue\r\n--x--\r\n",
            ),
        ),
        (
            "/v1/images/edits",
            "multipart/form-data; boundary=x",
            Bytes::from_static(
                b"--x\r\nContent-Disposition: form-data; name=\"stream\"\r\n\r\nTRUE\r\n--x--\r\n",
            ),
        ),
    ];
    for (path, content_type, body) in cases {
        let mut request = request(false, path);
        request.path = path.into();
        request.body = body;
        request.headers.insert(
            http::header::CONTENT_TYPE,
            content_type.parse().expect("content type"),
        );
        assert!(
            crate::request::classify(&request)
                .expect("classified")
                .stream
        );
    }

    let mut models = request(false, "claude-models");
    models.method = Method::GET;
    models.path = "/v1/models".into();
    models.body = Bytes::new();
    models
        .headers
        .insert("anthropic-version", "2023-06-01".parse().unwrap());
    assert_eq!(
        crate::request::classify(&models)
            .expect("Claude models")
            .key
            .kind,
        gproxy_protocol::OperationKind::Family(gproxy_protocol::WireFamily::Claude)
    );
}

#[test]
fn resource_operations_persist_pin_and_delete_credential_bindings() -> Result<(), InitError> {
    let host = MemoryHost::new(false);
    host.state.lock().expect("state lock").plan = Some(Plan {
        targets: vec![target()],
        budget: FailoverBudget { max_attempts: 1 },
    });
    let core = core(&host)?;
    let mut create = request(false, "file-create");
    create.path = "/v1/files".into();
    create.body = Bytes::new();
    block_on(core.execute(&host, create)).expect("create file");
    assert!(
        host.state
            .lock()
            .expect("state lock")
            .bindings
            .contains_key(&(3, 1, "file".into(), "file-1".into()))
    );

    let mut other = target();
    other.credential = CredentialId(8);
    host.state.lock().expect("state lock").plan = Some(Plan {
        targets: vec![other, target()],
        budget: FailoverBudget { max_attempts: 1 },
    });
    let mut retrieve = request(false, "file-retrieve");
    retrieve.method = Method::GET;
    retrieve.path = "/v1/files/file-1".into();
    retrieve.body = Bytes::new();
    block_on(core.execute(&host, retrieve)).expect("retrieve bound file");
    assert_eq!(
        host.state
            .lock()
            .expect("state lock")
            .loaded_credentials
            .last(),
        Some(&CredentialId(7))
    );

    let mut delete = request(false, "file-delete");
    delete.method = Method::DELETE;
    delete.path = "/v1/files/file-1".into();
    delete.body = Bytes::new();
    block_on(core.execute(&host, delete)).expect("delete bound file");
    assert!(
        !host
            .state
            .lock()
            .expect("state lock")
            .bindings
            .contains_key(&(3, 1, "file".into(), "file-1".into()))
    );
    Ok(())
}

#[test]
fn foreign_shared_surface_falls_through_without_reauthenticating() -> Result<(), InitError> {
    let host = MemoryHost::new(false);
    host.state.lock().expect("state lock").plan = Some(Plan {
        targets: vec![target()],
        budget: FailoverBudget { max_attempts: 1 },
    });
    let channels = gproxy_channel_api::ChannelRegistry::new([
        Box::new(host.clone()) as Box<dyn Channel>,
        Box::new(channel::ForeignSurface) as Box<dyn Channel>,
    ])
    .expect("channel registry");
    let core = Core::new(host.clone(), channels)?;
    let mut files = request(false, "shared-files");
    files.method = Method::GET;
    files.path = "/v1/files".into();
    files.body = Bytes::new();
    assert_eq!(
        block_on(core.execute(&host, files))
            .expect("OpenAI files fallthrough")
            .status,
        StatusCode::OK
    );
    let state = host.state.lock().expect("state lock");
    assert_eq!(state.auth_calls, 1);
    assert_eq!(state.admit_calls, 1);
    Ok(())
}

#[test]
fn transformed_claude_attempts_settle_native_usage_before_relay() -> Result<(), InitError> {
    for stream in [false, true] {
        let host = MemoryHost::new(false);
        host.state.lock().expect("state lock").credential.channel = "claudecode".into();
        let target = Target {
            provider: ProviderRef {
                id: 4,
                name: "claude-provider".into(),
                channel: "claudecode".into(),
                settings: json!({}),
            },
            credential: CredentialId(7),
            upstream_model: "claude-test".into(),
        };
        host.state.lock().expect("state lock").plan = Some(Plan {
            targets: vec![target],
            budget: FailoverBudget { max_attempts: 1 },
        });
        let channels =
            gproxy_channel_api::ChannelRegistry::new([
                Box::new(gproxy_channels::ClaudeCodeChannel) as Box<dyn Channel>,
            ])
            .expect("channel registry");
        let core = Core::new(host.clone(), channels)?;
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
fn every_declared_builtin_transform_is_wired() {
    for channel in [
        &gproxy_channels::OpenAiChannel as &dyn Channel,
        &gproxy_channels::ClaudeCodeChannel as &dyn Channel,
    ] {
        for support in channel.descriptor().supports {
            if support.source != support.target {
                assert!(
                    gproxy_transform::can_transform(support.source, support.target),
                    "{} declares an unwired transform: {:?}",
                    channel.descriptor().id,
                    support
                );
            }
        }
    }
}

#[test]
fn claude_file_surface_funnels_create_and_lists_owned_binding() -> Result<(), InitError> {
    let host = MemoryHost::new(false);
    {
        let mut state = host.state.lock().expect("state lock");
        state.credential.channel = "claudecode".into();
        state.plan = Some(Plan {
            targets: vec![Target {
                provider: ProviderRef {
                    id: 4,
                    name: "claude-provider".into(),
                    channel: "claudecode".into(),
                    settings: json!({}),
                },
                credential: CredentialId(7),
                upstream_model: "claude-test".into(),
            }],
            budget: FailoverBudget { max_attempts: 1 },
        });
    }
    let channels =
        gproxy_channel_api::ChannelRegistry::new([
            Box::new(gproxy_channels::ClaudeCodeChannel) as Box<dyn Channel>
        ])
        .expect("channel registry");
    let core = Core::new(host.clone(), channels)?;
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
fn execute_honors_failover_budget_and_settles_only_the_final_attempt() -> Result<(), InitError> {
    for (budget, succeeds) in [(2, true), (1, false)] {
        let host = MemoryHost::new(false);
        {
            let mut state = host.state.lock().expect("state lock");
            state.plan = Some(Plan {
                targets: vec![target(), target(), target()],
                budget: FailoverBudget {
                    max_attempts: budget,
                },
            });
            state.statuses = [StatusCode::TOO_MANY_REQUESTS, StatusCode::OK].into();
        }
        let core = core(&host)?;
        let result = block_on(core.execute(&host, request(false, &format!("budget-{budget}"))));
        if succeeds {
            assert_eq!(
                result.expect("second attempt succeeds").status,
                StatusCode::OK
            );
        } else {
            assert!(matches!(result, Err(CoreError::UpstreamExhausted(_))));
        }

        let state = host.state.lock().expect("state lock");
        assert_eq!(state.auth_calls, 1);
        assert_eq!(state.admit_calls, 1);
        assert_eq!(state.resolved_models, [Some("alias".into())]);
        assert_eq!(state.authorizations.len(), budget as usize);
        assert_eq!(state.captures.len(), budget as usize);
        assert_eq!(state.settlements.len(), usize::from(succeeds));
        assert_eq!(state.admission_finishes, [succeeds]);
    }

    let host = MemoryHost::new(false);
    {
        let mut state = host.state.lock().expect("state lock");
        state.plan = Some(Plan {
            targets: vec![target(), target(), target()],
            budget: FailoverBudget { max_attempts: 3 },
        });
        state.statuses = [StatusCode::UNAUTHORIZED, StatusCode::OK].into();
    }
    let core = core(&host)?;
    let result = block_on(core.execute(&host, request(false, "credential-dead")));
    assert!(matches!(result, Err(CoreError::UpstreamExhausted(_))));
    let state = host.state.lock().expect("state lock");
    assert_eq!(state.authorizations.len(), 1);
    assert_eq!(state.captures.len(), 1);
    assert!(state.settlements.is_empty());
    assert_eq!(state.admission_finishes, [false]);
    Ok(())
}

fn core(host: &MemoryHost) -> Result<Core<MemoryHost>, InitError> {
    let channels =
        gproxy_channel_api::ChannelRegistry::new([Box::new(host.clone()) as Box<dyn Channel>])
            .expect("channel registry");
    Core::new(host.clone(), channels)
}

fn target() -> Target {
    Target {
        provider: ProviderRef {
            id: 3,
            name: "provider".into(),
            channel: "memory".into(),
            settings: json!({}),
        },
        credential: CredentialId(7),
        upstream_model: "upstream-model".into(),
    }
}

fn request(stream: bool, id: &str) -> RequestCtx {
    RequestCtx {
        request_id: format!("request-{id}"),
        method: Method::POST,
        path: "/v1/responses".into(),
        query: None,
        headers: HeaderMap::new(),
        body: Bytes::from(format!(r#"{{"model":"alias","stream":{stream}}}"#)),
        upgrade: false,
        mode: RoutingMode::Aggregated,
    }
}

fn block_on<F: Future>(future: F) -> F::Output {
    let mut context = Context::from_waker(Waker::noop());
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => return output,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}
