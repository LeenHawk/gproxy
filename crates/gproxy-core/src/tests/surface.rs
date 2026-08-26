use bytes::Bytes;
use gproxy_channel_api::{
    BoxFuture, ChannelError, ForwardRetry, ForwardSpec, SurfaceAction, SurfaceAffinity,
    SurfaceBody, SurfaceEntry, SurfaceReply, SurfaceRequest, SurfaceServices, SurfaceTable,
    SynthCtx, Synthesizer,
};
use gproxy_protocol::{PathPattern, Seg};
use http::Method;

struct MemorySynth;

static SYNTH: MemorySynth = MemorySynth;
static ENTRIES: [SurfaceEntry; 13] = [
    SurfaceEntry {
        method: &Method::GET,
        pattern: PathPattern(&[
            Seg::Lit("surface"),
            Seg::Lit("tasks"),
            Seg::Param("task_id"),
        ]),
        affinity: SurfaceAffinity::Binding {
            kind: "task",
            param: "task_id",
        },
        action: SurfaceAction::Synthesize {
            handler: &SYNTH,
            upstream: false,
        },
    },
    SurfaceEntry {
        method: &Method::GET,
        pattern: PathPattern(&[Seg::Lit("surface"), Seg::Lit("header")]),
        affinity: SurfaceAffinity::Header {
            name: "x-session",
            ttl_secs: 60,
        },
        action: SurfaceAction::Synthesize {
            handler: &SYNTH,
            upstream: false,
        },
    },
    SurfaceEntry {
        method: &Method::POST,
        pattern: PathPattern(&[Seg::Lit("surface"), Seg::Lit("body")]),
        affinity: SurfaceAffinity::HeaderOrBodyField {
            header: "x-server-id",
            body_field: "server_id",
            ttl_secs: 60,
        },
        action: SurfaceAction::Synthesize {
            handler: &SYNTH,
            upstream: false,
        },
    },
    SurfaceEntry {
        method: &Method::GET,
        pattern: PathPattern(&[Seg::Lit("surface"), Seg::Lit("invoke")]),
        affinity: SurfaceAffinity::None,
        action: SurfaceAction::Synthesize {
            handler: &SYNTH,
            upstream: true,
        },
    },
    SurfaceEntry {
        method: &Method::GET,
        pattern: PathPattern(&[
            Seg::Lit("surface"),
            Seg::Lit("forward"),
            Seg::Param("task_id"),
        ]),
        affinity: SurfaceAffinity::Binding {
            kind: "task",
            param: "task_id",
        },
        action: SurfaceAction::Forward(ForwardSpec {
            label: "forward-test",
            upstream_template: "/control/{task_id}",
            retry: ForwardRetry::Retryable,
        }),
    },
    SurfaceEntry {
        method: &Method::GET,
        pattern: PathPattern(&[
            Seg::Lit("surface"),
            Seg::Lit("socket"),
            Seg::Param("task_id"),
        ]),
        affinity: SurfaceAffinity::Binding {
            kind: "task",
            param: "task_id",
        },
        action: SurfaceAction::ForwardWebSocket(ForwardSpec {
            label: "socket-test",
            upstream_template: "/socket/{task_id}",
            retry: ForwardRetry::SingleAttempt,
        }),
    },
    SurfaceEntry {
        method: &Method::POST,
        pattern: PathPattern(&[Seg::Lit("surface"), Seg::Lit("alias")]),
        affinity: SurfaceAffinity::None,
        action: SurfaceAction::OperationAlias {
            canonical_path: "/v1/responses",
        },
    },
    SurfaceEntry {
        method: &Method::POST,
        pattern: PathPattern(&[Seg::Lit("surface"), Seg::Lit("token")]),
        affinity: SurfaceAffinity::ResponseBodyToken {
            field: "remote_token",
            namespace: "remote",
            request_body_field: None,
            also_body_field: Some("server_id"),
            also_path_field: Some("environment_id"),
            ttl_secs: 60,
        },
        action: SurfaceAction::Synthesize {
            handler: &SYNTH,
            upstream: false,
        },
    },
    SurfaceEntry {
        method: &Method::POST,
        pattern: PathPattern(&[Seg::Lit("surface"), Seg::Lit("token"), Seg::Lit("refresh")]),
        affinity: SurfaceAffinity::ResponseBodyToken {
            field: "remote_token",
            namespace: "remote",
            request_body_field: Some("server_id"),
            also_body_field: Some("server_id"),
            also_path_field: Some("environment_id"),
            ttl_secs: 60,
        },
        action: SurfaceAction::Synthesize {
            handler: &SYNTH,
            upstream: false,
        },
    },
    SurfaceEntry {
        method: &Method::GET,
        pattern: PathPattern(&[
            Seg::Lit("surface"),
            Seg::Lit("environment"),
            Seg::Param("environment_id"),
        ]),
        affinity: SurfaceAffinity::PathParam {
            name: "environment_id",
            ttl_secs: 60,
        },
        action: SurfaceAction::Synthesize {
            handler: &SYNTH,
            upstream: false,
        },
    },
    SurfaceEntry {
        method: &Method::GET,
        pattern: PathPattern(&[Seg::Lit("surface"), Seg::Lit("token"), Seg::Lit("socket")]),
        affinity: SurfaceAffinity::BearerToken {
            namespace: "remote",
        },
        action: SurfaceAction::ForwardWebSocket(ForwardSpec {
            label: "token-socket-test",
            upstream_template: "/socket/token",
            retry: ForwardRetry::SingleAttempt,
        }),
    },
    SurfaceEntry {
        method: &Method::GET,
        pattern: PathPattern(&[Seg::Lit("surface"), Seg::Lit("retry")]),
        affinity: SurfaceAffinity::Header {
            name: "x-retry-session",
            ttl_secs: 60,
        },
        action: SurfaceAction::Forward(ForwardSpec {
            label: "retry-test",
            upstream_template: "/control/retry",
            retry: ForwardRetry::Retryable,
        }),
    },
    SurfaceEntry {
        method: &Method::POST,
        pattern: PathPattern(&[Seg::Lit("surface"), Seg::Lit("mutate")]),
        affinity: SurfaceAffinity::None,
        action: SurfaceAction::Forward(ForwardSpec {
            label: "mutation-test",
            upstream_template: "/control/mutate",
            retry: ForwardRetry::SingleAttempt,
        }),
    },
];

