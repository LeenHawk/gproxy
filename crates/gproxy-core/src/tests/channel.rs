use bytes::Bytes;
use gproxy_channel_api::{
    BoxFuture, Channel, ChannelDescriptor, ChannelError, ChannelSupport, Disposition, Frame,
    NormalizedUsage, PrepareCtx, PreparedRequest, ResourceCtx, ResourceMutation, ResponseView,
    SimpleHttp, StreamCtx, StreamDecoder, StreamEnd, StreamTail, SurfaceRequest, SurfaceTable,
    UsageCtx,
};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey};

use super::memory::MemoryHost;

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
static SUPPORTS: [ChannelSupport; 7] = [
    ChannelSupport::passthrough(KEY),
    ChannelSupport::passthrough(STREAM_KEY),
    ChannelSupport::passthrough(CREATE_FILE),
    ChannelSupport::passthrough(RETRIEVE_FILE),
    ChannelSupport::passthrough(DELETE_FILE),
    ChannelSupport::passthrough(LIST_FILES),
    ChannelSupport::passthrough(WEB_SEARCH),
];
static DESCRIPTOR: ChannelDescriptor = ChannelDescriptor {
    id: "memory",
    display_name: "Memory",
    supports: &SUPPORTS,
};

pub(super) struct ForeignSurface;

static FOREIGN_SUPPORTS: [ChannelSupport; 0] = [];
static FOREIGN_DESCRIPTOR: ChannelDescriptor = ChannelDescriptor {
    id: "foreign",
    display_name: "Foreign",
    supports: &FOREIGN_SUPPORTS,
};
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
        }),
    }];

impl Channel for ForeignSurface {
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
    fn descriptor(&self) -> &'static ChannelDescriptor {
        &DESCRIPTOR
    }

    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        let token = ctx.secret["access_token"]
            .as_str()
            .ok_or_else(|| ChannelError::Secret("access_token missing".into()))?;
        let request = http::Request::builder()
            .method(ctx.method)
            .uri(format!("https://upstream.test{}", ctx.path))
            .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
            .body(ctx.body.clone())
            .map_err(|error| ChannelError::Prepare(error.to_string()))?;
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

    fn stream_decoder(&self, _: StreamCtx<'_>) -> Option<Box<dyn StreamDecoder>> {
        Some(Box::new(self.clone()))
    }

    fn resource_mutations(
        &self,
        ctx: ResourceCtx<'_>,
    ) -> Result<Vec<ResourceMutation>, ChannelError> {
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

impl StreamDecoder for MemoryHost {
    fn push(&mut self, chunk: Bytes) -> Result<Vec<Frame>, ChannelError> {
        Ok(vec![Frame(chunk)])
    }

    fn finish(&mut self, end: StreamEnd) -> Result<StreamTail, ChannelError> {
        let omit_usage = self.state.lock().expect("state lock").omit_usage;
        Ok(StreamTail {
            frames: (end == StreamEnd::Complete)
                .then_some(Frame(Bytes::from_static(b"tail")))
                .into_iter()
                .collect(),
            usage: (!omit_usage).then(|| NormalizedUsage {
                input_tokens: 10,
                output_tokens: 5,
                ..Default::default()
            }),
        })
    }
}
