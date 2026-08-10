//! Permanent credential daily aggregates and upstream quota-cycle history.

use std::collections::BTreeMap;

use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::sea_query::Value;
use sea_orm::sea_query::{Expr, ExprTrait, OnConflict};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseBackend, DatabaseConnection,
    EntityTrait, QueryFilter, QueryOrder, QuerySelect, Select, Statement,
};

use crate::store::persistence::db::entities::usage::{
    credential_quota_cycle as cycle, credential_quota_cycle_model as cycle_model,
    credential_usage_daily as daily,
};
use crate::store::persistence::db::ops::is_unique_violation;
use crate::store::persistence::records::{
    CredentialQuotaCycle, CredentialQuotaCycleInput, CredentialQuotaCycleModel,
    CredentialQuotaCycleModelInput, CredentialUsageDaily, CredentialUsageDailyInput,
};
use crate::store::persistence::{CredentialQuotaCycleQuery, CredentialUsageDailyQuery};

fn daily_record(m: daily::Model) -> anyhow::Result<CredentialUsageDaily> {
    Ok(CredentialUsageDaily {
        id: m.id,
        day_start: m.day_start,
        credential_id: m.credential_id,
        provider_id: m.provider_id,
        model: m.model,
        requests: m.requests,
        input_tokens: m.input_tokens,
        output_tokens: m.output_tokens,
        image_output_tokens: m.image_output_tokens,
        cache_read_tokens: m.cache_read_tokens,
        cache_creation_5m_tokens: m.cache_creation_5m_tokens,
        cache_creation_30m_tokens: m.cache_creation_30m_tokens,
        cache_creation_1h_tokens: m.cache_creation_1h_tokens,
        cost: m.cost.parse()?,
        created_at: m.created_at,
        updated_at: m.updated_at,
    })
}

fn normalized_model(model: Option<&str>) -> Option<String> {
    model
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(str::to_owned)
}

fn normalized_cycle_model(model: &str) -> String {
    normalized_model(Some(model)).unwrap_or_else(|| "unknown".to_owned())
}

fn daily_bucket(input: &CredentialUsageDailyInput) -> Select<daily::Entity> {
    daily::Entity::find()
        .filter(daily::Column::DayStart.eq(input.day_start))
        .filter(daily::Column::CredentialId.eq(input.credential_id))
        .filter(match normalized_model(input.model.as_deref()) {
            Some(model) => daily::Column::Model.eq(model),
            None => daily::Column::Model.is_null(),
        })
}

fn new_daily(input: &CredentialUsageDailyInput, now: i64) -> daily::ActiveModel {
    daily::ActiveModel {
        id: NotSet,
        day_start: Set(input.day_start),
        credential_id: Set(input.credential_id),
        provider_id: Set(input.provider_id),
        model: Set(normalized_model(input.model.as_deref())),
        requests: Set(input.requests),
        input_tokens: Set(input.input_tokens),
        output_tokens: Set(input.output_tokens),
        image_output_tokens: Set(input.image_output_tokens),
        cache_read_tokens: Set(input.cache_read_tokens),
        cache_creation_5m_tokens: Set(input.cache_creation_5m_tokens),
        cache_creation_30m_tokens: Set(input.cache_creation_30m_tokens),
        cache_creation_1h_tokens: Set(input.cache_creation_1h_tokens),
        cost: Set(input.cost.to_string()),
        created_at: Set(now),
        updated_at: Set(now),
    }
}

