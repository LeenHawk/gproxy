use bytes::Bytes;
use gproxy_channel_api::{
    BoxFuture, Channel, ChannelDescriptor, ChannelError, ChannelSupport, Disposition,
    NormalizedUsage, PrepareCtx, PreparedRequest, ResourceCtx, ResourceMutation, ResponseView,
    SessionPreparer, SimpleHttp, StreamCtx, StreamDecoder, SurfaceRequest, SurfaceTable, UsageCtx,
};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey};

use super::memory::MemoryHost;
use super::{block_on, core, request, target};
use crate::control::{FailoverBudget, Plan};

const KEY: OperationKey = OperationKey::content(
    Operation::GenerateContent,
    ContentGenerationKind::OpenAiResponses,
);
const STREAM_KEY: OperationKey = OperationKey::content(
    Operation::StreamGenerateContent,
    ContentGenerationKind::OpenAiResponses,
);
const CREATE_FILE: OperationKey =
    OperationKey::family(Operation::CreateFile, gproxy_protocol::WireFamily::OpenAi);
const RETRIEVE_FILE: OperationKey =
    OperationKey::family(Operation::RetrieveFile, gproxy_protocol::WireFamily::OpenAi);
const DELETE_FILE: OperationKey =
    OperationKey::family(Operation::DeleteFile, gproxy_protocol::WireFamily::OpenAi);
const LIST_FILES: OperationKey =
    OperationKey::family(Operation::ListFiles, gproxy_protocol::WireFamily::OpenAi);
const WEB_SEARCH: OperationKey =
    OperationKey::family(Operation::WebSearch, gproxy_protocol::WireFamily::OpenAi);
const REALTIME: OperationKey = OperationKey::family(
    Operation::CreateRealtimeCall,
    gproxy_protocol::WireFamily::OpenAi,
);
const CLAUDE_MODELS: OperationKey =
    OperationKey::family(Operation::ListModels, gproxy_protocol::WireFamily::Claude);
const OPENAI_MODELS: OperationKey =
    OperationKey::family(Operation::ListModels, gproxy_protocol::WireFamily::OpenAi);
const CLAUDE_MESSAGES: OperationKey = OperationKey::content(
    Operation::GenerateContent,
    ContentGenerationKind::ClaudeMessages,
);
static SUPPORTS: [ChannelSupport; 11] = [
    ChannelSupport::passthrough(KEY),
    ChannelSupport::passthrough(STREAM_KEY),
    ChannelSupport::passthrough(CREATE_FILE),
    ChannelSupport::passthrough(RETRIEVE_FILE),
    ChannelSupport::passthrough(DELETE_FILE),
    ChannelSupport::passthrough(LIST_FILES),
    ChannelSupport::passthrough(WEB_SEARCH),
    ChannelSupport::passthrough(REALTIME),
    ChannelSupport::passthrough(CLAUDE_MODELS),
    ChannelSupport::passthrough(OPENAI_MODELS),
    ChannelSupport::passthrough(CLAUDE_MESSAGES),
];
static DESCRIPTOR: ChannelDescriptor = ChannelDescriptor {
    id: "memory",
    display_name: "Memory",
    supports: &SUPPORTS,
    provider_fields: &[],
    credential_fields: &[],
    endpoint_overrides: false,
    traffic_policy: gproxy_channel_api::ChannelTrafficPolicy::new(
        &["*"],
        &["x-test-visible"],
        &["*"],
    ),
};

pub(super) struct ForeignSurface;
pub(super) struct NeedsContinuation;

static FOREIGN_SUPPORTS: [ChannelSupport; 0] = [];
static FOREIGN_DESCRIPTOR: ChannelDescriptor = ChannelDescriptor {
    id: "foreign",
    display_name: "Foreign",
    supports: &FOREIGN_SUPPORTS,
    provider_fields: &[],
    credential_fields: &[],
    endpoint_overrides: false,
    traffic_policy: gproxy_channel_api::ChannelTrafficPolicy::new(&[], &[], &[]),
};
static CONTINUATION_DESCRIPTOR: ChannelDescriptor = ChannelDescriptor {
    id: "continuation-test",
    display_name: "Continuation Test",
    supports: &[],
    provider_fields: &[],
    credential_fields: &[],
    endpoint_overrides: false,
    traffic_policy: gproxy_channel_api::ChannelTrafficPolicy::new(&[], &[], &[]),
};

