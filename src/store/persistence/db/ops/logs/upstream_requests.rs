//! Upstream-request log ops for the `db` backend (append-only).

use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder,
    sea_query::Expr,
};

use crate::store::persistence::records::{UpstreamRequest, UpstreamRequestInput};

use crate::store::persistence::db::entities::logs::upstream_request;

fn to_record(m: upstream_request::Model) -> anyhow::Result<UpstreamRequest> {
    Ok(UpstreamRequest {
        id: m.id,
        request_id: m.request_id,
        at: m.at,
        provider_id: m.provider_id,
        credential_id: m.credential_id,
        url: m.url,
        method: m.method,
        status: m.status,
        latency_ms: m.latency_ms,
        headers_json: m
            .headers_json
            .map(|s| serde_json::from_str(&s))
            .transpose()?,
        body: m.body,
        response_body: m.response_body,
        created_at: m.created_at,
        updated_at: m.updated_at,
    })
}

pub async fn append(
    conn: &DatabaseConnection,
    input: UpstreamRequestInput,
) -> anyhow::Result<UpstreamRequest> {
    let now = crate::store::persistence::db::ops::now_secs();
    let headers = input
        .headers_json
        .map(|v| serde_json::to_string(&v))
        .transpose()?;

    let model = upstream_request::ActiveModel {
        id: NotSet,
        request_id: Set(input.request_id),
        at: Set(input.at),
        provider_id: Set(input.provider_id),
        credential_id: Set(input.credential_id),
        url: Set(input.url),
        method: Set(input.method),
        status: Set(input.status),
        latency_ms: Set(input.latency_ms),
        headers_json: Set(headers),
        body: Set(input.body),
        response_body: Set(input.response_body),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(conn)
    .await?;

    to_record(model)
}

pub async fn list(
    conn: &DatabaseConnection,
    request_id: &str,
) -> anyhow::Result<Vec<UpstreamRequest>> {
    upstream_request::Entity::find()
        .filter(upstream_request::Column::RequestId.eq(request_id))
        .order_by_asc(upstream_request::Column::Id)
        .all(conn)
        .await?
        .into_iter()
        .map(to_record)
        .collect()
}

/// Atomically backfill one captured transport call. No-op when either identity
/// field no longer matches (for example, retention removed and reused its id).
pub async fn update_response_body(
    conn: &DatabaseConnection,
    capture_id: i64,
    request_id: &str,
    response_body: Option<String>,
) -> anyhow::Result<()> {
    let now = crate::store::persistence::db::ops::now_secs();
    upstream_request::Entity::update_many()
        .col_expr(
            upstream_request::Column::ResponseBody,
            Expr::value(response_body),
        )
        .col_expr(upstream_request::Column::UpdatedAt, Expr::value(now))
        .filter(upstream_request::Column::Id.eq(capture_id))
        .filter(upstream_request::Column::RequestId.eq(request_id))
        .exec(conn)
        .await?;
    Ok(())
}
