use std::sync::Arc;

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
pub use gproxy_core::ModelAliasTarget;

use crate::app_state::AppState;

/// Resolved model alias stored in request extensions.
/// `None` means no alias matched — use original model name.
#[derive(Debug, Clone)]
pub struct ResolvedAlias {
    pub provider_name: Option<String>,
    pub model_id: Option<String>,
    /// Suffix stripped during alias resolution (e.g. `"-fast"`).
    /// When present, the handler should append it back to the resolved model_id.
    pub suffix: Option<String>,
}

/// Axum middleware: resolve model aliases.
///
/// If the request model matches an alias, stores `ResolvedAlias` in extensions
/// with the target provider and model. Supports suffix-aware resolution:
/// if exact alias lookup fails, tries stripping known suffixes and retrying.
pub async fn model_alias_middleware(
    State(state): State<Arc<AppState>>,
    mut request: Request,
    next: Next,
) -> Response {
    // Try to get model from extensions (set by request_model middleware)
    let model = request
        .extensions()
        .get::<super::request_model::ExtractedModel>()
        .and_then(|m| m.0.clone());

    let (resolved, suffix) = if let Some(ref m) = model {
        // Try exact alias match first.
        if let Some(r) = state.resolve_model_alias(m) {
            (Some(r), None)
        } else if let Some((base, suffix)) =
            gproxy_sdk::provider::suffix::strip_any_suffix(m)
        {
            // Try alias resolution on the base model (suffix stripped).
            if let Some(r) = state.resolve_model_alias(base) {
                (Some(r), Some(suffix.to_string()))
            } else {
                (None, None)
            }
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };

    request.extensions_mut().insert(ResolvedAlias {
        provider_name: resolved.as_ref().map(|r| r.provider_name.clone()),
        model_id: resolved.as_ref().map(|r| r.model_id.clone()),
        suffix,
    });

    next.run(request).await
}