impl Channel for NeedsContinuation {
    fn routing_table(&self) -> &'static [ChannelSupport] {
        &[]
    }

    fn descriptor(&self) -> &'static ChannelDescriptor {
        &CONTINUATION_DESCRIPTOR
    }

    fn prepare(&self, _: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        Err(ChannelError::Prepare("unused".into()))
    }

    fn classify(&self, _: ResponseView<'_>) -> Disposition {
        Disposition::Terminal
    }

    fn extract_usage(&self, _: UsageCtx<'_>) -> Option<NormalizedUsage> {
        None
    }

    fn requires_continuations(&self) -> bool {
        true
    }
}
static FOREIGN_SURFACES: [gproxy_channel_api::SurfaceEntry; 1] =
    [gproxy_channel_api::SurfaceEntry {
        method: &http::Method::GET,
        pattern: gproxy_protocol::PathPattern(&[
            gproxy_protocol::Seg::Lit("v1"),
            gproxy_protocol::Seg::Lit("files"),
        ]),
        affinity: gproxy_channel_api::SurfaceAffinity::None,
        action: gproxy_channel_api::SurfaceAction::Forward(gproxy_channel_api::ForwardSpec {
            label: "foreign_files",
            upstream_template: "/foreign/files",
            retry: gproxy_channel_api::ForwardRetry::Retryable,
        }),
    }];

impl Channel for ForeignSurface {
    fn routing_table(&self) -> &'static [ChannelSupport] {
        &[]
    }

    fn descriptor(&self) -> &'static ChannelDescriptor {
        &FOREIGN_DESCRIPTOR
    }

    fn prepare(&self, _: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        Err(ChannelError::Prepare("foreign surface only".into()))
    }

    fn classify(&self, _: ResponseView<'_>) -> Disposition {
        Disposition::Terminal
    }

    fn extract_usage(&self, _: UsageCtx<'_>) -> Option<NormalizedUsage> {
        None
    }

    fn surfaces(&self) -> SurfaceTable {
        SurfaceTable(&FOREIGN_SURFACES)
    }
}

