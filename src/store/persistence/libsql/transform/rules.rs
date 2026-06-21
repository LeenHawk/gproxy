//! Rule ops for the libSQL edge backend.

use crate::store::libsql::{LibsqlClient, arg_integer, arg_text};
use crate::store::persistence::libsql::row::{
    Row, col_bool, col_i64, col_json, col_opt_json, col_opt_str, col_str,
};
use crate::store::persistence::libsql::util::{
    arg_bool, arg_opt_i64, arg_opt_text, exec, last_rowid, now_secs, query, query_one,
};
use crate::store::persistence::records::{Rule, RuleInput};

const COLS: &str = "id, rule_set_id, kind, config_json, filter_model_pattern, \
     filter_operation_keys, sort_order, enabled, created_at, updated_at";

fn decode(row: &Row) -> anyhow::Result<Rule> {
    Ok(Rule {
        id: col_i64(row, 0)?,
        rule_set_id: col_i64(row, 1)?,
        kind: col_str(row, 2)?,
        config_json: col_json(row, 3)?,
        filter_model_pattern: col_opt_str(row, 4)?,
        filter_operation_keys: col_opt_json(row, 5)?,
        sort_order: col_i64(row, 6)?,
        enabled: col_bool(row, 7)?,
        created_at: col_i64(row, 8)?,
        updated_at: col_i64(row, 9)?,
    })
}

pub async fn get(client: &LibsqlClient, id: i64) -> anyhow::Result<Option<Rule>> {
    query_one(
        client,
        &format!("SELECT {COLS} FROM rules WHERE id = ?"),
        &[arg_integer(id)],
    )
    .await?
    .as_ref()
    .map(decode)
    .transpose()
}

pub async fn list(client: &LibsqlClient, rule_set_id: i64) -> anyhow::Result<Vec<Rule>> {
    query(
        client,
        &format!("SELECT {COLS} FROM rules WHERE rule_set_id = ?"),
        &[arg_integer(rule_set_id)],
    )
    .await?
    .iter()
    .map(decode)
    .collect()
}

pub async fn upsert(client: &LibsqlClient, input: RuleInput) -> anyhow::Result<Rule> {
    let now = now_secs();
    let config = serde_json::to_string(&input.config_json)?;
    let filter_keys = input
        .filter_operation_keys
        .as_ref()
        .map(serde_json::to_string)
        .transpose()?;

    let id = match input.id {
        Some(id) if get(client, id).await?.is_some() => {
            exec(
                client,
                "UPDATE rules SET rule_set_id=?, kind=?, config_json=?, filter_model_pattern=?, \
                 filter_operation_keys=?, sort_order=?, enabled=?, updated_at=? WHERE id=?",
                &[
                    arg_integer(input.rule_set_id),
                    arg_text(&input.kind),
                    arg_text(&config),
                    arg_opt_text(input.filter_model_pattern.as_deref()),
                    arg_opt_text(filter_keys.as_deref()),
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
                    "INSERT INTO rules \
                     (id, rule_set_id, kind, config_json, filter_model_pattern, \
                      filter_operation_keys, sort_order, enabled, created_at, updated_at) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    &[
                        arg_opt_i64(maybe_id),
                        arg_integer(input.rule_set_id),
                        arg_text(&input.kind),
                        arg_text(&config),
                        arg_opt_text(input.filter_model_pattern.as_deref()),
                        arg_opt_text(filter_keys.as_deref()),
                        arg_integer(input.sort_order),
                        arg_bool(input.enabled),
                        arg_integer(now),
                        arg_integer(now),
                    ],
                )
                .await
                .map_err(|e| anyhow::anyhow!("libsql insert rule: {e}"))?;
            match maybe_id {
                Some(id) => id,
                None => last_rowid(&qr)?,
            }
        }
    };

    get(client, id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("rule vanished after upsert"))
}

pub async fn delete(client: &LibsqlClient, id: i64) -> anyhow::Result<bool> {
    let n = exec(client, "DELETE FROM rules WHERE id = ?", &[arg_integer(id)]).await?;
    Ok(n > 0)
}

pub async fn delete_by_rule_set(client: &LibsqlClient, rule_set_id: i64) -> anyhow::Result<()> {
    exec(
        client,
        "DELETE FROM rules WHERE rule_set_id = ?",
        &[arg_integer(rule_set_id)],
    )
    .await?;
    Ok(())
}
