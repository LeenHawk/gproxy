//! Provider ↔ rule-set attachment ops for the libSQL edge backend.

use crate::store::libsql::{LibsqlClient, arg_integer};
use crate::store::persistence::libsql::row::{Row, col_bool, col_i64};
use crate::store::persistence::libsql::util::{
    arg_bool, arg_opt_i64, exec, last_rowid, now_secs, query, query_one,
};
use crate::store::persistence::records::{ProviderRuleSet, ProviderRuleSetInput};

const COLS: &str = "id, provider_id, rule_set_id, sort_order, enabled, created_at, updated_at";

fn decode(row: &Row) -> anyhow::Result<ProviderRuleSet> {
    Ok(ProviderRuleSet {
        id: col_i64(row, 0)?,
        provider_id: col_i64(row, 1)?,
        rule_set_id: col_i64(row, 2)?,
        sort_order: col_i64(row, 3)?,
        enabled: col_bool(row, 4)?,
        created_at: col_i64(row, 5)?,
        updated_at: col_i64(row, 6)?,
    })
}

async fn get(client: &LibsqlClient, id: i64) -> anyhow::Result<Option<ProviderRuleSet>> {
    query_one(
        client,
        &format!("SELECT {COLS} FROM provider_rule_sets WHERE id = ?"),
        &[arg_integer(id)],
    )
    .await?
    .as_ref()
    .map(decode)
    .transpose()
}

pub async fn list(client: &LibsqlClient, provider_id: i64) -> anyhow::Result<Vec<ProviderRuleSet>> {
    query(
        client,
        &format!("SELECT {COLS} FROM provider_rule_sets WHERE provider_id = ?"),
        &[arg_integer(provider_id)],
    )
    .await?
    .iter()
    .map(decode)
    .collect()
}

pub async fn upsert(
    client: &LibsqlClient,
    input: ProviderRuleSetInput,
) -> anyhow::Result<ProviderRuleSet> {
    let now = now_secs();

    let id = match input.id {
        Some(id) if get(client, id).await?.is_some() => {
            exec(
                client,
                "UPDATE provider_rule_sets SET provider_id=?, rule_set_id=?, sort_order=?, \
                 enabled=?, updated_at=? WHERE id=?",
                &[
                    arg_integer(input.provider_id),
                    arg_integer(input.rule_set_id),
                    arg_integer(input.sort_order),
                    arg_bool(input.enabled),
                    arg_integer(now),
                    arg_integer(id),
                ],
            )
            .await?;
            id
        }
        maybe_id => {
            let qr = client
                .execute(
                    "INSERT INTO provider_rule_sets \
                     (id, provider_id, rule_set_id, sort_order, enabled, created_at, updated_at) \
                     VALUES (?, ?, ?, ?, ?, ?, ?)",
                    &[
                        arg_opt_i64(maybe_id),
                        arg_integer(input.provider_id),
                        arg_integer(input.rule_set_id),
                        arg_integer(input.sort_order),
                        arg_bool(input.enabled),
                        arg_integer(now),
                        arg_integer(now),
                    ],
                )
                .await
                .map_err(|e| anyhow::anyhow!("libsql insert provider_rule_set: {e}"))?;
            match maybe_id {
                Some(id) => id,
                None => last_rowid(&qr)?,
            }
        }
    };

    get(client, id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("provider_rule_set vanished after upsert"))
}

pub async fn delete(client: &LibsqlClient, id: i64) -> anyhow::Result<bool> {
    let n = exec(
        client,
        "DELETE FROM provider_rule_sets WHERE id = ?",
        &[arg_integer(id)],
    )
    .await?;
    Ok(n > 0)
}

pub async fn delete_by_provider(client: &LibsqlClient, provider_id: i64) -> anyhow::Result<()> {
    exec(
        client,
        "DELETE FROM provider_rule_sets WHERE provider_id = ?",
        &[arg_integer(provider_id)],
    )
    .await?;
    Ok(())
}

pub async fn delete_by_rule_set(client: &LibsqlClient, rule_set_id: i64) -> anyhow::Result<()> {
    exec(
        client,
        "DELETE FROM provider_rule_sets WHERE rule_set_id = ?",
        &[arg_integer(rule_set_id)],
    )
    .await?;
    Ok(())
}
