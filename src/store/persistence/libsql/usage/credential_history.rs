//! Permanent credential daily aggregates and upstream quota-cycle history.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::store::libsql::{LibsqlClient, arg_integer, arg_text};
use crate::store::persistence::libsql::row::{
    Row, col_decimal, col_i64, col_opt_i64, col_opt_json, col_opt_str, col_str,
};
use crate::store::persistence::libsql::util::{
    arg_opt_i64, arg_opt_text, exec, last_rowid, now_secs, query, query_one,
};
use crate::store::persistence::records::{
    CredentialQuotaCycle, CredentialQuotaCycleInput, CredentialQuotaCycleModel,
    CredentialQuotaCycleModelInput, CredentialUsageDaily, CredentialUsageDailyInput,
};
use crate::store::persistence::{CredentialQuotaCycleQuery, CredentialUsageDailyQuery};

const DAILY_COLS: &str = "id, day_start, credential_id, provider_id, model, requests, \
    input_tokens, output_tokens, image_output_tokens, cache_read_tokens, \
    cache_creation_5m_tokens, cache_creation_30m_tokens, cache_creation_1h_tokens, \
    cost, created_at, updated_at";

fn daily_decode(row: &Row) -> anyhow::Result<CredentialUsageDaily> {
    Ok(CredentialUsageDaily {
        id: col_i64(row, 0)?,
        day_start: col_i64(row, 1)?,
        credential_id: col_i64(row, 2)?,
        provider_id: col_i64(row, 3)?,
        model: col_opt_str(row, 4)?,
        requests: col_i64(row, 5)?,
        input_tokens: col_i64(row, 6)?,
        output_tokens: col_i64(row, 7)?,
        image_output_tokens: col_i64(row, 8)?,
        cache_read_tokens: col_i64(row, 9)?,
        cache_creation_5m_tokens: col_i64(row, 10)?,
        cache_creation_30m_tokens: col_i64(row, 11)?,
        cache_creation_1h_tokens: col_i64(row, 12)?,
        cost: col_decimal(row, 13)?,
        created_at: col_i64(row, 14)?,
        updated_at: col_i64(row, 15)?,
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

fn push_opt_text_predicate(
    sql: &mut String,
    args: &mut Vec<Value>,
    col: &str,
    value: Option<&str>,
) {
    if let Some(value) = value {
        sql.push_str(&format!(" AND {col} = ?"));
        args.push(arg_text(value));
    } else {
        sql.push_str(&format!(" AND {col} IS NULL"));
    }
}

async fn daily_id(
    client: &LibsqlClient,
    input: &CredentialUsageDailyInput,
) -> anyhow::Result<Option<i64>> {
    let mut sql = String::from(
        "SELECT id FROM credential_usage_daily WHERE day_start = ? AND credential_id = ?",
    );
    let mut args = vec![
        arg_integer(input.day_start),
        arg_integer(input.credential_id),
    ];
    let model = normalized_model(input.model.as_deref());
    push_opt_text_predicate(&mut sql, &mut args, "model", model.as_deref());
    query_one(client, &sql, &args)
        .await?
        .as_ref()
        .map(|r| col_i64(r, 0))
        .transpose()
}

async fn get_daily(client: &LibsqlClient, id: i64) -> anyhow::Result<Option<CredentialUsageDaily>> {
    query_one(
        client,
        &format!("SELECT {DAILY_COLS} FROM credential_usage_daily WHERE id = ?"),
        &[arg_integer(id)],
    )
    .await?
    .as_ref()
    .map(daily_decode)
    .transpose()
}

async fn insert_daily(
    client: &LibsqlClient,
    input: &CredentialUsageDailyInput,
    now: i64,
) -> anyhow::Result<i64> {
    let model = normalized_model(input.model.as_deref());
    let qr = client
        .execute(
            "INSERT INTO credential_usage_daily (day_start, credential_id, provider_id, model, \
             requests, input_tokens, output_tokens, image_output_tokens, cache_read_tokens, \
             cache_creation_5m_tokens, cache_creation_30m_tokens, cache_creation_1h_tokens, cost, \
             created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            &[
                arg_integer(input.day_start),
                arg_integer(input.credential_id),
                arg_integer(input.provider_id),
                arg_opt_text(model.as_deref()),
                arg_integer(input.requests),
                arg_integer(input.input_tokens),
                arg_integer(input.output_tokens),
                arg_integer(input.image_output_tokens),
                arg_integer(input.cache_read_tokens),
                arg_integer(input.cache_creation_5m_tokens),
                arg_integer(input.cache_creation_30m_tokens),
                arg_integer(input.cache_creation_1h_tokens),
                arg_text(&input.cost.to_string()),
                arg_integer(now),
                arg_integer(now),
            ],
        )
        .await
        .map_err(|e| anyhow::anyhow!("libsql insert credential daily: {e}"))?;
    last_rowid(&qr)
}

pub async fn upsert_daily(
    client: &LibsqlClient,
    mut input: CredentialUsageDailyInput,
) -> anyhow::Result<CredentialUsageDaily> {
    input.model = normalized_model(input.model.as_deref());
    const RETRIES: u32 = 5;
    let now = now_secs();
    for _ in 0..RETRIES {
        if let Some(id) = daily_id(client, &input).await? {
            exec(
                client,
                "UPDATE credential_usage_daily SET provider_id=?, requests=?, input_tokens=?, \
                 output_tokens=?, image_output_tokens=?, cache_read_tokens=?, \
                 cache_creation_5m_tokens=?, cache_creation_30m_tokens=?, \
                 cache_creation_1h_tokens=?, cost=?, updated_at=? WHERE id=?",
                &[
                    arg_integer(input.provider_id),
                    arg_integer(input.requests),
                    arg_integer(input.input_tokens),
                    arg_integer(input.output_tokens),
                    arg_integer(input.image_output_tokens),
                    arg_integer(input.cache_read_tokens),
                    arg_integer(input.cache_creation_5m_tokens),
                    arg_integer(input.cache_creation_30m_tokens),
                    arg_integer(input.cache_creation_1h_tokens),
                    arg_text(&input.cost.to_string()),
                    arg_integer(now),
                    arg_integer(id),
                ],
            )
            .await?;
            return get_daily(client, id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("credential daily bucket vanished"));
        }
        match insert_daily(client, &input, now).await {
            Ok(id) => {
                return get_daily(client, id)
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("credential daily bucket vanished"));
            }
            Err(e) if e.to_string().to_ascii_lowercase().contains("unique") => continue,
            Err(e) => return Err(anyhow::anyhow!("libsql insert credential daily: {e}")),
        }
    }
    anyhow::bail!("credential_usage_daily upsert: persistent write contention")
}

pub async fn add_daily(
    client: &LibsqlClient,
    mut input: CredentialUsageDailyInput,
) -> anyhow::Result<CredentialUsageDaily> {
    input.model = normalized_model(input.model.as_deref());
    const RETRIES: u32 = 5;
    let now = now_secs();
    for _ in 0..RETRIES {
        let Some(id) = daily_id(client, &input).await? else {
            match insert_daily(client, &input, now).await {
                Ok(id) => {
                    return get_daily(client, id)
                        .await?
                        .ok_or_else(|| anyhow::anyhow!("credential daily bucket vanished"));
                }
                Err(e) if e.to_string().to_ascii_lowercase().contains("unique") => continue,
                Err(e) => return Err(anyhow::anyhow!("libsql insert credential daily: {e}")),
            }
        };
        let raw_cost = query_one(
            client,
            "SELECT cost FROM credential_usage_daily WHERE id = ?",
            &[arg_integer(id)],
        )
        .await?
        .as_ref()
        .map(|r| col_str(r, 0))
        .transpose()?
        .ok_or_else(|| anyhow::anyhow!("credential daily bucket vanished"))?;
        let cost = raw_cost.parse::<rust_decimal::Decimal>()? + input.cost;
        let changed = exec(
            client,
            "UPDATE credential_usage_daily SET provider_id=?, requests=requests+?, \
             input_tokens=input_tokens+?, output_tokens=output_tokens+?, \
             image_output_tokens=image_output_tokens+?, cache_read_tokens=cache_read_tokens+?, \
             cache_creation_5m_tokens=cache_creation_5m_tokens+?, \
             cache_creation_30m_tokens=cache_creation_30m_tokens+?, \
             cache_creation_1h_tokens=cache_creation_1h_tokens+?, cost=?, updated_at=? \
             WHERE id=? AND cost=?",
            &[
                arg_integer(input.provider_id),
                arg_integer(input.requests),
                arg_integer(input.input_tokens),
                arg_integer(input.output_tokens),
                arg_integer(input.image_output_tokens),
                arg_integer(input.cache_read_tokens),
                arg_integer(input.cache_creation_5m_tokens),
                arg_integer(input.cache_creation_30m_tokens),
                arg_integer(input.cache_creation_1h_tokens),
                arg_text(&cost.to_string()),
                arg_integer(now),
                arg_integer(id),
                arg_text(&raw_cost),
            ],
        )
        .await?;
        if changed > 0 {
            return get_daily(client, id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("credential daily bucket vanished"));
        }
    }
    anyhow::bail!("credential_usage_daily add: persistent write contention")
}

pub async fn query_daily(
    client: &LibsqlClient,
    q: &CredentialUsageDailyQuery,
) -> anyhow::Result<Vec<CredentialUsageDaily>> {
    let mut sql = format!("SELECT {DAILY_COLS} FROM credential_usage_daily WHERE 1=1");
    let mut args = Vec::new();
    if let Some(v) = q.credential_id {
        sql.push_str(" AND credential_id = ?");
        args.push(arg_integer(v));
    }
    if let Some(v) = q.provider_id {
        sql.push_str(" AND provider_id = ?");
        args.push(arg_integer(v));
    }
    if let Some(v) = q.from {
        sql.push_str(" AND day_start >= ?");
        args.push(arg_integer(v));
    }
    if let Some(v) = q.to {
        sql.push_str(" AND day_start <= ?");
        args.push(arg_integer(v));
    }
    sql.push_str(" ORDER BY day_start ASC, id ASC");
    query(client, &sql, &args)
        .await?
        .iter()
        .map(daily_decode)
        .collect()
}

/// Rebuild every retained raw-usage bucket in the inclusive timestamp range.
/// Buckets absent from retained raw usage are preserved.
pub async fn reconcile_daily(
    client: &LibsqlClient,
    from: Option<i64>,
    to: Option<i64>,
) -> anyhow::Result<u64> {
    let mut sql = String::from(
        "SELECT (at / 86400) * 86400, credential_id, MAX(provider_id), \
         NULLIF(TRIM(model), ''), COUNT(*), \
         SUM(input_tokens), SUM(output_tokens), SUM(image_output_tokens), \
         SUM(cache_read_tokens), SUM(cache_creation_5m_tokens), \
         SUM(cache_creation_30m_tokens), SUM(cache_creation_1h_tokens), \
         CAST(COALESCE(SUM(CAST(cost AS NUMERIC)), 0) AS TEXT) FROM usages \
         WHERE credential_id IS NOT NULL AND provider_id IS NOT NULL",
    );
    let mut args = Vec::new();
    if let Some(v) = from {
        sql.push_str(" AND at >= ?");
        args.push(arg_integer(v));
    }
    if let Some(v) = to {
        sql.push_str(" AND at <= ?");
        args.push(arg_integer(v));
    }
    sql.push_str(
        " GROUP BY (at / 86400) * 86400, credential_id, NULLIF(TRIM(model), '') \
         ORDER BY (at / 86400) * 86400",
    );
    let rows = query(client, &sql, &args).await?;
    let mut cost_sql = String::from(
        "SELECT at, credential_id, model, cost FROM usages \
         WHERE credential_id IS NOT NULL AND provider_id IS NOT NULL",
    );
    let mut cost_args = Vec::new();
    if let Some(v) = from {
        cost_sql.push_str(" AND at >= ?");
        cost_args.push(arg_integer(v));
    }
    if let Some(v) = to {
        cost_sql.push_str(" AND at <= ?");
        cost_args.push(arg_integer(v));
    }
    let mut exact_costs = BTreeMap::new();
    for row in query(client, &cost_sql, &cost_args).await? {
        let at = col_i64(&row, 0)?;
        let key = (
            at - at.rem_euclid(86_400),
            col_i64(&row, 1)?,
            normalized_model(col_opt_str(&row, 2)?.as_deref()),
        );
        *exact_costs
            .entry(key)
            .or_insert(rust_decimal::Decimal::ZERO) += col_decimal(&row, 3)?;
    }
    let count = u64::try_from(rows.len())?;
    for row in rows {
        let input = CredentialUsageDailyInput {
            day_start: col_i64(&row, 0)?,
            credential_id: col_i64(&row, 1)?,
            provider_id: col_i64(&row, 2)?,
            model: normalized_model(col_opt_str(&row, 3)?.as_deref()),
            requests: col_i64(&row, 4)?,
            input_tokens: col_i64(&row, 5)?,
            output_tokens: col_i64(&row, 6)?,
            image_output_tokens: col_i64(&row, 7)?,
            cache_read_tokens: col_i64(&row, 8)?,
            cache_creation_5m_tokens: col_i64(&row, 9)?,
            cache_creation_30m_tokens: col_i64(&row, 10)?,
            cache_creation_1h_tokens: col_i64(&row, 11)?,
            cost: exact_costs
                .get(&(
                    col_i64(&row, 0)?,
                    col_i64(&row, 1)?,
                    normalized_model(col_opt_str(&row, 3)?.as_deref()),
                ))
                .copied()
                .unwrap_or(col_decimal(&row, 12)?),
        };
        upsert_daily(client, input).await?;
    }
    Ok(count)
}

const CYCLE_COLS: &str = "id, credential_id, provider_id, channel, window_key, name, label, \
    scope_kind, scope_json, meter_kind, period_start, period_end, boundary_source, \
    boundary_confidence, close_reason, status, last_observed_at, used_percent, upstream_used, \
    upstream_limit, coverage, requests, input_tokens, output_tokens, image_output_tokens, \
    cache_read_tokens, cache_creation_5m_tokens, cache_creation_30m_tokens, \
    cache_creation_1h_tokens, cost, estimated_tokens, estimated_cost, aggregated_through, \
    finalized_at, created_at, updated_at";

fn opt_decimal(row: &Row, idx: usize) -> anyhow::Result<Option<rust_decimal::Decimal>> {
    col_opt_str(row, idx)?
        .map(|v| v.parse::<rust_decimal::Decimal>())
        .transpose()
        .map_err(Into::into)
}

fn cycle_decode(row: &Row) -> anyhow::Result<CredentialQuotaCycle> {
    Ok(CredentialQuotaCycle {
        id: col_i64(row, 0)?,
        credential_id: col_i64(row, 1)?,
        provider_id: col_i64(row, 2)?,
        channel: col_str(row, 3)?,
        window_key: col_str(row, 4)?,
        name: col_str(row, 5)?,
        label: col_opt_str(row, 6)?,
        scope_kind: col_str(row, 7)?,
        scope_json: col_opt_json(row, 8)?,
        meter_kind: col_str(row, 9)?,
        period_start: col_opt_i64(row, 10)?,
        period_end: col_opt_i64(row, 11)?,
        boundary_source: col_str(row, 12)?,
        boundary_confidence: col_str(row, 13)?,
        close_reason: col_opt_str(row, 14)?,
        status: col_str(row, 15)?,
        last_observed_at: col_opt_i64(row, 16)?,
        used_percent: opt_decimal(row, 17)?,
        upstream_used: opt_decimal(row, 18)?,
        upstream_limit: opt_decimal(row, 19)?,
        coverage: col_str(row, 20)?,
        requests: col_i64(row, 21)?,
        input_tokens: col_i64(row, 22)?,
        output_tokens: col_i64(row, 23)?,
        image_output_tokens: col_i64(row, 24)?,
        cache_read_tokens: col_i64(row, 25)?,
        cache_creation_5m_tokens: col_i64(row, 26)?,
        cache_creation_30m_tokens: col_i64(row, 27)?,
        cache_creation_1h_tokens: col_i64(row, 28)?,
        cost: col_decimal(row, 29)?,
        estimated_tokens: col_opt_i64(row, 30)?,
        estimated_cost: opt_decimal(row, 31)?,
        aggregated_through: col_opt_i64(row, 32)?,
        finalized_at: col_opt_i64(row, 33)?,
        created_at: col_i64(row, 34)?,
        updated_at: col_i64(row, 35)?,
    })
}

pub async fn get_cycle(
    client: &LibsqlClient,
    id: i64,
) -> anyhow::Result<Option<CredentialQuotaCycle>> {
    query_one(
        client,
        &format!("SELECT {CYCLE_COLS} FROM credential_quota_cycles WHERE id = ?"),
        &[arg_integer(id)],
    )
    .await?
    .as_ref()
    .map(cycle_decode)
    .transpose()
}

async fn open_cycle_id(
    client: &LibsqlClient,
    credential_id: i64,
    window_key: &str,
) -> anyhow::Result<Option<i64>> {
    query_one(
        client,
        "SELECT id FROM credential_quota_cycles \
         WHERE credential_id = ? AND window_key = ? AND open_slot = 1",
        &[arg_integer(credential_id), arg_text(window_key)],
    )
    .await?
    .as_ref()
    .map(|r| col_i64(r, 0))
    .transpose()
}

pub async fn get_open_cycle(
    client: &LibsqlClient,
    credential_id: i64,
    window_key: &str,
) -> anyhow::Result<Option<CredentialQuotaCycle>> {
    match open_cycle_id(client, credential_id, window_key).await? {
        Some(id) => get_cycle(client, id).await,
        None => Ok(None),
    }
}

fn cycle_args(input: &CredentialQuotaCycleInput, scope_json: Option<&str>, now: i64) -> Vec<Value> {
    vec![
        arg_integer(input.provider_id),
        arg_text(&input.channel),
        arg_text(&input.name),
        arg_opt_text(input.label.as_deref()),
        arg_text(&input.scope_kind),
        arg_opt_text(scope_json),
        arg_text(&input.meter_kind),
        arg_opt_i64(input.period_start),
        arg_opt_i64(input.period_end),
        arg_text(&input.boundary_source),
        arg_text(&input.boundary_confidence),
        arg_opt_i64(input.last_observed_at),
        arg_opt_text(
            input
                .used_percent
                .as_ref()
                .map(|v| v.to_string())
                .as_deref(),
        ),
        arg_opt_text(
            input
                .upstream_used
                .as_ref()
                .map(|v| v.to_string())
                .as_deref(),
        ),
        arg_opt_text(
            input
                .upstream_limit
                .as_ref()
                .map(|v| v.to_string())
                .as_deref(),
        ),
        arg_text(&input.coverage),
        arg_integer(input.requests),
        arg_integer(input.input_tokens),
        arg_integer(input.output_tokens),
        arg_integer(input.image_output_tokens),
        arg_integer(input.cache_read_tokens),
        arg_integer(input.cache_creation_5m_tokens),
        arg_integer(input.cache_creation_30m_tokens),
        arg_integer(input.cache_creation_1h_tokens),
        arg_text(&input.cost.to_string()),
        arg_opt_i64(input.estimated_tokens),
        arg_opt_text(
            input
                .estimated_cost
                .as_ref()
                .map(|v| v.to_string())
                .as_deref(),
        ),
        arg_opt_i64(input.aggregated_through),
        arg_integer(now),
    ]
}

pub async fn upsert_cycle(
    client: &LibsqlClient,
    input: CredentialQuotaCycleInput,
) -> anyhow::Result<CredentialQuotaCycle> {
    const RETRIES: u32 = 5;
    let now = now_secs();
    let scope_json = input
        .scope_json
        .as_ref()
        .map(serde_json::to_string)
        .transpose()?;
    for _ in 0..RETRIES {
        if let Some(id) = open_cycle_id(client, input.credential_id, &input.window_key).await? {
            let existing = get_cycle(client, id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("quota cycle vanished before update"))?;
            let mut input = input.clone();
            input.preserve_monotonic_local(&existing);
            let mut args = cycle_args(&input, scope_json.as_deref(), now);
            args.push(arg_integer(id));
            let sql = String::from(
                "UPDATE credential_quota_cycles SET provider_id=?, channel=?, name=?, label=?, \
                 scope_kind=?, scope_json=?, meter_kind=?, period_start=?, period_end=?, \
                 boundary_source=?, boundary_confidence=?, last_observed_at=?, used_percent=?, \
                 upstream_used=?, upstream_limit=?, coverage=?, requests=?, input_tokens=?, \
                 output_tokens=?, image_output_tokens=?, cache_read_tokens=?, \
                 cache_creation_5m_tokens=?, cache_creation_30m_tokens=?, \
                 cache_creation_1h_tokens=?, cost=?, estimated_tokens=?, estimated_cost=?, \
                 aggregated_through=?, updated_at=? WHERE id=? AND open_slot=1",
            );
            let changed = exec(client, &sql, &args).await?;
            if changed == 0 {
                return get_cycle(client, id)
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("quota cycle vanished after finalize"));
            }
            return get_cycle(client, id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("quota cycle vanished after update"));
        }
        // Insert order differs from update args at window_key/status/open fields.
        let insert_args = vec![
            arg_integer(input.credential_id),
            arg_integer(input.provider_id),
            arg_text(&input.channel),
            arg_text(&input.window_key),
            arg_text(&input.name),
            arg_opt_text(input.label.as_deref()),
            arg_text(&input.scope_kind),
            arg_opt_text(scope_json.as_deref()),
            arg_text(&input.meter_kind),
            arg_opt_i64(input.period_start),
            arg_opt_i64(input.period_end),
            arg_text(&input.boundary_source),
            arg_text(&input.boundary_confidence),
            arg_opt_i64(input.last_observed_at),
            arg_opt_text(
                input
                    .used_percent
                    .as_ref()
                    .map(|v| v.to_string())
                    .as_deref(),
            ),
            arg_opt_text(
                input
                    .upstream_used
                    .as_ref()
                    .map(|v| v.to_string())
                    .as_deref(),
            ),
            arg_opt_text(
                input
                    .upstream_limit
                    .as_ref()
                    .map(|v| v.to_string())
                    .as_deref(),
            ),
            arg_text(&input.coverage),
            arg_integer(input.requests),
            arg_integer(input.input_tokens),
            arg_integer(input.output_tokens),
            arg_integer(input.image_output_tokens),
            arg_integer(input.cache_read_tokens),
            arg_integer(input.cache_creation_5m_tokens),
            arg_integer(input.cache_creation_30m_tokens),
            arg_integer(input.cache_creation_1h_tokens),
            arg_text(&input.cost.to_string()),
            arg_opt_i64(input.estimated_tokens),
            arg_opt_text(
                input
                    .estimated_cost
                    .as_ref()
                    .map(|v| v.to_string())
                    .as_deref(),
            ),
            arg_opt_i64(input.aggregated_through),
            arg_integer(now),
            arg_integer(now),
        ];
        let result = client
            .execute(
                "INSERT INTO credential_quota_cycles (credential_id, provider_id, channel, \
                 window_key, name, label, scope_kind, scope_json, meter_kind, period_start, \
                 period_end, boundary_source, boundary_confidence, status, open_slot, \
                 last_observed_at, used_percent, upstream_used, upstream_limit, coverage, requests, \
                 input_tokens, output_tokens, image_output_tokens, cache_read_tokens, \
                 cache_creation_5m_tokens, cache_creation_30m_tokens, \
                 cache_creation_1h_tokens, cost, estimated_tokens, estimated_cost, \
                 aggregated_through, created_at, updated_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'open', 1, ?, ?, ?, ?, ?, ?, ?, \
                 ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                &insert_args,
            )
            .await;
        match result {
            Ok(qr) => {
                let id = last_rowid(&qr)?;
                return get_cycle(client, id)
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("quota cycle vanished after insert"));
            }
            Err(e) if e.to_string().to_ascii_lowercase().contains("unique") => continue,
            Err(e) => return Err(anyhow::anyhow!("libsql insert quota cycle: {e}")),
        }
    }
    anyhow::bail!("credential quota-cycle upsert: persistent write contention")
}