/// Replace a daily aggregate (used by reconciliation/backfill).
pub async fn upsert_daily(
    conn: &DatabaseConnection,
    input: CredentialUsageDailyInput,
) -> anyhow::Result<CredentialUsageDaily> {
    const RETRIES: u32 = 5;
    let now = crate::store::persistence::db::ops::now_secs();
    for _ in 0..RETRIES {
        if let Some(existing) = daily_bucket(&input).one(conn).await? {
            let mut am: daily::ActiveModel = existing.into();
            am.provider_id = Set(input.provider_id);
            am.requests = Set(input.requests);
            am.input_tokens = Set(input.input_tokens);
            am.output_tokens = Set(input.output_tokens);
            am.image_output_tokens = Set(input.image_output_tokens);
            am.cache_read_tokens = Set(input.cache_read_tokens);
            am.cache_creation_5m_tokens = Set(input.cache_creation_5m_tokens);
            am.cache_creation_30m_tokens = Set(input.cache_creation_30m_tokens);
            am.cache_creation_1h_tokens = Set(input.cache_creation_1h_tokens);
            am.cost = Set(input.cost.to_string());
            am.updated_at = Set(now);
            return daily_record(am.update(conn).await?);
        }
        match new_daily(&input, now).insert(conn).await {
            Ok(model) => return daily_record(model),
            Err(e) if is_unique_violation(&e) => continue,
            Err(e) => return Err(e.into()),
        }
    }
    anyhow::bail!("credential_usage_daily upsert: persistent write contention")
}

/// Atomically add one settled request to a permanent daily bucket.
pub async fn add_daily(
    conn: &DatabaseConnection,
    input: CredentialUsageDailyInput,
) -> anyhow::Result<CredentialUsageDaily> {
    const RETRIES: u32 = 5;
    let now = crate::store::persistence::db::ops::now_secs();
    for _ in 0..RETRIES {
        let Some(existing) = daily_bucket(&input).one(conn).await? else {
            match new_daily(&input, now).insert(conn).await {
                Ok(model) => return daily_record(model),
                Err(e) if is_unique_violation(&e) => continue,
                Err(e) => return Err(e.into()),
            }
        };
        let next_cost = existing.cost.parse::<rust_decimal::Decimal>()? + input.cost;
        let res = daily::Entity::update_many()
            .col_expr(
                daily::Column::Requests,
                Expr::col(daily::Column::Requests).add(input.requests),
            )
            .col_expr(
                daily::Column::InputTokens,
                Expr::col(daily::Column::InputTokens).add(input.input_tokens),
            )
            .col_expr(
                daily::Column::OutputTokens,
                Expr::col(daily::Column::OutputTokens).add(input.output_tokens),
            )
            .col_expr(
                daily::Column::ImageOutputTokens,
                Expr::col(daily::Column::ImageOutputTokens).add(input.image_output_tokens),
            )
            .col_expr(
                daily::Column::CacheReadTokens,
                Expr::col(daily::Column::CacheReadTokens).add(input.cache_read_tokens),
            )
            .col_expr(
                daily::Column::CacheCreation5mTokens,
                Expr::col(daily::Column::CacheCreation5mTokens).add(input.cache_creation_5m_tokens),
            )
            .col_expr(
                daily::Column::CacheCreation30mTokens,
                Expr::col(daily::Column::CacheCreation30mTokens)
                    .add(input.cache_creation_30m_tokens),
            )
            .col_expr(
                daily::Column::CacheCreation1hTokens,
                Expr::col(daily::Column::CacheCreation1hTokens).add(input.cache_creation_1h_tokens),
            )
            .col_expr(daily::Column::Cost, Expr::value(next_cost.to_string()))
            .col_expr(daily::Column::ProviderId, Expr::value(input.provider_id))
            .col_expr(daily::Column::UpdatedAt, Expr::value(now))
            .filter(daily::Column::Id.eq(existing.id))
            .filter(daily::Column::Cost.eq(existing.cost))
            .exec(conn)
            .await?;
        if res.rows_affected > 0 {
            return daily::Entity::find_by_id(existing.id)
                .one(conn)
                .await?
                .ok_or_else(|| anyhow::anyhow!("credential daily bucket vanished"))
                .and_then(daily_record);
        }
    }
    anyhow::bail!("credential_usage_daily add: persistent write contention")
}

pub async fn query_daily(
    conn: &DatabaseConnection,
    q: &CredentialUsageDailyQuery,
) -> anyhow::Result<Vec<CredentialUsageDaily>> {
    let mut sel = daily::Entity::find();
    if let Some(v) = q.credential_id {
        sel = sel.filter(daily::Column::CredentialId.eq(v));
    }
    if let Some(v) = q.provider_id {
        sel = sel.filter(daily::Column::ProviderId.eq(v));
    }
    if let Some(v) = q.from {
        sel = sel.filter(daily::Column::DayStart.gte(v));
    }
    if let Some(v) = q.to {
        sel = sel.filter(daily::Column::DayStart.lte(v));
    }
    sel.order_by_asc(daily::Column::DayStart)
        .order_by_asc(daily::Column::Id)
        .all(conn)
        .await?
        .into_iter()
        .map(daily_record)
        .collect()
}