pub(super) fn table() -> SurfaceTable {
    SurfaceTable(&ENTRIES)
}

impl Synthesizer for MemorySynth {
    fn respond<'a>(
        &'a self,
        ctx: SynthCtx<'a>,
        services: SurfaceServices<'a>,
    ) -> BoxFuture<'a, Result<SurfaceReply, ChannelError>> {
        Box::pin(async move {
            if let Some(invoke) = services.invoke {
                return invoke
                    .invoke(SurfaceRequest {
                        label: "test-control",
                        key: None,
                        stream: false,
                        method: Method::GET,
                        upstream_path: "/control".into(),
                        query: None,
                        headers: http::HeaderMap::new(),
                        body: Bytes::new(),
                        credential: None,
                    })
                    .await
                    .map_err(|error| ChannelError::Prepare(error.to_string()));
            }
            let task = ctx
                .params
                .iter()
                .find_map(|(name, value)| (*name == "task_id").then_some(value.as_str()));
            let credential = match task {
                Some(task) => {
                    services
                        .bindings
                        .find(
                            services.provider.id,
                            services.identity.user_id,
                            "task",
                            task,
                        )
                        .await
                        .expect("binding lookup")
                        .expect("task binding")
                        .credential
                        .0
                }
                None => -1,
            };
            let usage = services.usage.window(0).await.expect("usage window");
            let mut body = serde_json::json!({
                "credential": credential,
                "provider": services.provider.id,
                "slot": services.provider.settings["slot"],
                "user": services.identity.user_id,
                "cost": usage.cost,
            });
            if ctx.path == "/surface/token" {
                body["remote_token"] = serde_json::Value::String("remote-secret".into());
                body["server_id"] = serde_json::Value::String("remote-server".into());
                body["environment_id"] = serde_json::Value::String("remote-environment".into());
            }
            let mut headers = http::HeaderMap::new();
            if ctx.path == "/surface/header" {
                let session = ctx
                    .headers
                    .get("x-session")
                    .cloned()
                    .unwrap_or_else(|| http::HeaderValue::from_static("generated"));
                headers.insert("x-session", session);
            }
            Ok(SurfaceReply {
                status: http::StatusCode::OK,
                headers,
                body: SurfaceBody::Full(Bytes::from(body.to_string())),
            })
        })
    }
}
