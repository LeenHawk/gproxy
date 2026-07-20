//! Native framework adapter for `GET /metrics`.

use crate::app::AppState;

/// The router owns admin authentication; this adapter only obtains the
/// persistence snapshot before delegating response construction to `http::ops`.
pub async fn metrics(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> axum::response::Response {
    let aggregate = match state.persistence.metrics_aggregate().await {
        Ok(aggregate) => Some(aggregate),
        Err(error) => {
            tracing::warn!(error = %error, "metrics aggregate failed");
            None
        }
    };
    super::ops_response(crate::http::ops::metrics(aggregate.as_ref()))
}