pub async fn query_cycles(
    client: &LibsqlClient,
    q: &CredentialQuotaCycleQuery,
) -> anyhow::Result<Vec<CredentialQuotaCycle>> {
    let mut sql = format!("SELECT {CYCLE_COLS} FROM credential_quota_cycles WHERE 1=1");
    let mut args = Vec::new();
    if let Some(v) = q.credential_id {
        sql.push_str(" AND credential_id=?");
        args.push(arg_integer(v));
    }
    if let Some(v) = q.provider_id {
        sql.push_str(" AND provider_id=?");
        args.push(arg_integer(v));
    }
    if let Some(ref v) = q.channel {
        sql.push_str(" AND channel=?");
        args.push(arg_text(v));
    }
    if let Some(ref v) = q.window_key {
        sql.push_str(" AND window_key=?");
        args.push(arg_text(v));
    }
    if let Some(ref v) = q.status {
        sql.push_str(" AND status=?");
        args.push(arg_text(v));
    }
    if let Some(v) = q.from {
        sql.push_str(" AND (period_end >= ? OR last_observed_at >= ?)");
        args.push(arg_integer(v));
        args.push(arg_integer(v));
    }
    if let Some(v) = q.to {
        sql.push_str(
            " AND (period_start <= ? OR (period_start IS NULL AND last_observed_at <= ?))",
        );
        args.push(arg_integer(v));
        args.push(arg_integer(v));
    }
    if let Some(v) = q.before_id {
        sql.push_str(" AND id < ?");
        args.push(arg_integer(v));
    }
    sql.push_str(" ORDER BY id DESC");
    if q.limit > 0 {
        sql.push_str(" LIMIT ?");
        args.push(arg_integer(q.limit as i64));
    }
    query(client, &sql, &args)
        .await?
        .iter()
        .map(cycle_decode)
        .collect()
}

