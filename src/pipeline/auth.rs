//! Inbound API-key authentication against the control-plane snapshot.

use std::sync::Arc;

use http::HeaderMap;

use crate::app::snapshot::{ControlPlaneSnapshot, KeyIdentity};
use crate::pipeline::error::PipelineError;

pub use crate::util::api_key::{extract_bearer, key_digest};

/// Resolve an inbound API key → digest → snapshot identity. No DB hit. 401
/// short-circuits HERE, before any upstream candidate is built.
pub fn authenticate(
    cp: &ControlPlaneSnapshot,
    headers: &HeaderMap,
    query: Option<&str>,
) -> Result<Arc<KeyIdentity>, PipelineError> {
    crate::util::api_key::authenticate(&cp.keys_by_digest, headers, query)
        .cloned()
        .ok_or(PipelineError::Unauthorized)
}
