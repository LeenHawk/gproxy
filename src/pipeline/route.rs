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

/// Look up a logical route inside one public namespace. Namespace names are
/// normalized to lowercase at snapshot build time.
pub fn route_in_namespace<'a>(
    cp: &'a ControlPlaneSnapshot,
    namespace: &str,
    route_name: &str,
) -> Result<&'a Arc<ResolvedRoute>, PipelineError> {
    cp.routes_by_namespace
        .get(&namespace.to_ascii_lowercase())
        .and_then(|routes| routes.get(route_name))
        .ok_or_else(|| PipelineError::UnknownRoute(route_name.to_string()))
}
