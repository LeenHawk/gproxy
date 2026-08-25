use web_time::Instant;

use gproxy_channel_api::SurfaceRequest;

use crate::api::Core;
use crate::boundary::{ExecOutcome, RequestCtx};
use crate::error::CoreError;
use crate::host::Host;

use super::super::affinity::Selected;

pub(crate) async fn declared<H: Host>(
    core: &Core<H>,
    selected: &Selected,
    ctx: &RequestCtx,
    started: Instant,
) -> Result<ExecOutcome, CoreError> {
    let (spec, websocket) = match &selected.entry.action {
        gproxy_channel_api::SurfaceAction::Forward(spec) => (spec, false),
        gproxy_channel_api::SurfaceAction::ForwardWebSocket(spec) => (spec, true),
        gproxy_channel_api::SurfaceAction::OperationAlias { .. } => {
            return Err(CoreError::Internal(
                "operation alias reached the forward engine".into(),
            ));
        }
        gproxy_channel_api::SurfaceAction::Synthesize { .. } => {
            return Err(CoreError::Internal(
                "synthesizer reached the forward engine".into(),
            ));
        }
    };
    let request = SurfaceRequest {
        label: spec.label,
        key: None,
        stream: !websocket
            && !matches!(
                selected.entry.affinity,
                gproxy_channel_api::SurfaceAffinity::BodyField { .. }
                    | gproxy_channel_api::SurfaceAffinity::HeaderOrBodyField { .. }
                    | gproxy_channel_api::SurfaceAffinity::ResponseBodyToken { .. }
            ),
        method: ctx.method.clone(),
        upstream_path: super::super::template::render(spec.upstream_template, &selected.params)?,
        query: ctx.query.clone(),
        headers: ctx.headers.clone(),
        body: ctx.body.clone(),
        credential: Some(selected.target.credential),
    };
    super::request(
        core,
        &selected.target,
        request,
        websocket,
        ctx.request_id.clone(),
        started,
        None,
    )
    .await
}
