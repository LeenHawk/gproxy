//! Downstream-request log ops for the `db` backend (append-only).

use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::store::persistence::records::{DownstreamRequest, DownstreamRequestInput};

use crate::store::persistence::db::entities::logs::downstream_request;

fn to_record(m: downstream_request::Model) -> anyhow::Result<DownstreamRequest> {
    Ok(DownstreamRequest {
        id: m.id,
        request_id: m.request_id,
        at: m.at,
        method: m.method,
        path: m.path,
        query: m.query,
        status: m.status,
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
    input: DownstreamRequestInput,
) -> anyhow::Result<DownstreamRequest> {
    let now = crate::store::persistence::db::ops::now_secs();
    let headers = input
        .headers_json
        .map(|v| serde_json::to_string(&v))
        .transpose()?;

    let model = downstream_request::ActiveModel {
        id: NotSet,
        request_id: Set(input.request_id),
        at: Set(input.at),
        method: Set(input.method),
        path: Set(input.path),
        query: Set(input.query),
        status: Set(input.status),
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
) -> anyhow::Result<Vec<DownstreamRequest>> {
    downstream_request::Entity::find()
        .filter(downstream_request::Column::RequestId.eq(request_id))
        .all(conn)
        .await?
        .into_iter()
        .map(to_record)
        .collect()
}

/// Recent rows across all requests, `id` DESC, keyset cursor `before_id`.
pub async fn list_recent(
    conn: &DatabaseConnection,
    limit: u64,
    before_id: Option<i64>,
) -> anyhow::Result<Vec<DownstreamRequest>> {
    use sea_orm::{QueryOrder, QuerySelect};
    let mut sel = downstream_request::Entity::find();
    if let Some(v) = before_id {
        sel = sel.filter(downstream_request::Column::Id.lt(v));
    }
    sel.order_by_desc(downstream_request::Column::Id)
        .limit(limit)
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
    if let Some(m) = downstream_request::Entity::find()
        .filter(downstream_request::Column::RequestId.eq(request_id))
        .one(conn)
        .await?
    {
        let mut am: downstream_request::ActiveModel = m.into();
        am.response_body = Set(response_body);
        am.updated_at = Set(now);
        am.update(conn).await?;
    }
    Ok(())
}