impl Channel for MemoryHost {
    fn routing_table(&self) -> &'static [ChannelSupport] {
        &SUPPORTS
    }

    fn descriptor(&self) -> &'static ChannelDescriptor {
        &DESCRIPTOR
    }

    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        let token = ctx.secret["access_token"]
            .as_str()
            .ok_or_else(|| ChannelError::Secret("access_token missing".into()))?;
        let mut uri = format!("https://upstream.test{}", ctx.path);
        if let Some(query) = ctx.query {
            uri.push('?');
            uri.push_str(query);
        }
        let mut request = http::Request::builder()
            .method(ctx.method)
            .uri(uri)
            .body(ctx.body.clone())
            .map_err(|error| ChannelError::Prepare(error.to_string()))?;
        *request.headers_mut() = ctx.headers.clone();
        request.headers_mut().insert(
            http::header::AUTHORIZATION,
            format!("Bearer {token}")
                .parse()
                .map_err(|error| ChannelError::Prepare(format!("upstream credential: {error}")))?,
        );
        Ok(PreparedRequest {
            request,
            framing: None,
            websocket: false,
            profile: None,
        })
    }

    fn classify(&self, response: ResponseView<'_>) -> Disposition {
        match response.status {
            http::StatusCode::TOO_MANY_REQUESTS => Disposition::Retryable,
            http::StatusCode::UNAUTHORIZED => Disposition::CredentialDead,
            status if status.is_success() => Disposition::Success,
            _ => Disposition::Terminal,
        }
    }

    fn extract_usage(&self, ctx: UsageCtx<'_>) -> Option<NormalizedUsage> {
        if self.state.lock().expect("state lock").omit_usage {
            return None;
        }
        serde_json::from_slice::<serde_json::Value>(ctx.response_body)
            .ok()?
            .get("usage")?;
        Some(NormalizedUsage {
            input_tokens: 10,
            output_tokens: 5,
            ..Default::default()
        })
    }

    fn session_preparer(&self) -> Option<SessionPreparer> {
        Some(super::channel_session::prepare_test_session)
    }

    fn stream_decoder(&self, _: StreamCtx<'_>) -> Option<Box<dyn StreamDecoder>> {
        Some(Box::new(self.clone()))
    }

    fn resource_mutations(
        &self,
        ctx: ResourceCtx<'_>,
    ) -> Result<Vec<ResourceMutation>, ChannelError> {
        if ctx.key.operation == Operation::CreateRealtimeCall {
            let id = ctx
                .response_headers
                .get(http::header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.rsplit('/').next())
                .ok_or_else(|| ChannelError::Observe("test call id missing".into()))?;
            return Ok(vec![ResourceMutation::Save {
                kind: "realtime_call",
                id: id.into(),
                summary: serde_json::json!({"id": id}),
            }]);
        }
        if ctx.key.operation == Operation::DeleteFile {
            return Ok(ctx
                .request_resource
                .map(|(kind, id)| ResourceMutation::Delete {
                    kind,
                    id: id.to_owned(),
                })
                .into_iter()
                .collect());
        }
        let value: serde_json::Value = serde_json::from_slice(ctx.response_body)
            .map_err(|error| ChannelError::Observe(error.to_string()))?;
        let resources = value
            .get("data")
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_else(|| vec![value]);
        Ok(resources
            .into_iter()
            .filter_map(|summary| {
                let id = summary.get("id")?.as_str()?.to_owned();
                Some(ResourceMutation::Save {
                    kind: "file",
                    id,
                    summary,
                })
            })
            .collect())
    }

    fn refresh_due(&self, secret: &serde_json::Value) -> Option<i64> {
        secret.get("expires_at")?.as_i64()
    }

    fn refresh<'a>(
        &'a self,
        _: &'a serde_json::Value,
        _: &'a serde_json::Value,
        http: &'a dyn SimpleHttp,
    ) -> Option<BoxFuture<'a, Result<serde_json::Value, ChannelError>>> {
        let request = http::Request::post("https://auth.test/refresh")
            .body(Bytes::new())
            .expect("refresh request");
        let send = http.send(request);
        Some(Box::pin(async move {
            let response = send.await?;
            serde_json::from_slice(response.body())
                .map_err(|error| ChannelError::Refresh(error.to_string()))
        }))
    }

    fn prepare_surface(
        &self,
        request: &SurfaceRequest,
        websocket: bool,
        _: &serde_json::Value,
        secret: &serde_json::Value,
    ) -> Result<PreparedRequest, ChannelError> {
        let token = secret["access_token"]
            .as_str()
            .ok_or_else(|| ChannelError::Secret("access_token missing".into()))?;
        let mut uri = format!("https://upstream.test{}", request.upstream_path);
        if let Some(query) = &request.query {
            uri.push('?');
            uri.push_str(query);
        }
        let request = http::Request::builder()
            .method(&request.method)
            .uri(uri)
            .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
            .body(request.body.clone())
            .map_err(|error| ChannelError::Prepare(error.to_string()))?;
        Ok(PreparedRequest {
            request,
            framing: None,
            websocket,
            profile: None,
        })
    }

    fn surfaces(&self) -> SurfaceTable {
        super::surface::table()
    }
}

#[test]
fn ingress_floor_strips_credentials_after_claude_classification() -> Result<(), crate::InitError> {
    let host = MemoryHost::new(false);
    let mut state = host.state.lock().expect("state lock");
    state.plan = Some(Plan {
        targets: vec![target()],
        budget: FailoverBudget { max_attempts: 1 },
    });
    drop(state);
    let core = core(&host)?;
    let mut request = request(false, "claude-ingress-floor");
    request.method = http::Method::POST;
    request.path = "/v1/messages".into();
    request.query = Some("key=caller-test-key&beta=true".into());
    request.body = Bytes::from_static(
        br#"{"model":"alias","max_tokens":8,"messages":[{"role":"user","content":"hi"}]}"#,
    );
    for (name, value) in [
        ("x-api-key", "caller-test-key"),
        ("anthropic-version", "2023-06-01"),
        ("anthropic-beta", "files-api-2025-04-14"),
        ("connection", "x-caller-hop"),
        ("x-caller-hop", "drop-me"),
        ("x-forwarded-for", "192.0.2.1"),
        ("accept-encoding", "gzip"),
    ] {
        request.headers.insert(name, value.parse().expect("header"));
    }

    assert_eq!(
        block_on(core.execute(&host, request))
            .expect("Claude request remains classified and supported")
            .status,
        http::StatusCode::OK
    );
    let state = host.state.lock().expect("state lock");
    assert_eq!(state.auth_calls, 1);
    let (headers, uri) = state.upstream_requests.last().expect("upstream request");
    assert_eq!(headers[http::header::AUTHORIZATION], "Bearer fresh");
    for denied in [
        "x-api-key",
        "connection",
        "x-caller-hop",
        "x-forwarded-for",
        "accept-encoding",
    ] {
        assert!(!headers.contains_key(denied), "forwarded {denied}");
    }
    assert_eq!(headers["anthropic-version"], "2023-06-01");
    assert_eq!(headers["anthropic-beta"], "files-api-2025-04-14");
    assert_eq!(uri, "https://upstream.test/v1/messages?beta=true");
    Ok(())
}