pub async fn finalize_cycle(
    client: &LibsqlClient,
    id: i64,
    period_end: Option<i64>,
    close_reason: &str,
    finalized_at: i64,
) -> anyhow::Result<Option<CredentialQuotaCycle>> {
    let Some(existing) = get_cycle(client, id).await? else {
        return Ok(None);
    };
    if existing.status != "open" {
        return Ok(Some(existing));
    }
    exec(
        client,
        "UPDATE credential_quota_cycles SET period_end=COALESCE(?, period_end), close_reason=?, \
         status='finalized', open_slot=NULL, finalized_at=?, updated_at=? WHERE id=? AND open_slot=1",
        &[
            arg_opt_i64(period_end),
            arg_text(close_reason),
            arg_integer(finalized_at),
            arg_integer(finalized_at),
            arg_integer(id),
        ],
    )
    .await?;
    get_cycle(client, id).await
}

const MODEL_COLS: &str = "id, cycle_id, model, requests, input_tokens, output_tokens, \
    image_output_tokens, cache_read_tokens, cache_creation_5m_tokens, \
    cache_creation_30m_tokens, cache_creation_1h_tokens, cost, created_at, updated_at";

fn model_decode(row: &Row) -> anyhow::Result<CredentialQuotaCycleModel> {
    Ok(CredentialQuotaCycleModel {
        id: col_i64(row, 0)?,
        cycle_id: col_i64(row, 1)?,
        model: col_str(row, 2)?,
        requests: col_i64(row, 3)?,
        input_tokens: col_i64(row, 4)?,
        output_tokens: col_i64(row, 5)?,
        image_output_tokens: col_i64(row, 6)?,
        cache_read_tokens: col_i64(row, 7)?,
        cache_creation_5m_tokens: col_i64(row, 8)?,
        cache_creation_30m_tokens: col_i64(row, 9)?,
        cache_creation_1h_tokens: col_i64(row, 10)?,
        cost: col_decimal(row, 11)?,
        created_at: col_i64(row, 12)?,
        updated_at: col_i64(row, 13)?,
    })
}

