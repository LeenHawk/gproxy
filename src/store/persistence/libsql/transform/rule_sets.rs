//! Rule-set ops for the libSQL edge backend. Unique `name`.

use crate::store::libsql::{LibsqlClient, arg_integer, arg_text};
use crate::store::persistence::libsql::row::{Row, col_bool, col_i64, col_opt_str, col_str};
use crate::store::persistence::libsql::util::{
    arg_bool, arg_opt_i64, arg_opt_text, exec, last_rowid, now_secs, query, query_one,
};
use crate::store::persistence::records::{RuleSet, RuleSetInput};

const COLS: &str = "id, name, enabled, description, created_at, updated_at";

fn decode(row: &Row) -> anyhow::Result<RuleSet> {
    Ok(RuleSet {
        id: col_i64(row, 0)?,
        name: col_str(row, 1)?,
        enabled: col_bool(row, 2)?,
        description: col_opt_str(row, 3)?,
        created_at: col_i64(row, 4)?,
        updated_at: col_i64(row, 5)?,
    })
}

pub async fn get(client: &LibsqlClient, id: i64) -> anyhow::Result<Option<RuleSet>> {
    query_one(
        client,
        &format!("SELECT {COLS} FROM rule_sets WHERE id = ?"),
        &[arg_integer(id)],
    )
    .await?
    .as_ref()
    .map(decode)
    .transpose()
}

pub async fn list(client: &LibsqlClient) -> anyhow::Result<Vec<RuleSet>> {
    query(client, &format!("SELECT {COLS} FROM rule_sets"), &[])
        .await?
        .iter()
        .map(decode)
        .collect()
}

pub async fn get_by_name(client: &LibsqlClient, name: &str) -> anyhow::Result<Option<RuleSet>> {
    query_one(
        client,
        &format!("SELECT {COLS} FROM rule_sets WHERE name = ?"),
        &[arg_text(name)],
    )
    .await?
    .as_ref()
    .map(decode)
    .transpose()
}

pub async fn upsert(client: &LibsqlClient, input: RuleSetInput) -> anyhow::Result<RuleSet> {
    let now = now_secs();

    // Enforce uniqueness on `name`.
    if let Some(row) = query_one(
        client,
        "SELECT id FROM rule_sets WHERE name = ?",
        &[arg_text(&input.name)],
    )
    .await?
    {
        let existing = col_i64(&row, 0)?;
        if Some(existing) != input.id {
            return Err(crate::store::persistence::ConflictError::new(format!(
                "rule set name already exists: {}",
                input.name
            ))
            .into());
        }
    }

    let id = match input.id {
        Some(id) if get(client, id).await?.is_some() => {
            client
                .execute(
                    "UPDATE rule_sets SET name=?, enabled=?, description=?, updated_at=? WHERE id=?",
                    &[
                        arg_text(&input.name),
                        arg_bool(input.enabled),
                        arg_opt_text(input.description.as_deref()),
                        arg_integer(now),
                        arg_integer(id),
                    ],
                )
                .await
                .map_err(|e| {
                    crate::store::persistence::libsql::conflict_if_unique(e, || {
                        format!("rule set name already exists: {}", input.name)
                    })
                })?;
            id
        }
        maybe_id => {
            let qr = client
                .execute(
                    "INSERT INTO rule_sets (id, name, enabled, description, created_at, \
                     updated_at) VALUES (?, ?, ?, ?, ?, ?)",
                    &[
                        arg_opt_i64(maybe_id),
                        arg_text(&input.name),
                        arg_bool(input.enabled),
                        arg_opt_text(input.description.as_deref()),
                        arg_integer(now),
                        arg_integer(now),
                    ],
                )
                .await
                .map_err(|e| {
                    crate::store::persistence::libsql::conflict_if_unique(e, || {
                        format!("rule set name already exists: {}", input.name)
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
        .ok_or_else(|| anyhow::anyhow!("rule_set vanished after upsert"))
}

pub async fn delete(client: &LibsqlClient, id: i64) -> anyhow::Result<bool> {
    // cascade: this set's rules and its provider attachments (not the providers).
    super::rules::delete_by_rule_set(client, id).await?;
    super::provider_rule_sets::delete_by_rule_set(client, id).await?;
    let n = exec(
        client,
        "DELETE FROM rule_sets WHERE id = ?",
        &[arg_integer(id)],
    )
    .await?;
    Ok(n > 0)
}