/// Rebuild every retained raw-usage bucket in the inclusive timestamp range.
/// Existing permanent buckets with no retained raw rows are intentionally left
/// untouched: they may represent data already removed by retention.
pub async fn reconcile_daily(
    conn: &DatabaseConnection,
    from: Option<i64>,
    to: Option<i64>,
) -> anyhow::Result<u64> {
    let backend = conn.get_database_backend();
    let day_expr = match backend {
        DatabaseBackend::MySql => "(at DIV 86400) * 86400",
        _ => "(at / 86400) * 86400",
    };
    let cost_expr = match backend {
        DatabaseBackend::MySql => "CAST(COALESCE(SUM(CAST(cost AS DECIMAL(65, 30))), 0) AS CHAR)",
        _ => "CAST(COALESCE(SUM(CAST(cost AS NUMERIC)), 0) AS TEXT)",
    };
    let mut sql = format!(
        "SELECT {day_expr} AS day_start, credential_id, MAX(provider_id) AS provider_id, \
         NULLIF(TRIM(model), '') AS model, COUNT(*) AS requests, \
         CAST(SUM(input_tokens) AS BIGINT) AS input_tokens, \
         CAST(SUM(output_tokens) AS BIGINT) AS output_tokens, \
         CAST(SUM(image_output_tokens) AS BIGINT) AS image_output_tokens, \
         CAST(SUM(cache_read_tokens) AS BIGINT) AS cache_read_tokens, \
         CAST(SUM(cache_creation_5m_tokens) AS BIGINT) AS cache_creation_5m_tokens, \
         CAST(SUM(cache_creation_30m_tokens) AS BIGINT) AS cache_creation_30m_tokens, \
         CAST(SUM(cache_creation_1h_tokens) AS BIGINT) AS cache_creation_1h_tokens, \
         {cost_expr} AS cost FROM usages \
         WHERE credential_id IS NOT NULL AND provider_id IS NOT NULL"
    );
    let mut values = Vec::<Value>::new();
    let mut push = |column: &str, value: i64| {
        let placeholder = if backend == DatabaseBackend::Postgres {
            format!("${}", values.len() + 1)
        } else {
            "?".to_owned()
        };
        sql.push_str(&format!(" AND {column} {placeholder}"));
        values.push(value.into());
    };
    if let Some(value) = from {
        push("at >=", value);
    }
    if let Some(value) = to {
        push("at <=", value);
    }
    sql.push_str(&format!(
        " GROUP BY {day_expr}, credential_id, NULLIF(TRIM(model), '') ORDER BY {day_expr}"
    ));
    let rows = conn
        .query_all_raw(Statement::from_sql_and_values(backend, sql, values))
        .await?;
    let exact_costs = if backend == DatabaseBackend::Sqlite {
        sqlite_daily_costs(conn, from, to).await?
    } else {
        BTreeMap::new()
    };
    let count = u64::try_from(rows.len())?;
    for row in rows {
        let input = CredentialUsageDailyInput {
            day_start: row.try_get("", "day_start")?,
            credential_id: row.try_get("", "credential_id")?,
            provider_id: row.try_get("", "provider_id")?,
            model: normalized_model(row.try_get::<Option<String>>("", "model")?.as_deref()),
            requests: row.try_get("", "requests")?,
            input_tokens: row.try_get("", "input_tokens")?,
            output_tokens: row.try_get("", "output_tokens")?,
            image_output_tokens: row.try_get("", "image_output_tokens")?,
            cache_read_tokens: row.try_get("", "cache_read_tokens")?,
            cache_creation_5m_tokens: row.try_get("", "cache_creation_5m_tokens")?,
            cache_creation_30m_tokens: row.try_get("", "cache_creation_30m_tokens")?,
            cache_creation_1h_tokens: row.try_get("", "cache_creation_1h_tokens")?,
            cost: exact_costs
                .get(&(
                    row.try_get("", "day_start")?,
                    row.try_get("", "credential_id")?,
                    row.try_get("", "model")?,
                ))
                .copied()
                .unwrap_or(row.try_get::<String>("", "cost")?.parse()?),
        };
        upsert_daily(conn, input).await?;
    }
    Ok(count)
}

