//! Usage-rollup ops for the libSQL edge backend (accumulate by dimension bucket).

use serde_json::Value;

use crate::store::libsql::{LibsqlClient, arg_integer, arg_text};
use crate::store::persistence::libsql::row::{
    Row, col_decimal, col_i64, col_opt_i64, col_opt_str, col_str,
};
use crate::store::persistence::libsql::util::{
    arg_opt_i64, arg_opt_text, exec, last_rowid, now_secs, query, query_one,
};
use crate::store::persistence::records::{UsageRollup, UsageRollupInput};

const COLS: &str = "id, granularity, bucket_start, provider_id, org_id, team_id, user_id, \
     route_name, model, requests, input_tokens, output_tokens, cost, created_at, updated_at";

fn decode(row: &Row) -> anyhow::Result<UsageRollup> {
    Ok(UsageRollup {
        id: col_i64(row, 0)?,
        granularity: col_str(row, 1)?,
        bucket_start: col_i64(row, 2)?,
        provider_id: col_opt_i64(row, 3)?,
        org_id: col_opt_i64(row, 4)?,
        team_id: col_opt_i64(row, 5)?,
        user_id: col_opt_i64(row, 6)?,
        route_name: col_opt_str(row, 7)?,
        model: col_opt_str(row, 8)?,
        requests: col_i64(row, 9)?,
        input_tokens: col_i64(row, 10)?,
        output_tokens: col_i64(row, 11)?,
        cost: col_decimal(row, 12)?,
        created_at: col_i64(row, 13)?,
        updated_at: col_i64(row, 14)?,
    })
}

/// Append an `<col> = ?`/`<col> IS NULL` predicate for an optional i64 dimension.
fn push_opt_i64(sql: &mut String, args: &mut Vec<Value>, col: &str, v: Option<i64>) {
    match v {
        Some(n) => {
            sql.push_str(&format!(" AND {col} = ?"));
            args.push(arg_integer(n));
        }
        None => sql.push_str(&format!(" AND {col} IS NULL")),
    }
}

fn push_opt_text(sql: &mut String, args: &mut Vec<Value>, col: &str, v: Option<&str>) {
    match v {
        Some(s) => {
            sql.push_str(&format!(" AND {col} = ?"));
            args.push(arg_text(s));
        }
        None => sql.push_str(&format!(" AND {col} IS NULL")),
    }
}

async fn find_bucket_id(
    client: &LibsqlClient,
    input: &UsageRollupInput,
) -> anyhow::Result<Option<i64>> {
    let mut sql =
        String::from("SELECT id FROM usage_rollups WHERE granularity = ? AND bucket_start = ?");
    let mut args: Vec<Value> = vec![
        arg_text(&input.granularity),
        arg_integer(input.bucket_start),
    ];
    push_opt_i64(&mut sql, &mut args, "provider_id", input.provider_id);
    push_opt_i64(&mut sql, &mut args, "org_id", input.org_id);
    push_opt_i64(&mut sql, &mut args, "team_id", input.team_id);
    push_opt_i64(&mut sql, &mut args, "user_id", input.user_id);
    push_opt_text(
        &mut sql,
        &mut args,
        "route_name",
        input.route_name.as_deref(),
    );
    push_opt_text(&mut sql, &mut args, "model", input.model.as_deref());

    query_one(client, &sql, &args)
        .await?
        .as_ref()
        .map(|r| col_i64(r, 0))
        .transpose()
}

async fn get(client: &LibsqlClient, id: i64) -> anyhow::Result<Option<UsageRollup>> {
    query_one(
        client,
        &format!("SELECT {COLS} FROM usage_rollups WHERE id = ?"),
        &[arg_integer(id)],
    )
    .await?
    .as_ref()
    .map(decode)
    .transpose()
}

