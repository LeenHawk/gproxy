//! Downstream-request log ops for the `db` backend (append-only).

use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder, QuerySelect, QueryTrait, Select,
};

use crate::store::persistence::records::{DownstreamRequest, DownstreamRequestInput};
use crate::store::persistence::{LogQuery, PageQuery, PageResult};

use crate::store::persistence::db::entities::logs::downstream_request;
use crate::store::persistence::db::entities::usage::usage;

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

/// Filtered rows across all requests, `id` DESC, keyset cursor `before_id`.
pub async fn query(
    conn: &DatabaseConnection,
    q: &LogQuery,
) -> anyhow::Result<Vec<DownstreamRequest>> {
    filtered(q, true)
        .order_by_desc(downstream_request::Column::Id)
        .limit(q.limit)
        .all(conn)
        .await?
        .into_iter()
        .map(to_record)
        .collect()
}

pub async fn query_page(
    conn: &DatabaseConnection,
    q: &LogQuery,
    page: &PageQuery,
) -> anyhow::Result<PageResult<DownstreamRequest>> {
    let total = filtered(q, false).count(conn).await?;
    let items = filtered(q, false)
        .order_by_desc(downstream_request::Column::Id)
        .offset(page.offset)
        .limit(page.limit)
        .all(conn)
        .await?
        .into_iter()
        .map(to_record)
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(PageResult { items, total })
}

fn filtered(q: &LogQuery, include_cursor: bool) -> Select<downstream_request::Entity> {
    use downstream_request::Column as D;
    use usage::Column as U;

    let mut sel = downstream_request::Entity::find();
    if let Some(v) = q.at_from {
        sel = sel.filter(D::At.gte(v));
    }
    if let Some(v) = q.at_to {
        sel = sel.filter(D::At.lte(v));
    }
    if include_cursor && let Some(v) = q.before_id {
        sel = sel.filter(D::Id.lt(v));
    }

    if q.provider_id.is_some() || q.user_id.is_some() || q.route_name.is_some() {
        let mut usages = usage::Entity::find().select_only().column(U::RequestId);
        if let Some(v) = q.provider_id {
            usages = usages.filter(U::ProviderId.eq(v));
        }
        if let Some(v) = q.user_id {
            usages = usages.filter(U::UserId.eq(v));
        }
        if let Some(ref v) = q.route_name {
            usages = usages.filter(U::RouteName.eq(v.clone()));
        }
        sel = sel.filter(D::RequestId.in_subquery(usages.into_query()));
    }

    sel
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