/// SQLite's NUMERIC SUM can pass through a floating representation. Read the
/// retained decimal strings once and sum them in Rust so the v23 upgrade
/// preserves monetary values exactly.
async fn sqlite_daily_costs(
    conn: &DatabaseConnection,
    from: Option<i64>,
    to: Option<i64>,
) -> anyhow::Result<BTreeMap<(i64, i64, Option<String>), rust_decimal::Decimal>> {
    let mut sql = String::from(
        "SELECT at, credential_id, model, cost FROM usages \
         WHERE credential_id IS NOT NULL AND provider_id IS NOT NULL",
    );
    let mut values = Vec::<Value>::new();
    if let Some(v) = from {
        sql.push_str(" AND at >= ?");
        values.push(v.into());
    }
    if let Some(v) = to {
        sql.push_str(" AND at <= ?");
        values.push(v.into());
    }
    let mut totals = BTreeMap::new();
    for row in conn
        .query_all_raw(Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            sql,
            values,
        ))
        .await?
    {
        let at: i64 = row.try_get("", "at")?;
        let key = (
            at - at.rem_euclid(86_400),
            row.try_get("", "credential_id")?,
            normalized_model(row.try_get::<Option<String>>("", "model")?.as_deref()),
        );
        let cost = row
            .try_get::<String>("", "cost")?
            .parse::<rust_decimal::Decimal>()?;
        *totals.entry(key).or_insert(rust_decimal::Decimal::ZERO) += cost;
    }
    Ok(totals)
}

fn cycle_record(m: cycle::Model) -> anyhow::Result<CredentialQuotaCycle> {
    Ok(CredentialQuotaCycle {
        id: m.id,
        credential_id: m.credential_id,
        provider_id: m.provider_id,
        channel: m.channel,
        window_key: m.window_key,
        name: m.name,
        label: m.label,
        scope_kind: m.scope_kind,
        scope_json: m.scope_json.map(|v| serde_json::from_str(&v)).transpose()?,
        meter_kind: m.meter_kind,
        period_start: m.period_start,
        period_end: m.period_end,
        boundary_source: m.boundary_source,
        boundary_confidence: m.boundary_confidence,
        close_reason: m.close_reason,
        status: m.status,
        last_observed_at: m.last_observed_at,
        used_percent: m.used_percent.map(|v| v.parse()).transpose()?,
        upstream_used: m.upstream_used.map(|v| v.parse()).transpose()?,
        upstream_limit: m.upstream_limit.map(|v| v.parse()).transpose()?,
        coverage: m.coverage,
        requests: m.requests,
        input_tokens: m.input_tokens,
        output_tokens: m.output_tokens,
        image_output_tokens: m.image_output_tokens,
        cache_read_tokens: m.cache_read_tokens,
        cache_creation_5m_tokens: m.cache_creation_5m_tokens,
        cache_creation_30m_tokens: m.cache_creation_30m_tokens,
        cache_creation_1h_tokens: m.cache_creation_1h_tokens,
        cost: m.cost.parse()?,
        estimated_tokens: m.estimated_tokens,
        estimated_cost: m.estimated_cost.map(|v| v.parse()).transpose()?,
        aggregated_through: m.aggregated_through,
        finalized_at: m.finalized_at,
        created_at: m.created_at,
        updated_at: m.updated_at,
    })
}

pub async fn get_cycle(
    conn: &DatabaseConnection,
    id: i64,
) -> anyhow::Result<Option<CredentialQuotaCycle>> {
    cycle::Entity::find_by_id(id)
        .one(conn)
        .await?
        .map(cycle_record)
        .transpose()
}

pub async fn get_open_cycle(
    conn: &DatabaseConnection,
    credential_id: i64,
    window_key: &str,
) -> anyhow::Result<Option<CredentialQuotaCycle>> {
    cycle::Entity::find()
        .filter(cycle::Column::CredentialId.eq(credential_id))
        .filter(cycle::Column::WindowKey.eq(window_key))
        .filter(cycle::Column::OpenSlot.eq(1))
        .one(conn)
        .await?
        .map(cycle_record)
        .transpose()
}

