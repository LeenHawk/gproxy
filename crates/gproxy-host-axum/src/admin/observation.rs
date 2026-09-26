//! Instance-wide history. Every endpoint is gated independently of UI visibility.
use super::reply_sdk;
use crate::{HostState, error::ErrorResponse};
use axum::{
    Extension, Router,
    extract::{Path, Query, State},
    response::{IntoResponse, Response},
    routing::get,
};
use gproxy_app::{AdminScope, dto::PortalUsageQuery};
use gproxy_sdk::dto::{LogQuery, UsageGroupQuery, UsageQuery, UsageRecordQuery, UsageTrendQuery};
use gproxy_seaorm::BatchConnectionTrait;

pub fn routes<C: BatchConnectionTrait + Send + Sync + 'static>() -> Router<HostState<C>> {
    Router::new()
        .route("/usage", get(usage::<C>))
        .route("/usage/records", get(records::<C>))
        .route("/logs/downstream", get(downstream::<C>))
        .route("/logs/upstream", get(upstream::<C>))
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
            let summary = usage.summary(filter.clone()).await?;
            let groups = match query.group_by {
                Some(group_by) => {
                    usage
                        .group(UsageGroupQuery {
                            filter: filter.clone(),
                            group_by,
                        })
                        .await?
                }
                None => Vec::new(),
            };
            let trend = match query.bucket_ms {
                Some(bucket_ms) => usage.trend(UsageTrendQuery { filter, bucket_ms }).await?,
                None => Vec::new(),
            };
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

async fn no_store(mut response: Response) -> Response {
    response.headers_mut().insert(
        http::header::CACHE_CONTROL,
        http::HeaderValue::from_static("no-store"),
    );
    response
}