#[test]
fn aggregated_model_list_refreshes_upstream_and_keeps_declared_metadata()
-> Result<(), crate::InitError> {
    let host = MemoryHost::new(false);
    let mut state = host.state.lock().expect("state lock");
    state.plan = Some(Plan {
        targets: vec![target()],
        budget: FailoverBudget { max_attempts: 1 },
    });
    state.exposed_models[0].display_name = Some("Alias model".into());
    state.exposed_models[0].context_window = Some(128_000);
    state.exposed_models[0].max_output_tokens = Some(16_384);
    state.exposed_models[0].thinking_supported = Some(true);
    drop(state);
    let core = core(&host)?;
    let mut request = request(false, "local-models");
    request.method = http::Method::GET;
    request.path = "/v1/models".into();
    request.body = Bytes::new();
    let outcome = block_on(core.execute(&host, request)).expect("local model list");
    let crate::ResponseBody::Full(body) = outcome.body else {
        panic!("local response was not buffered");
    };
    let body: serde_json::Value = serde_json::from_slice(&body).expect("model list JSON");
    assert_eq!(body["data"][0]["id"], "alias");
    assert_eq!(body["data"][0]["context_window"], 128_000);
    assert_eq!(body["data"][0]["context_length"], 128_000);
    assert_eq!(body["data"][0]["max_completion_tokens"], 16_384);
    assert_eq!(
        body["data"][0]["supported_parameters"],
        serde_json::json!(["reasoning"])
    );
    assert_eq!(body["data"][1]["id"], "provider/fresh-model");
    assert_eq!(body["data"][1]["context_window"], 200_000);
    assert_eq!(body["data"][1]["max_completion_tokens"], 32_000);
    let state = host.state.lock().expect("state lock");
    assert_eq!(state.upstream_requests.len(), 1);
    assert_eq!(state.settlements.len(), 1);
    assert!(
        state
            .settlements
            .iter()
            .all(|settlement| settlement.cost == rust_decimal::Decimal::ZERO)
    );
    assert_eq!(state.admission_finishes, [true, true]);
    Ok(())
}

#[test]
fn claude_magic_cache_follows_the_provider_switch() -> Result<(), crate::InitError> {
    const TRIGGER: &str = "GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_1FAS5GV9R5H29T5Y2J9584K6O95M2NBVW52C95CX984FRJY";

    for enabled in [false, true] {
        let host = MemoryHost::new(false);
        let mut selected = target();
        selected.provider.settings = serde_json::json!({
            "enable_claude_magic_cache": enabled
        });
        host.state.lock().expect("state lock").plan = Some(Plan {
            targets: vec![selected],
            budget: FailoverBudget { max_attempts: 1 },
        });
        let core = core(&host)?;
        let mut request = request(false, if enabled { "cache-on" } else { "cache-off" });
        request.path = "/v1/messages".into();
        request
            .headers
            .insert("x-api-key", "caller".parse().unwrap());
        request.body = Bytes::from(
            serde_json::json!({
                "model": "alias",
                "max_tokens": 8,
                "messages": [{"role": "user", "content": format!("stable prompt {TRIGGER}")}]
            })
            .to_string(),
        );
        block_on(core.execute(&host, request)).expect("Claude magic cache request");
        let state = host.state.lock().expect("state lock");
        let body: serde_json::Value =
            serde_json::from_slice(state.upstream_bodies.last().expect("upstream body")).unwrap();
        if enabled {
            let block = &body["messages"][0]["content"][0];
            assert_eq!(block["text"], "stable prompt");
            assert_eq!(block["cache_control"]["type"], "ephemeral");
            assert_eq!(block["cache_control"]["ttl"], "1h");
        } else {
            assert!(
                body["messages"][0]["content"]
                    .as_str()
                    .unwrap()
                    .contains(TRIGGER)
            );
        }
    }
    Ok(())
}