fn apply_cycle_input(
    am: &mut cycle::ActiveModel,
    input: &CredentialQuotaCycleInput,
    scope_json: Option<String>,
    now: i64,
) {
    am.provider_id = Set(input.provider_id);
    am.channel = Set(input.channel.clone());
    am.name = Set(input.name.clone());
    am.label = Set(input.label.clone());
    am.scope_kind = Set(input.scope_kind.clone());
    am.scope_json = Set(scope_json);
    am.meter_kind = Set(input.meter_kind.clone());
    am.period_start = Set(input.period_start);
    am.period_end = Set(input.period_end);
    am.boundary_source = Set(input.boundary_source.clone());
    am.boundary_confidence = Set(input.boundary_confidence.clone());
    am.last_observed_at = Set(input.last_observed_at);
    am.used_percent = Set(input.used_percent.map(|v| v.to_string()));
    am.upstream_used = Set(input.upstream_used.map(|v| v.to_string()));
    am.upstream_limit = Set(input.upstream_limit.map(|v| v.to_string()));
    am.coverage = Set(input.coverage.clone());
    am.requests = Set(input.requests);
    am.input_tokens = Set(input.input_tokens);
    am.output_tokens = Set(input.output_tokens);
    am.image_output_tokens = Set(input.image_output_tokens);
    am.cache_read_tokens = Set(input.cache_read_tokens);
    am.cache_creation_5m_tokens = Set(input.cache_creation_5m_tokens);
    am.cache_creation_30m_tokens = Set(input.cache_creation_30m_tokens);
    am.cache_creation_1h_tokens = Set(input.cache_creation_1h_tokens);
    am.cost = Set(input.cost.to_string());
    am.estimated_tokens = Set(input.estimated_tokens);
    am.estimated_cost = Set(input.estimated_cost.map(|v| v.to_string()));
    am.aggregated_through = Set(input.aggregated_through);
    am.updated_at = Set(now);
}

pub async fn upsert_cycle(
    conn: &DatabaseConnection,
    input: CredentialQuotaCycleInput,
) -> anyhow::Result<CredentialQuotaCycle> {
    const RETRIES: u32 = 5;
    let now = crate::store::persistence::db::ops::now_secs();
    let scope_json = input
        .scope_json
        .as_ref()
        .map(serde_json::to_string)
        .transpose()?;
    for _ in 0..RETRIES {
        if let Some(existing) = cycle::Entity::find()
            .filter(cycle::Column::CredentialId.eq(input.credential_id))
            .filter(cycle::Column::WindowKey.eq(input.window_key.clone()))
            .filter(cycle::Column::OpenSlot.eq(1))
            .one(conn)
            .await?
        {
            let id = existing.id;
            let mut input = input.clone();
            input.preserve_monotonic_local(&cycle_record(existing.clone())?);
            let mut am: cycle::ActiveModel = existing.into();
            apply_cycle_input(&mut am, &input, scope_json.clone(), now);
            let update = cycle::Entity::update(am)
                .validate()?
                .filter(cycle::Column::OpenSlot.eq(1))
                .exec_without_returning(conn)
                .await;
            match update {
                Ok(_) => {
                    return get_cycle(conn, id)
                        .await?
                        .ok_or_else(|| anyhow::anyhow!("quota cycle vanished after update"));
                }
                Err(sea_orm::DbErr::RecordNotUpdated) => {
                    return get_cycle(conn, id)
                        .await?
                        .ok_or_else(|| anyhow::anyhow!("quota cycle vanished after finalize"));
                }
                Err(err) => return Err(err.into()),
            }
        }
        let mut am = cycle::ActiveModel {
            id: NotSet,
            credential_id: Set(input.credential_id),
            provider_id: Set(input.provider_id),
            channel: Set(input.channel.clone()),
            window_key: Set(input.window_key.clone()),
            name: Set(input.name.clone()),
            label: Set(input.label.clone()),
            scope_kind: Set(input.scope_kind.clone()),
            scope_json: Set(scope_json.clone()),
            meter_kind: Set(input.meter_kind.clone()),
            period_start: Set(input.period_start),
            period_end: Set(input.period_end),
            boundary_source: Set(input.boundary_source.clone()),
            boundary_confidence: Set(input.boundary_confidence.clone()),
            close_reason: Set(None),
            status: Set("open".to_owned()),
            open_slot: Set(Some(1)),
            last_observed_at: Set(input.last_observed_at),
            used_percent: Set(input.used_percent.map(|v| v.to_string())),
            upstream_used: Set(input.upstream_used.map(|v| v.to_string())),
            upstream_limit: Set(input.upstream_limit.map(|v| v.to_string())),
            coverage: Set(input.coverage.clone()),
            requests: Set(input.requests),
            input_tokens: Set(input.input_tokens),
            output_tokens: Set(input.output_tokens),
            image_output_tokens: Set(input.image_output_tokens),
            cache_read_tokens: Set(input.cache_read_tokens),
            cache_creation_5m_tokens: Set(input.cache_creation_5m_tokens),
            cache_creation_30m_tokens: Set(input.cache_creation_30m_tokens),
            cache_creation_1h_tokens: Set(input.cache_creation_1h_tokens),
            cost: Set(input.cost.to_string()),
            estimated_tokens: Set(input.estimated_tokens),
            estimated_cost: Set(input.estimated_cost.map(|v| v.to_string())),
            aggregated_through: Set(input.aggregated_through),
            finalized_at: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
        };
        apply_cycle_input(&mut am, &input, scope_json.clone(), now);
        match am.insert(conn).await {
            Ok(model) => return cycle_record(model),
            Err(e) if is_unique_violation(&e) => continue,
            Err(e) => return Err(e.into()),
        }
    }
    anyhow::bail!("credential quota-cycle upsert: persistent write contention")
}

