use bytes::Bytes;
use gproxy_channel_api::{
    BoxFuture, ChannelError, ForwardSpec, SurfaceAction, SurfaceAffinity, SurfaceBody,
    SurfaceEntry, SurfaceReply, SurfaceRequest, SurfaceServices, SurfaceTable, SynthCtx,
    Synthesizer,
};
use gproxy_protocol::{PathPattern, Seg};
use http::Method;

struct MemorySynth;

static SYNTH: MemorySynth = MemorySynth;
static ENTRIES: [SurfaceEntry; 6] = [
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
        affinity: SurfaceAffinity::BodyField {
            name: "server_id",
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
            let body = serde_json::json!({
                "credential": credential,
                "provider": services.provider.id,
                "slot": services.provider.settings["slot"],
                "user": services.identity.user_id,
                "cost": usage.cost,
            });
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
