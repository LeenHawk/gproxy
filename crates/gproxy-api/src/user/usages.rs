use crate::auth::SessionUser;
use crate::error::HttpError;
use axum::Json;
use axum::extract::{Extension, State};
use gproxy_server::AppState;
use gproxy_storage::{Scope, UsageQuery, UsageQueryCount, UsageQueryRow};
use std::sync::Arc;

pub async fn query_usages(
    State(state): State<Arc<AppState>>,
    Extension(session): Extension<SessionUser>,
    Json(mut query): Json<UsageQuery>,
) -> Result<Json<Vec<UsageQueryRow>>, HttpError> {
    let user_id = session.user_id;
    query.user_id = Scope::Eq(user_id);
    let rows = state.storage().query_usages(&query).await?;
    Ok(Json(rows))
}

pub async fn count_usages(
    State(state): State<Arc<AppState>>,
    Extension(session): Extension<SessionUser>,
    Json(mut query): Json<UsageQuery>,
) -> Result<Json<UsageQueryCount>, HttpError> {
    let user_id = session.user_id;
    query.user_id = Scope::Eq(user_id);
    let count = state.storage().count_usages(&query).await?;
    Ok(Json(count))
}
