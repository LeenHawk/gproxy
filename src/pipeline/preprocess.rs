//! Aggregated-mode preprocessing: resolve the requested model name to a
//! canonical route name via the alias / route tables.

use crate::app::snapshot::ControlPlaneSnapshot;
use crate::pipeline::classify::peek_model;
use crate::pipeline::context::RequestCtx;
use crate::pipeline::error::PipelineError;

/// Resolve the body's `model` → alias → route name, falling back to a direct
/// route-name match. 404 if neither matches.
pub fn preprocess(cp: &ControlPlaneSnapshot, ctx: &RequestCtx) -> Result<String, PipelineError> {
    let model =
        peek_model(&ctx.body).ok_or_else(|| PipelineError::UnknownRoute("<no model>".into()))?;
    if let Some(route) = cp.alias_to_route.get(&model) {
        return Ok(route.clone());
    }
    if cp.routes_by_name.contains_key(&model) {
        return Ok(model);
    }
    Err(PipelineError::UnknownRoute(model))
}
