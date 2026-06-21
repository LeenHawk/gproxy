//! Quota ops for the libSQL edge backend. Unique per `(scope, scope_id)`.

use crate::store::libsql::{LibsqlClient, arg_integer, arg_text};
use crate::store::persistence::libsql::row::{Row, col_decimal, col_i64, col_str};
use crate::store::persistence::libsql::util::{arg_opt_i64, exec, last_rowid, now_secs, query_one};
use crate::store::persistence::records::{Quota, QuotaInput, Scope};

const COLS: &str = "id, scope, scope_id, quota_total, cost_used, created_at, updated_at";

fn decode(row: &Row) -> anyhow::Result<Quota> {
    Ok(Quota {
        id: col_i64(row, 0)?,
        scope: Scope::parse(&col_str(row, 1)?)?,
        scope_id: col_i64(row, 2)?,
        quota_total: col_decimal(row, 3)?,
        cost_used: col_decimal(row, 4)?,
        created_at: col_i64(row, 5)?,
        updated_at: col_i64(row, 6)?,
    })
}

async fn get_by_id(client: &LibsqlClient, id: i64) -> anyhow::Result<Option<Quota>> {
    query_one(
        client,
        &format!("SELECT {COLS} FROM quotas WHERE id = ?"),
        &[arg_integer(id)],
    )
    .await?
    .as_ref()
    .map(decode)
    .transpose()
}

pub async fn get(
    client: &LibsqlClient,
    scope: Scope,
    scope_id: i64,
) -> anyhow::Result<Option<Quota>> {
    query_one(
        client,
        &format!("SELECT {COLS} FROM quotas WHERE scope = ? AND scope_id = ?"),
        &[arg_text(scope.as_str()), arg_integer(scope_id)],
    )
    .await?
    .as_ref()
    .map(decode)
    .transpose()
}

pub async fn upsert(client: &LibsqlClient, input: QuotaInput) -> anyhow::Result<Quota> {
    let now = now_secs();

    // Enforce uniqueness on (scope, scope_id).
    if let Some(existing) = get(client, input.scope, input.scope_id).await?
        && Some(existing.id) != input.id
    {
        return Err(crate::store::persistence::ConflictError::new(format!(
            "quota already exists for scope {}:{}",
            input.scope.as_str(),
            input.scope_id
        ))
        .into());
    }

    let id = match input.id {
        Some(id) if get_by_id(client, id).await?.is_some() => {
            // cost_used is billing-owned (accumulated via add_cost). Editing an
            // EXISTING quota must NOT clobber it — omit it from the UPDATE. The
            // INSERT branch below (seed/import to an absent id) still honors input.
            client
                .execute(
                    "UPDATE quotas SET scope=?, scope_id=?, quota_total=?, updated_at=? \
                     WHERE id=?",
                    &[
                        arg_text(input.scope.as_str()),
                        arg_integer(input.scope_id),
                        arg_text(&input.quota_total.to_string()),
                        arg_integer(now),
                        arg_integer(id),
                    ],
                )
                .await
                .map_err(|e| {
                    crate::store::persistence::libsql::conflict_if_unique(e, || {
                        format!(
                            "quota already exists for scope {}:{}",
                            input.scope.as_str(),
                            input.scope_id
                        )
                    })
                })?;
            id
        }
        maybe_id => {
            let qr = client
                .execute(
                    "INSERT INTO quotas \
                     (id, scope, scope_id, quota_total, cost_used, created_at, updated_at) \
                     VALUES (?, ?, ?, ?, ?, ?, ?)",
                    &[
                        arg_opt_i64(maybe_id),
                        arg_text(input.scope.as_str()),
                        arg_integer(input.scope_id),
                        arg_text(&input.quota_total.to_string()),
                        arg_text(&input.cost_used.to_string()),
                        arg_integer(now),
                        arg_integer(now),
                    ],
                )
                .await
                .map_err(|e| {
                    crate::store::persistence::libsql::conflict_if_unique(e, || {
                        format!(
                            "quota already exists for scope {}:{}",
                            input.scope.as_str(),
                            input.scope_id
                        )
                    })
                })?;
            match maybe_id {
                Some(id) => id,
                None => last_rowid(&qr)?,
            }
        }
    };

    get_by_id(client, id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("quota vanished after upsert"))
}

pub async fn delete(client: &LibsqlClient, id: i64) -> anyhow::Result<bool> {
    let n = exec(
        client,
        "DELETE FROM quotas WHERE id = ?",
        &[arg_integer(id)],
    )
    .await?;
    Ok(n > 0)
}

pub async fn delete_by_scope(
    client: &LibsqlClient,
    scope: Scope,
    scope_id: i64,
) -> anyhow::Result<()> {
    exec(
        client,
        "DELETE FROM quotas WHERE scope = ? AND scope_id = ?",
        &[arg_text(scope.as_str()), arg_integer(scope_id)],
    )
    .await?;
    Ok(())
}

/// Add `delta` to `cost_used` for the `(scope, scope_id)` row. No-op when the
/// row is absent. `cost_used` is a TEXT decimal, so SQL `+` can't do exact
/// arithmetic; instead the read-add-write is guarded by a compare-and-swap on
/// the RAW stored text (retried on contention) — Turso is shared across
/// isolates/instances, so concurrent settles must not lose increments.
pub async fn add_cost(
    client: &LibsqlClient,
    scope: Scope,
    scope_id: i64,
    delta: rust_decimal::Decimal,
) -> anyhow::Result<()> {
    const CAS_RETRIES: u32 = 5;
    for _ in 0..CAS_RETRIES {
        let Some(row) = query_one(
            client,
            "SELECT id, cost_used FROM quotas WHERE scope = ? AND scope_id = ?",
            &[arg_text(scope.as_str()), arg_integer(scope_id)],
        )
        .await?
        else {
            return Ok(()); // no quota row → nothing to charge
        };
        let id = col_i64(&row, 0)?;
        let raw = col_str(&row, 1)?;
        let updated = raw.parse::<rust_decimal::Decimal>()? + delta;
        let n = exec(
            client,
            "UPDATE quotas SET cost_used = ?, updated_at = ? WHERE id = ? AND cost_used = ?",
            &[
                arg_text(&updated.to_string()),
                arg_integer(now_secs()),
                arg_integer(id),
                arg_text(&raw),
            ],
        )
        .await?;
        if n > 0 {
            return Ok(());
        }
    }
    anyhow::bail!(
        "quota add_cost: persistent write contention for {}:{scope_id}",
        scope.as_str()
    )
}
