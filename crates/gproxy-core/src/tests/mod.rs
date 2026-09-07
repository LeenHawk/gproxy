mod bindings;
mod channel;
mod channel_session;
mod channels;
mod credential_budget;
mod memory;
mod model_metadata;
mod models;
mod orchestration;
mod pricing;
mod process;
mod realtime;
mod refusal;
mod services;
mod session_affinity;
mod surface;
mod surface_engine;
mod surface_harness;
mod traffic;
mod websocket;

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
use crate::control::{FailoverBudget, Plan, ProviderRef, Target};
use crate::error::CoreError;
use crate::host::{CredentialHealth, CredentialId};
use crate::usage::{Ended, UsageSource};
use crate::{Core, InitError};

#[test]
fn invoke_refreshes_with_version_guard_and_finishes_the_funnel() -> Result<(), InitError> {
    for (conflict, lease_loser, expected_token, rotations) in [
        (false, false, "Bearer fresh", &[4][..]),
        (true, false, "Bearer peer", &[4][..]),
        (false, true, "Bearer peer", &[][..]),
    ] {
        let host = MemoryHost::new(conflict);
        host.state.lock().expect("state lock").peer_refresh_on_wait = lease_loser;
        let core = core(&host)?;
        let id = format!("{conflict}-{lease_loser}");
        let outcome = block_on(core.invoke(&host, &target(), request(false, &id))).expect("invoke");
        assert_eq!(outcome.status, StatusCode::OK);
        assert_eq!(outcome.disposition, Disposition::Success);
        assert!(matches!(outcome.body, ResponseBody::Full(_)));

        let state = host.state.lock().expect("state lock");
        assert_eq!(state.lease_calls, 1);
        assert_eq!(state.wait_calls, usize::from(lease_loser));
        assert_eq!(state.rotations, rotations);
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
        assert_eq!(state.captures[0].status, Some(StatusCode::OK));
        assert!(state.captures[0].body.is_some());
        assert_eq!(state.captures[0].provider_id, Some(3));
        assert_eq!(state.captures[0].credential_id, Some(CredentialId(7)));
        assert_eq!(
            state.health.last(),
            Some(&(
                CredentialId(7),
                "upstream-model".into(),
                CredentialHealth::Healthy
            ))
        );
    }
    Ok(())
}

