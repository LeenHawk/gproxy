//! Routing-rule ops for the libSQL edge backend.
//! Unique per `(provider_id, operation, kind)`.

use crate::store::libsql::{LibsqlClient, arg_integer, arg_text};
use crate::store::persistence::libsql::row::{Row, col_bool, col_i64, col_opt_str, col_str};
use crate::store::persistence::libsql::util::{
    arg_bool, arg_opt_i64, arg_opt_text, exec, last_rowid, now_secs, query, query_one,
};
use crate::store::persistence::records::{RoutingRule, RoutingRuleInput};

const COLS: &str = "id, provider_id, operation, kind, implementation, dest_operation, dest_kind, \
     sort_order, enabled, created_at, updated_at";

fn decode(row: &Row) -> anyhow::Result<RoutingRule> {
    Ok(RoutingRule {
        id: col_i64(row, 0)?,
        provider_id: col_i64(row, 1)?,
        operation: col_str(row, 2)?,
        kind: col_str(row, 3)?,
        implementation: col_str(row, 4)?,
        dest_operation: col_opt_str(row, 5)?,
        dest_kind: col_opt_str(row, 6)?,
        sort_order: col_i64(row, 7)?,
        enabled: col_bool(row, 8)?,
        created_at: col_i64(row, 9)?,
        updated_at: col_i64(row, 10)?,
    })
}

pub async fn get(client: &LibsqlClient, id: i64) -> anyhow::Result<Option<RoutingRule>> {
    query_one(
        client,
        &format!("SELECT {COLS} FROM routing_rules WHERE id = ?"),
        &[arg_integer(id)],
    )
    .await?
    .as_ref()
    .map(decode)
    .transpose()
}

pub async fn list(client: &LibsqlClient, provider_id: i64) -> anyhow::Result<Vec<RoutingRule>> {
    query(
        client,
        &format!("SELECT {COLS} FROM routing_rules WHERE provider_id = ?"),
        &[arg_integer(provider_id)],
    )
    .await?
    .iter()
    .map(decode)
    .collect()
}

pub async fn upsert(client: &LibsqlClient, input: RoutingRuleInput) -> anyhow::Result<RoutingRule> {
    let now = now_secs();

    // Enforce uniqueness on (provider_id, operation, kind).
    if let Some(row) = query_one(
        client,
        "SELECT id FROM routing_rules WHERE provider_id = ? AND operation = ? AND kind = ?",
        &[
            arg_integer(input.provider_id),
            arg_text(&input.operation),
            arg_text(&input.kind),
        ],
    )
    .await?
    {
        let existing = col_i64(&row, 0)?;
        if Some(existing) != input.id {
            return Err(crate::store::persistence::ConflictError::new(format!(
                "routing rule already exists for provider {} ({}, {})",
                input.provider_id, input.operation, input.kind
            ))
            .into());
        }
    }

    let id = match input.id {
        Some(id) if get(client, id).await?.is_some() => {
            client
                .execute(
                    "UPDATE routing_rules SET provider_id=?, operation=?, kind=?, implementation=?, \
                     dest_operation=?, dest_kind=?, sort_order=?, enabled=?, updated_at=? WHERE id=?",
                    &[
                        arg_integer(input.provider_id),
                        arg_text(&input.operation),
                        arg_text(&input.kind),
                        arg_text(&input.implementation),
                        arg_opt_text(input.dest_operation.as_deref()),
                        arg_opt_text(input.dest_kind.as_deref()),
                        arg_integer(input.sort_order),
                        arg_bool(input.enabled),
                        arg_integer(now),
                        arg_integer(id),
                    ],
                )
                .await
                .map_err(|e| {
                    crate::store::persistence::libsql::conflict_if_unique(e, || {
                        format!(
                            "routing rule already exists for provider {} ({}, {})",
                            input.provider_id, input.operation, input.kind
                        )
                    })
                })?;
            id
        }
        maybe_id => {
            let qr = client
                .execute(
                    "INSERT INTO routing_rules \
                     (id, provider_id, operation, kind, implementation, dest_operation, \
                      dest_kind, sort_order, enabled, created_at, updated_at) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    &[
                        arg_opt_i64(maybe_id),
                        arg_integer(input.provider_id),
                        arg_text(&input.operation),
                        arg_text(&input.kind),
                        arg_text(&input.implementation),
                        arg_opt_text(input.dest_operation.as_deref()),
                        arg_opt_text(input.dest_kind.as_deref()),
                        arg_integer(input.sort_order),
                        arg_bool(input.enabled),
                        arg_integer(now),
                        arg_integer(now),
                    ],
                )
                .await
                .map_err(|e| {
                    crate::store::persistence::libsql::conflict_if_unique(e, || {
                        format!(
                            "routing rule already exists for provider {} ({}, {})",
                            input.provider_id, input.operation, input.kind
                        )
                    })
                })?;
            match maybe_id {
                Some(id) => id,
                None => last_rowid(&qr)?,
            }
        }
    };

    get(client, id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("routing_rule vanished after upsert"))
}

pub async fn delete(client: &LibsqlClient, id: i64) -> anyhow::Result<bool> {
    let n = exec(
        client,
        "DELETE FROM routing_rules WHERE id = ?",
        &[arg_integer(id)],
    )
    .await?;
    Ok(n > 0)
}

pub async fn delete_by_provider(client: &LibsqlClient, provider_id: i64) -> anyhow::Result<()> {
    exec(
        client,
        "DELETE FROM routing_rules WHERE provider_id = ?",
        &[arg_integer(provider_id)],
    )
    .await?;
    Ok(())
}