pub async fn query_cycles(
    conn: &DatabaseConnection,
    q: &CredentialQuotaCycleQuery,
) -> anyhow::Result<Vec<CredentialQuotaCycle>> {
    let mut sel = cycle::Entity::find();
    if let Some(v) = q.credential_id {
        sel = sel.filter(cycle::Column::CredentialId.eq(v));
    }
    if let Some(v) = q.provider_id {
        sel = sel.filter(cycle::Column::ProviderId.eq(v));
    }
    if let Some(ref v) = q.channel {
        sel = sel.filter(cycle::Column::Channel.eq(v.clone()));
    }
    if let Some(ref v) = q.window_key {
        sel = sel.filter(cycle::Column::WindowKey.eq(v.clone()));
    }
    if let Some(ref v) = q.status {
        sel = sel.filter(cycle::Column::Status.eq(v.clone()));
    }
    if let Some(v) = q.from {
        sel = sel.filter(
            cycle::Column::PeriodEnd
                .gte(v)
                .or(cycle::Column::LastObservedAt.gte(v)),
        );
    }
    if let Some(v) = q.to {
        sel = sel.filter(
            cycle::Column::PeriodStart
                .lte(v)
                .or(cycle::Column::PeriodStart
                    .is_null()
                    .and(cycle::Column::LastObservedAt.lte(v))),
        );
    }
    if let Some(v) = q.before_id {
        sel = sel.filter(cycle::Column::Id.lt(v));
    }
    sel = sel.order_by_desc(cycle::Column::Id);
    if q.limit > 0 {
        sel = sel.limit(q.limit);
    }
    sel.all(conn).await?.into_iter().map(cycle_record).collect()
}

pub async fn finalize_cycle(
    conn: &DatabaseConnection,
    id: i64,
    period_end: Option<i64>,
    close_reason: &str,
    finalized_at: i64,
) -> anyhow::Result<Option<CredentialQuotaCycle>> {
    let Some(existing) = cycle::Entity::find_by_id(id).one(conn).await? else {
        return Ok(None);
    };
    if existing.status != "open" {
        return cycle_record(existing).map(Some);
    }
    let mut am: cycle::ActiveModel = existing.into();
    if period_end.is_some() {
        am.period_end = Set(period_end);
    }
    am.close_reason = Set(Some(close_reason.to_owned()));
    am.status = Set("finalized".to_owned());
    am.open_slot = Set(None);
    am.finalized_at = Set(Some(finalized_at));
    am.updated_at = Set(finalized_at);
    match cycle::Entity::update(am)
        .validate()?
        .filter(cycle::Column::OpenSlot.eq(1))
        .exec_without_returning(conn)
        .await
    {
        Ok(_) | Err(sea_orm::DbErr::RecordNotUpdated) => get_cycle(conn, id).await,
        Err(err) => Err(err.into()),
    }
}

