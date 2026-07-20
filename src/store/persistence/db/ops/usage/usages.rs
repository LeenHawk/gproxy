//! Usage ops for the `db` backend (append-only, idempotent by `request_id`).

use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::sea_query::Value;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseBackend, DatabaseConnection,
    EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Select, Statement,
};

use crate::store::persistence::records::{Usage, UsageInput, UsageSummary};
use crate::store::persistence::{PageQuery, PageResult, UsageQuery};

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
        cache_creation_30m_tokens: m.cache_creation_30m_tokens,
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
        cache_creation_30m_tokens: Set(input.cache_creation_30m_tokens),
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
    filtered(q, true)
        .order_by_desc(usage::Column::Id)
        .limit(q.limit)
        .all(conn)
        .await?
        .into_iter()
        .map(to_record)
        .collect()
}

pub async fn query_page(
    conn: &DatabaseConnection,
    q: &UsageQuery,
    page: &PageQuery,
) -> anyhow::Result<PageResult<Usage>> {
    let total = filtered(q, false).count(conn).await?;
    let items = filtered(q, false)
        .order_by_desc(usage::Column::Id)
        .offset(page.offset)
        .limit(page.limit)
        .all(conn)
        .await?
        .into_iter()
        .map(to_record)
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(PageResult { items, total })
}

fn filtered(q: &UsageQuery, include_cursor: bool) -> Select<usage::Entity> {
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
    if include_cursor && let Some(v) = q.before_id {
        sel = sel.filter(C::Id.lt(v));
    }
    sel
}

fn push_summary_filter(
    sql: &mut String,
    values: &mut Vec<Value>,
    backend: DatabaseBackend,
    column: &str,
    operator: &str,
    value: Value,
) {
    let placeholder = if backend == DatabaseBackend::Postgres {
        format!("${}", values.len() + 1)
    } else {
        "?".to_owned()
    };
    sql.push_str(&format!(" AND {column} {operator} {placeholder}"));
    values.push(value);
}

/// Backend-side full-result aggregate for the usage explorer. Values remain
/// bound parameters; only fixed column/operator names are interpolated.
pub async fn summarize(conn: &DatabaseConnection, q: &UsageQuery) -> anyhow::Result<UsageSummary> {
    let backend = conn.get_database_backend();
    let cost_expr = match backend {
        DatabaseBackend::MySql => "CAST(COALESCE(SUM(CAST(cost AS DECIMAL(65, 30))), 0) AS CHAR)",
        DatabaseBackend::Postgres | DatabaseBackend::Sqlite => {
            "CAST(COALESCE(SUM(CAST(cost AS NUMERIC)), 0) AS TEXT)"
        }
        _ => "CAST(COALESCE(SUM(CAST(cost AS NUMERIC)), 0) AS TEXT)",
    };
    let mut sql = format!(
        "SELECT COUNT(*) AS requests, \
         CAST(COALESCE(SUM(input_tokens), 0) AS BIGINT) AS input_tokens, \
         CAST(COALESCE(SUM(output_tokens), 0) AS BIGINT) AS output_tokens, \
         CAST(COALESCE(SUM(cache_read_tokens), 0) AS BIGINT) AS cache_read_tokens, \
         CAST(COALESCE(SUM(cache_creation_5m_tokens), 0) AS BIGINT) AS cache_creation_5m_tokens, \
         CAST(COALESCE(SUM(cache_creation_30m_tokens), 0) AS BIGINT) AS cache_creation_30m_tokens, \
         CAST(COALESCE(SUM(cache_creation_1h_tokens), 0) AS BIGINT) AS cache_creation_1h_tokens, \
         {cost_expr} AS cost FROM usages WHERE 1=1"
    );
    let mut values = Vec::new();
    if let Some(v) = q.at_from {
        push_summary_filter(&mut sql, &mut values, backend, "at", ">=", v.into());
    }
    if let Some(v) = q.at_to {
        push_summary_filter(&mut sql, &mut values, backend, "at", "<=", v.into());
    }
    if let Some(v) = q.provider_id {
        push_summary_filter(&mut sql, &mut values, backend, "provider_id", "=", v.into());
    }
    if let Some(v) = q.user_id {
        push_summary_filter(&mut sql, &mut values, backend, "user_id", "=", v.into());
    }
    if let Some(ref v) = q.route_name {
        push_summary_filter(
            &mut sql,
            &mut values,
            backend,
            "route_name",
            "=",
            v.clone().into(),
        );
    }
    if let Some(ref v) = q.model {
        push_summary_filter(
            &mut sql,
            &mut values,
            backend,
            "model",
            "=",
            v.clone().into(),
        );
    }

    let row = conn
        .query_one_raw(Statement::from_sql_and_values(backend, sql, values))
        .await?
        .ok_or_else(|| anyhow::anyhow!("usage summary query returned no row"))?;
    Ok(UsageSummary {
        requests: row.try_get("", "requests")?,
        input_tokens: row.try_get("", "input_tokens")?,
        output_tokens: row.try_get("", "output_tokens")?,
        cache_read_tokens: row.try_get("", "cache_read_tokens")?,
        cache_creation_5m_tokens: row.try_get("", "cache_creation_5m_tokens")?,
        cache_creation_30m_tokens: row.try_get("", "cache_creation_30m_tokens")?,
        cache_creation_1h_tokens: row.try_get("", "cache_creation_1h_tokens")?,
        cost: row.try_get::<String>("", "cost")?.parse()?,
    })
}
