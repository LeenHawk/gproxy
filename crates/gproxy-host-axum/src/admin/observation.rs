//! Instance-wide history. Every endpoint is gated independently of UI visibility.
use super::reply_sdk;
use crate::{HostState, error::ErrorResponse};
use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use gproxy_app::{AdminScope, dto::PortalUsageQuery};
use gproxy_sdk::dto::{HistoryDelete, HistoryDeleted, LogQuery, UsageQuery, UsageRecordQuery};
use gproxy_sdk::manage::LogSide;
use gproxy_seaorm::BatchConnectionTrait;

pub fn routes<C: BatchConnectionTrait + Send + Sync + 'static>() -> Router<HostState<C>> {
    Router::new()
        .route("/usage", get(usage::<C>))
        .route("/usage/records", get(records::<C>).delete(clear_usage::<C>))
        .route("/usage/records/delete", post(delete_usage::<C>))
        .route(
            "/logs/downstream",
            get(downstream::<C>).delete(clear_downstream::<C>),
        )
        .route("/logs/downstream/delete", post(delete_downstream::<C>))
        .route(
            "/logs/upstream",
            get(upstream::<C>).delete(clear_upstream::<C>),
        )
        .route("/logs/upstream/delete", post(delete_upstream::<C>))
        .route("/logs/downstream/{id}", get(detail::<C>))
        .route("/logs/captures/{id}", get(capture::<C>))
        .route_layer(axum::middleware::map_response(no_store))
}

async fn usage<C: BatchConnectionTrait + Send + Sync + 'static>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Query(query): Query<PortalUsageQuery>,
) -> Response {
    crate::send(async move {
        gate!("usage", scope);
        let filter = UsageQuery {
            from_ms: query.from_ms,
            to_ms: query.to_ms,
            user_id: query.user_id,
            api_key_id: query.api_key_id,
            model: query.model,
            operation: query.operation,
            provider_id: query.provider_id,
            credential_id: query.credential_id,
            max_scan_rows: None,
        };
        let handle = state.app().gproxy().query();
        let usage = handle.usage();
        let result = async {
            let (summary, groups, trend) = usage
                .aggregate(filter, query.group_by, query.bucket_ms)
                .await?;
            Ok(gproxy_app::dto::PortalUsageDto {
                from_ms: query.from_ms,
                to_ms: query.to_ms,
                summary,
                groups,
                trend,
            })
        }
        .await;
        reply_sdk(result)
    })
    .await
}

macro_rules! history {
    ($name:ident, $section:literal, $query:ty, $family:ident, $method:ident) => {
        async fn $name<C: BatchConnectionTrait + Send + Sync + 'static>(
            State(state): State<HostState<C>>,
            Extension(scope): Extension<AdminScope>,
            Query(query): Query<$query>,
        ) -> Response {
            crate::send(async move {
                gate!($section, scope);
                reply_sdk(state.app().gproxy().query().$family().$method(query).await)
            })
            .await
        }
    };
}
history!(records, "usage", UsageRecordQuery, usage, records);
history!(downstream, "logs", LogQuery, logs, list);
history!(upstream, "logs", LogQuery, logs, upstream);

macro_rules! log_detail {
    ($name:ident, $method:ident) => {
        async fn $name<C: BatchConnectionTrait + Send + Sync + 'static>(
            State(state): State<HostState<C>>,
            Extension(scope): Extension<AdminScope>,
            Path(id): Path<String>,
        ) -> Response {
            crate::send(async move {
                gate!("logs", scope);
                reply_sdk(state.app().gproxy().query().logs().$method(&id).await)
            })
            .await
        }
    };
}
log_detail!(detail, detail);
log_detail!(capture, capture);

// `POST …/delete` removes the named rows, `DELETE` on the listing removes all
// of them. Deleting history is gated by the same section that reads it.
async fn delete_usage<C: BatchConnectionTrait + Send + Sync + 'static>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Json(body): Json<HistoryDelete>,
) -> Response {
    crate::send(async move {
        gate!("usage", scope);
        let history = state.app().gproxy().manage().history();
        reply_sdk(history.delete_usage(Some(&body.ids)).await.map(deleted))
    })
    .await
}

async fn clear_usage<C: BatchConnectionTrait + Send + Sync + 'static>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
) -> Response {
    crate::send(async move {
        gate!("usage", scope);
        let history = state.app().gproxy().manage().history();
        reply_sdk(history.delete_usage(None).await.map(deleted))
    })
    .await
}

macro_rules! delete_logs {
    ($delete:ident, $clear:ident, $side:expr) => {
        async fn $delete<C: BatchConnectionTrait + Send + Sync + 'static>(
            State(state): State<HostState<C>>,
            Extension(scope): Extension<AdminScope>,
            Json(body): Json<HistoryDelete>,
        ) -> Response {
            crate::send(async move {
                gate!("logs", scope);
                let history = state.app().gproxy().manage().history();
                reply_sdk(
                    history
                        .delete_logs($side, Some(&body.ids))
                        .await
                        .map(deleted),
                )
            })
            .await
        }
        async fn $clear<C: BatchConnectionTrait + Send + Sync + 'static>(
            State(state): State<HostState<C>>,
            Extension(scope): Extension<AdminScope>,
        ) -> Response {
            crate::send(async move {
                gate!("logs", scope);
                let history = state.app().gproxy().manage().history();
                reply_sdk(history.delete_logs($side, None).await.map(deleted))
            })
            .await
        }
    };
}
delete_logs!(delete_downstream, clear_downstream, LogSide::Downstream);
delete_logs!(delete_upstream, clear_upstream, LogSide::Upstream);

fn deleted(deleted: u64) -> HistoryDeleted {
    HistoryDeleted { deleted }
}

async fn no_store(mut response: Response) -> Response {
    response.headers_mut().insert(
        http::header::CACHE_CONTROL,
        http::HeaderValue::from_static("no-store"),
    );
    response
}