fn cycle_model_record(m: cycle_model::Model) -> anyhow::Result<CredentialQuotaCycleModel> {
    Ok(CredentialQuotaCycleModel {
        id: m.id,
        cycle_id: m.cycle_id,
        model: m.model,
        requests: m.requests,
        input_tokens: m.input_tokens,
        output_tokens: m.output_tokens,
        image_output_tokens: m.image_output_tokens,
        cache_read_tokens: m.cache_read_tokens,
        cache_creation_5m_tokens: m.cache_creation_5m_tokens,
        cache_creation_30m_tokens: m.cache_creation_30m_tokens,
        cache_creation_1h_tokens: m.cache_creation_1h_tokens,
        cost: m.cost.parse()?,
        created_at: m.created_at,
        updated_at: m.updated_at,
    })
}

pub async fn upsert_cycle_model(
    conn: &DatabaseConnection,
    mut input: CredentialQuotaCycleModelInput,
) -> anyhow::Result<CredentialQuotaCycleModel> {
    input.model = normalized_cycle_model(&input.model);
    let cycle_status = cycle::Entity::find_by_id(input.cycle_id).one(conn).await?;
    if !matches!(
        cycle_status.as_ref().map(|v| v.status.as_str()),
        Some("open")
    ) {
        anyhow::bail!("quota-cycle model snapshots can only update an open cycle")
    }
    let now = crate::store::persistence::db::ops::now_secs();
    cycle_model::Entity::insert(cycle_model::ActiveModel {
        id: NotSet,
        cycle_id: Set(input.cycle_id),
        model: Set(input.model.clone()),
        requests: Set(input.requests),
        input_tokens: Set(input.input_tokens),
        output_tokens: Set(input.output_tokens),
        image_output_tokens: Set(input.image_output_tokens),
        cache_read_tokens: Set(input.cache_read_tokens),
        cache_creation_5m_tokens: Set(input.cache_creation_5m_tokens),
        cache_creation_30m_tokens: Set(input.cache_creation_30m_tokens),
        cache_creation_1h_tokens: Set(input.cache_creation_1h_tokens),
        cost: Set(input.cost.to_string()),
        created_at: Set(now),
        updated_at: Set(now),
    })
    .on_conflict(
        OnConflict::columns([cycle_model::Column::CycleId, cycle_model::Column::Model])
            .update_columns([
                cycle_model::Column::Requests,
                cycle_model::Column::InputTokens,
                cycle_model::Column::OutputTokens,
                cycle_model::Column::ImageOutputTokens,
                cycle_model::Column::CacheReadTokens,
                cycle_model::Column::CacheCreation5mTokens,
                cycle_model::Column::CacheCreation30mTokens,
                cycle_model::Column::CacheCreation1hTokens,
                cycle_model::Column::Cost,
                cycle_model::Column::UpdatedAt,
            ])
            .to_owned(),
    )
    .exec(conn)
    .await?;
    cycle_model::Entity::find()
        .filter(cycle_model::Column::CycleId.eq(input.cycle_id))
        .filter(cycle_model::Column::Model.eq(input.model))
        .one(conn)
        .await?
        .ok_or_else(|| anyhow::anyhow!("quota-cycle model vanished after upsert"))
        .and_then(cycle_model_record)
}

pub async fn list_cycle_models(
    conn: &DatabaseConnection,
    cycle_id: i64,
) -> anyhow::Result<Vec<CredentialQuotaCycleModel>> {
    cycle_model::Entity::find()
        .filter(cycle_model::Column::CycleId.eq(cycle_id))
        .order_by_asc(cycle_model::Column::Model)
        .all(conn)
        .await?
        .into_iter()
        .map(cycle_model_record)
        .collect()
}