/// Accumulate `input` into its dimension bucket. First-insert races collide on
/// the `uq_usage_rollups_dims` unique index — the loser retries into the
/// accumulate path.
pub async fn add(client: &LibsqlClient, input: UsageRollupInput) -> anyhow::Result<UsageRollup> {
    const INSERT_RETRIES: u32 = 5;
    let now = now_secs();

    for _ in 0..INSERT_RETRIES {
        if let Some(id) = find_bucket_id(client, &input).await? {
            accumulate(client, id, &input, now).await?;
            return get(client, id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("usage_rollup vanished after add"));
        }
        let insert = client
            .execute(
                "INSERT INTO usage_rollups \
                 (granularity, bucket_start, provider_id, org_id, team_id, user_id, \
                  route_name, model, requests, input_tokens, output_tokens, cost, \
                  created_at, updated_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                &[
                    arg_text(&input.granularity),
                    arg_integer(input.bucket_start),
                    arg_opt_i64(input.provider_id),
                    arg_opt_i64(input.org_id),
                    arg_opt_i64(input.team_id),
                    arg_opt_i64(input.user_id),
                    arg_opt_text(input.route_name.as_deref()),
                    arg_opt_text(input.model.as_deref()),
                    arg_integer(input.requests),
                    arg_integer(input.input_tokens),
                    arg_integer(input.output_tokens),
                    arg_text(&input.cost.to_string()),
                    arg_integer(now),
                    arg_integer(now),
                ],
            )
            .await;
        match insert {
            Ok(qr) => {
                let id = last_rowid(&qr)?;
                return get(client, id)
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("usage_rollup vanished after add"));
            }
            // Concurrent first insert won the bucket: re-find → accumulate.
            Err(e) if e.to_string().to_ascii_lowercase().contains("unique") => continue,
            Err(e) => return Err(anyhow::anyhow!("libsql insert usage_rollup: {e}")),
        }
    }
    anyhow::bail!("usage_rollup add: persistent insert contention")
}

/// Accumulate `input` into bucket `id`. The integer columns add IN SQL (atomic
/// regardless of concurrent writers); `cost` is a TEXT decimal, so it rides a
/// compare-and-swap on the raw stored text, retried on contention — Turso is
/// shared across isolates/instances. A failed CAS applies nothing (single-row
/// guarded UPDATE), so the retry never double-counts the integer columns.
async fn accumulate(
    client: &LibsqlClient,
    id: i64,
    input: &UsageRollupInput,
    now: i64,
) -> anyhow::Result<()> {
    const CAS_RETRIES: u32 = 5;
    for _ in 0..CAS_RETRIES {
        let raw_cost = query_one(
            client,
            "SELECT cost FROM usage_rollups WHERE id = ?",
            &[arg_integer(id)],
        )
        .await?
        .as_ref()
        .map(|r| col_str(r, 0))
        .transpose()?
        .ok_or_else(|| anyhow::anyhow!("rollup bucket vanished"))?;
        let cost = raw_cost.parse::<rust_decimal::Decimal>()? + input.cost;
        let n = exec(
            client,
            "UPDATE usage_rollups SET requests = requests + ?, \
             input_tokens = input_tokens + ?, output_tokens = output_tokens + ?, \
             cost = ?, updated_at = ? WHERE id = ? AND cost = ?",
            &[
                arg_integer(input.requests),
                arg_integer(input.input_tokens),
                arg_integer(input.output_tokens),
                arg_text(&cost.to_string()),
                arg_integer(now),
                arg_integer(id),
                arg_text(&raw_cost),
            ],
        )
        .await?;
        if n > 0 {
            return Ok(());
        }
    }
    anyhow::bail!("usage_rollup accumulate: persistent write contention for bucket {id}")
}

pub async fn list(
    client: &LibsqlClient,
    granularity: &str,
    from: i64,
    to: i64,
    user_id: Option<i64>,
) -> anyhow::Result<Vec<UsageRollup>> {
    if let Some(uid) = user_id {
        query(
            client,
            &format!(
                "SELECT {COLS} FROM usage_rollups WHERE granularity = ? AND bucket_start >= ? \
                 AND bucket_start <= ? AND user_id = ? ORDER BY bucket_start ASC"
            ),
            &[
                arg_text(granularity),
                arg_integer(from),
                arg_integer(to),
                arg_integer(uid),
            ],
        )
        .await?
        .iter()
        .map(decode)
        .collect()
    } else {
        query(
            client,
            &format!(
                "SELECT {COLS} FROM usage_rollups WHERE granularity = ? AND bucket_start >= ? \
                 AND bucket_start <= ? ORDER BY bucket_start ASC"
            ),
            &[arg_text(granularity), arg_integer(from), arg_integer(to)],
        )
        .await?
        .iter()
        .map(decode)
        .collect()
    }
}
