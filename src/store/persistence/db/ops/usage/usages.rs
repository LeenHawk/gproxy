//! Usage ops for the `db` backend (append-only, idempotent by `request_id`).

use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect,
};

use crate::store::persistence::UsageQuery;
use crate::store::persistence::records::{Usage, UsageInput};

use crate::store::persistence::db::entities::usage::usage;

fn to_record(m: usage::Model) -> anyhow::Result<Usage> {
    Ok(Usage {
        id: m.id,
        request_id: m.request_id,
        at: m.at,
        route_name: m.route_name,
        provider_id: m.provider_id,
        credential_id: m.credential_id,
        org_id: m.org_id,
        team_id: m.team_id,
        user_id: m.user_id,
        user_key_id: m.user_key_id,
        operation: m.operation,
        kind: m.kind,
        model: m.model,
        input_tokens: m.input_tokens,
        output_tokens: m.output_tokens,
        cache_read_tokens: m.cache_read_tokens,
        cache_creation_5m_tokens: m.cache_creation_5m_tokens,
        cache_creation_1h_tokens: m.cache_creation_1h_tokens,
        cost: m.cost.parse::<rust_decimal::Decimal>()?,
        latency_ms: m.latency_ms,
        usage_source: m.usage_source,
        ended: m.ended,
        created_at: m.created_at,
        updated_at: m.updated_at,
    })
}

/// Append a usage row; `Ok(None)` when a row with the same `request_id` already
/// exists (idempotent settle, §17). A unique index on `request_id` backs the
/// pre-insert check (a concurrent duplicate insert errors rather than dupes).
pub async fn append(conn: &DatabaseConnection, input: UsageInput) -> anyhow::Result<Option<Usage>> {
    let existing = usage::Entity::find()
        .filter(usage::Column::RequestId.eq(input.request_id.clone()))
        .one(conn)
        .await?;
    if existing.is_some() {
        return Ok(None);
    }

    let now = crate::store::persistence::db::ops::now_secs();
    let model = usage::ActiveModel {
        id: NotSet,
        request_id: Set(input.request_id),
        at: Set(input.at),
        route_name: Set(input.route_name),
        provider_id: Set(input.provider_id),
        credential_id: Set(input.credential_id),
        org_id: Set(input.org_id),
        team_id: Set(input.team_id),
        user_id: Set(input.user_id),
        user_key_id: Set(input.user_key_id),
        operation: Set(input.operation),
        kind: Set(input.kind),
        model: Set(input.model),
        input_tokens: Set(input.input_tokens),
        output_tokens: Set(input.output_tokens),
        cache_read_tokens: Set(input.cache_read_tokens),
        cache_creation_5m_tokens: Set(input.cache_creation_5m_tokens),
        cache_creation_1h_tokens: Set(input.cache_creation_1h_tokens),
        cost: Set(input.cost.to_string()),
        latency_ms: Set(input.latency_ms),
        usage_source: Set(input.usage_source),
        ended: Set(input.ended),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(conn)
    .await?;
    to_record(model).map(Some)
}

pub async fn list(conn: &DatabaseConnection, limit: u64) -> anyhow::Result<Vec<Usage>> {
    usage::Entity::find()
        .order_by_desc(usage::Column::Id)
        .limit(limit)
        .all(conn)
        .await?
        .into_iter()
        .map(to_record)
        .collect()
}

/// Filtered + keyset-paginated usage rows (B4). Mirrors the rollup filter chain
/// (gte/lte on `at`, eq on the dimensions, `id < before_id` cursor), ordered
/// `id` DESC.
pub async fn query(conn: &DatabaseConnection, q: &UsageQuery) -> anyhow::Result<Vec<Usage>> {
    use usage::Column as C;
    let mut sel = usage::Entity::find();
    if let Some(v) = q.at_from {
        sel = sel.filter(C::At.gte(v));
    }
    if let Some(v) = q.at_to {
        sel = sel.filter(C::At.lte(v));
    }
    if let Some(v) = q.provider_id {
        sel = sel.filter(C::ProviderId.eq(v));
    }
    if let Some(v) = q.user_id {
        sel = sel.filter(C::UserId.eq(v));
    }
    if let Some(ref v) = q.route_name {
        sel = sel.filter(C::RouteName.eq(v.clone()));
    }
    if let Some(ref v) = q.model {
        sel = sel.filter(C::Model.eq(v.clone()));
    }
    if let Some(v) = q.before_id {
        sel = sel.filter(C::Id.lt(v));
    }
    sel.order_by_desc(C::Id)
        .limit(q.limit)
        .all(conn)
        .await?
        .into_iter()
        .map(to_record)
        .collect()
}
