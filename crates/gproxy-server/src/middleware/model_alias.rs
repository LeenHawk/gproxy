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
}

/// Axum middleware: resolve model aliases.
///
/// If the request model matches an alias, stores `ResolvedAlias` in extensions
/// with the target provider and model.
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

    let resolved = model.as_deref().and_then(|m| state.resolve_model_alias(m));

    request.extensions_mut().insert(ResolvedAlias {
        provider_name: resolved.as_ref().map(|r| r.provider_name.clone()),
        model_id: resolved.as_ref().map(|r| r.model_id.clone()),
    });

    next.run(request).await
}