pub async fn upsert_cycle_model(
    client: &LibsqlClient,
    mut input: CredentialQuotaCycleModelInput,
) -> anyhow::Result<CredentialQuotaCycleModel> {
    input.model = normalized_cycle_model(&input.model);
    let status = get_cycle(client, input.cycle_id).await?;
    if !matches!(status.as_ref().map(|v| v.status.as_str()), Some("open")) {
        anyhow::bail!("quota-cycle model snapshots can only update an open cycle")
    }
    let now = now_secs();
    client
        .execute(
            "INSERT INTO credential_quota_cycle_models (cycle_id, model, requests, input_tokens, \
             output_tokens, image_output_tokens, cache_read_tokens, cache_creation_5m_tokens, \
             cache_creation_30m_tokens, cache_creation_1h_tokens, cost, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(cycle_id, model) DO UPDATE SET requests=excluded.requests, \
             input_tokens=excluded.input_tokens, output_tokens=excluded.output_tokens, \
             image_output_tokens=excluded.image_output_tokens, \
             cache_read_tokens=excluded.cache_read_tokens, \
             cache_creation_5m_tokens=excluded.cache_creation_5m_tokens, \
             cache_creation_30m_tokens=excluded.cache_creation_30m_tokens, \
             cache_creation_1h_tokens=excluded.cache_creation_1h_tokens, cost=excluded.cost, \
             updated_at=excluded.updated_at",
            &[
                arg_integer(input.cycle_id),
                arg_text(&input.model),
                arg_integer(input.requests),
                arg_integer(input.input_tokens),
                arg_integer(input.output_tokens),
                arg_integer(input.image_output_tokens),
                arg_integer(input.cache_read_tokens),
                arg_integer(input.cache_creation_5m_tokens),
                arg_integer(input.cache_creation_30m_tokens),
                arg_integer(input.cache_creation_1h_tokens),
                arg_text(&input.cost.to_string()),
                arg_integer(now),
                arg_integer(now),
            ],
        )
        .await
        .map_err(|e| anyhow::anyhow!("libsql upsert quota-cycle model: {e}"))?;
    query_one(
        client,
        &format!(
            "SELECT {MODEL_COLS} FROM credential_quota_cycle_models WHERE cycle_id=? AND model=?"
        ),
        &[arg_integer(input.cycle_id), arg_text(&input.model)],
    )
    .await?
    .as_ref()
    .map(model_decode)
    .transpose()?
    .ok_or_else(|| anyhow::anyhow!("quota-cycle model vanished after upsert"))
}

pub async fn list_cycle_models(
    client: &LibsqlClient,
    cycle_id: i64,
) -> anyhow::Result<Vec<CredentialQuotaCycleModel>> {
    query(
        client,
        &format!(
            "SELECT {MODEL_COLS} FROM credential_quota_cycle_models \
             WHERE cycle_id=? ORDER BY model ASC"
        ),
        &[arg_integer(cycle_id)],
    )
    .await?
    .iter()
    .map(model_decode)
    .collect()
}
