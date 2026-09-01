use web_time::Instant;

use crate::api::Core;
use crate::boundary::{ExecOutcome, RequestCtx};
use crate::control::{FailoverBudget, Target};
use crate::error::CoreError;
use crate::host::Host;

use super::super::affinity::Selected;

pub(crate) async fn declared<H: Host>(
    core: &Core<H>,
    selected: &Selected,
    ctx: &RequestCtx,
    budget: FailoverBudget,
    started: Instant,
) -> Result<(ExecOutcome, Target), CoreError> {
    let (spec, websocket) = match &selected.entry.action {
        gproxy_channel_api::SurfaceAction::Forward(spec) => (spec, false),
        gproxy_channel_api::SurfaceAction::ForwardWebSocket(spec) => (spec, true),
        gproxy_channel_api::SurfaceAction::OperationAlias { .. } => {
            return Err(CoreError::Internal(
                "operation alias reached the forward engine".into(),
            ));
        }
        gproxy_channel_api::SurfaceAction::Synthesize { .. }
        | gproxy_channel_api::SurfaceAction::PublicSynthesize { .. } => {
            return Err(CoreError::Internal(
                "synthesizer reached the forward engine".into(),
            ));
        }
    };
    super::failover::run(core, selected, ctx, spec, websocket, budget, started).await
}
