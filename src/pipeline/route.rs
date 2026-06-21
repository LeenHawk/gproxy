//! Route lookup: canonical route name → resolved backend pool.

use std::sync::Arc;

use crate::app::snapshot::{ControlPlaneSnapshot, ResolvedRoute};
use crate::pipeline::error::PipelineError;

/// Look up a route by canonical name.
pub fn route<'a>(
    cp: &'a ControlPlaneSnapshot,
    route_name: &str,
) -> Result<&'a Arc<ResolvedRoute>, PipelineError> {
    cp.routes_by_name
        .get(route_name)
        .ok_or_else(|| PipelineError::UnknownRoute(route_name.to_string()))
}
