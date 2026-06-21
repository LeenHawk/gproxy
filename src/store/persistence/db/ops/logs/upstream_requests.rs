//! Upstream-request log ops for the `db` backend (append-only).

use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

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
        .all(conn)
        .await?
        .into_iter()
        .map(to_record)
        .collect()
}

/// Backfill `response_body` (and `updated_at`) on rows matching `request_id`.
/// No-op when no row matches. Used by streaming responses that settle after the
/// row was appended.
pub async fn update_response_body(
    conn: &DatabaseConnection,
    request_id: &str,
    response_body: Option<String>,
) -> anyhow::Result<()> {
    let now = crate::store::persistence::db::ops::now_secs();
    if let Some(m) = upstream_request::Entity::find()
        .filter(upstream_request::Column::RequestId.eq(request_id))
        .one(conn)
        .await?
    {
        let mut am: upstream_request::ActiveModel = m.into();
        am.response_body = Set(response_body);
        am.updated_at = Set(now);
        am.update(conn).await?;
    }
    Ok(())
}