#[test]
fn configured_fingerprint_overrides_headers_and_fails_loudly() -> Result<(), InitError> {
    let host = MemoryHost::new(false);
    host.state.lock().expect("state lock").credential.secret =
        json!({"access_token": "fresh", "expires_at": i64::MAX});
    let core = core(&host)?;
    let mut target = target();
    let mut headers = HeaderMap::new();
    headers.insert("originator", "operator-profile".parse().unwrap());
    target.provider.fingerprint = Some(crate::ConfiguredFingerprint::Usable(Box::new(
        crate::FingerprintOverride {
            headers,
            profile: None,
        },
    )));
    block_on(core.invoke(&host, &target, request(false, "fingerprint")))
        .expect("configured header fingerprint");
    assert_eq!(
        host.state.lock().expect("state lock").fingerprint_headers,
        ["operator-profile"]
    );

    target.provider.fingerprint = Some(crate::ConfiguredFingerprint::Invalid("empty".into()));
    let error = block_on(core.invoke(&host, &target, request(false, "invalid-fingerprint")))
        .expect_err("invalid fingerprint must fail the attempt");
    assert!(matches!(
        error,
        CoreError::Channel(gproxy_channel_api::ChannelError::Prepare(_))
    ));
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
fn missing_usage_uses_nonzero_cross_target_estimates() -> Result<(), InitError> {
    for stream in [false, true] {
        let host = MemoryHost::new(false);
        host.state.lock().expect("state lock").omit_usage = true;
        let core = core(&host)?;
        let outcome = block_on(core.invoke(&host, &target(), request(stream, "estimate")))
            .expect("estimated invoke");
        if let ResponseBody::Stream(mut body) = outcome.body {
            block_on(async { while body.next().await.is_some() {} });
        }
        let state = host.state.lock().expect("state lock");
        let settlement = state.settlements.last().expect("estimated settlement");
        assert_eq!(settlement.source, UsageSource::Estimated);
        assert!(settlement.usage.input_tokens > 0);
        assert!(settlement.usage.output_tokens > 0);
        assert!(settlement.cost > Decimal::ZERO);
    }
    let host = MemoryHost::new(false);
    host.state.lock().expect("state lock").omit_usage = true;
    let core = core(&host)?;
    let mut search = request(false, "estimate-search");
    search.path = "/v1/alpha/search".into();
    block_on(core.invoke(&host, &target(), search)).expect("estimated search");
    assert_eq!(
        host.state.lock().expect("state lock").settlements[0]
            .usage
            .metrics["web_searches"],
        Decimal::ONE
    );
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
            crate::execution::request::classify(&request)
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
        crate::execution::request::classify(&models)
            .expect("Claude models")
            .key
            .kind(),
        gproxy_protocol::OperationKind::Family(gproxy_protocol::WireFamily::Claude)
    );
    models.upgrade = true;
    assert!(matches!(
        crate::execution::request::classify(&models),
        Err(CoreError::Unsupported)
    ));

    let mut gemini = request(false, "gemini-framing");
    gemini.path = "/v1beta/models/gemini-test:streamGenerateContent".into();
    gemini.body = Bytes::from_static(b"{}");
    let classified = crate::execution::request::classify(&gemini).expect("Gemini JSON array");
    assert!(classified.stream);
    assert_eq!(
        classified.framing,
        gproxy_protocol::StreamFraming::JsonArray
    );
    gemini.query = Some("alt=sse".into());
    assert_eq!(
        crate::execution::request::classify(&gemini)
            .expect("Gemini SSE")
            .framing,
        gproxy_protocol::StreamFraming::Sse
    );

    let mut veo_poll = request(false, "veo-poll");
    veo_poll.method = Method::GET;
    veo_poll.path = "/v1beta/models/veo-3/operations/op-1".into();
    veo_poll.body = Bytes::new();
    let classified = crate::execution::request::classify(&veo_poll).expect("Veo poll");
    assert_eq!(classified.model.as_deref(), Some("veo-3"));
    assert_eq!(classified.resource(), Some(("video", "op-1")));

    let mut veo_create = request(false, "veo-create");
    veo_create.path = "/v1beta/models/veo-3:predictLongRunning".into();
    veo_create.body = Bytes::from_static(br#"{"instances":[{"prompt":"hello"}]}"#);
    assert_eq!(
        crate::execution::request::classify(&veo_create)
            .expect("Veo create")
            .resource(),
        None
    );

    let mut realtime = request(false, "realtime-model");
    realtime.path = "/v1/realtime/calls".into();
    realtime.body = Bytes::from_static(
        br#"{"sdp":"offer","session":{"type":"realtime","model":"gpt-realtime"}}"#,
    );
    assert_eq!(
        crate::execution::request::classify(&realtime)
            .expect("Realtime call")
            .model
            .as_deref(),
        Some("gpt-realtime")
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
        assert_eq!(state.health[0].1, "upstream-model");
        assert_eq!(state.health[0].2, CredentialHealth::Degraded);
        assert!(
            state
                .health
                .iter()
                .all(|(_, model, _)| model == "upstream-model")
        );
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
            fingerprint: None,
            proxy_url: None,
            traffic_blacklist: Default::default(),
        },
        credential: CredentialId(7),
        upstream_model: "upstream-model".into(),
        tier: 0,
        rules: Default::default(),
    }
}

fn request(stream: bool, id: &str) -> RequestCtx {
    RequestCtx {
        request_id: format!("request-{id}"),
        client_ip: None,
        method: Method::POST,
        path: "/v1/responses".into(),
        query: None,
        headers: HeaderMap::new(),
        body: Bytes::from(format!(r#"{{"model":"alias","stream":{stream}}}"#)),
        upgrade: false,
        force_model_refresh: false,
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
