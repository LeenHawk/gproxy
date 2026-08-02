//! Final failover error and count-token local fallback.

use crate::app::AppState;
use crate::pipeline::context::{Candidate, RequestCtx};
use crate::pipeline::error::PipelineError;
use crate::pipeline::local_ops;
use crate::pipeline::outcome::ExecOutcome;
use crate::protocol::Operation;

pub(super) fn finish(
    state: &AppState,
    ctx: &RequestCtx,
    candidates: &[Candidate],
    last_error: Option<PipelineError>,
    attempts: u32,
    max_attempts: u32,
) -> Result<ExecOutcome, PipelineError> {
    if ctx.op.expect("classified").operation == Operation::CountTokens
        && let Some(candidate) = candidates.first()
        && let Some(outcome) = local_ops::serve_local(state, &state.cp(), ctx, candidate)
    {
        let error = last_error
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| PipelineError::AllAttemptsFailed.to_string());
        super::telemetry::all_attempts_failed(ctx, attempts, max_attempts, &error);
        tracing::warn!("all upstream count attempts failed; serving local count fallback");
        return Ok(outcome);
    }
    let error = last_error.unwrap_or(PipelineError::AllAttemptsFailed);
    super::telemetry::all_attempts_failed(ctx, attempts, max_attempts, &error.to_string());
    Err(error)
}
