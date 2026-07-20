//! Credential-model-status ops for the libSQL edge backend.

use crate::store::libsql::{LibsqlClient, arg_integer, arg_text};
use crate::store::persistence::libsql::row::{
    Row, col_i64, col_opt_i64, col_opt_json, col_opt_str, col_str,
};
use crate::store::persistence::libsql::util::{arg_opt_text, exec, now_secs, query, query_one};
use crate::store::persistence::records::{CredentialModelStatus, CredentialModelStatusInput};

const COLS: &str = "id, credential_id, channel, model_id, health_kind, health_json, checked_at, \
     last_error, created_at, updated_at";

fn decode(row: &Row) -> anyhow::Result<CredentialModelStatus> {
    Ok(CredentialModelStatus {
        id: col_i64(row, 0)?,
        credential_id: col_i64(row, 1)?,
        channel: col_str(row, 2)?,
        model_id: col_str(row, 3)?,
        health_kind: col_str(row, 4)?,
        health_json: col_opt_json(row, 5)?,
        checked_at: col_opt_i64(row, 6)?,
        last_error: col_opt_str(row, 7)?,
        created_at: col_i64(row, 8)?,
        updated_at: col_i64(row, 9)?,
    })
}

async fn get(client: &LibsqlClient, id: i64) -> anyhow::Result<Option<CredentialModelStatus>> {
    query_one(
        client,
        &format!("SELECT {COLS} FROM credential_model_statuses WHERE id = ?"),
        &[arg_integer(id)],
    )
    .await?
    .as_ref()
    .map(decode)
    .transpose()
}

pub async fn list(
    client: &LibsqlClient,
    credential_id: i64,
) -> anyhow::Result<Vec<CredentialModelStatus>> {
    query(
        client,
        &format!("SELECT {COLS} FROM credential_model_statuses WHERE credential_id = ?"),
        &[arg_integer(credential_id)],
    )
    .await?
    .iter()
    .map(decode)
    .collect()
}

pub async fn list_all(client: &LibsqlClient) -> anyhow::Result<Vec<CredentialModelStatus>> {
    query(
        client,
        &format!("SELECT {COLS} FROM credential_model_statuses"),
        &[],
    )
    .await?
    .iter()
    .map(decode)
    .collect()
}

pub async fn upsert(
    client: &LibsqlClient,
    input: CredentialModelStatusInput,
) -> anyhow::Result<CredentialModelStatus> {
    let now = now_secs();
    let health = input
        .health_json
        .as_ref()
        .map(serde_json::to_string)
        .transpose()?;

    if let Some(id) = input.id
        && get(client, id).await?.is_some()
    {
        exec(
            client,
            "UPDATE credential_model_statuses SET credential_id=?, channel=?, model_id=?, \
                 health_kind=?, health_json=?, checked_at=?, last_error=?, updated_at=? WHERE id=?",
            &[
                arg_integer(input.credential_id),
                arg_text(&input.channel),
                arg_text(&input.model_id),
                arg_text(&input.health_kind),
                arg_opt_text(health.as_deref()),
                crate::store::persistence::libsql::util::arg_opt_i64(input.checked_at),
                arg_opt_text(input.last_error.as_deref()),
                arg_integer(now),
                arg_integer(id),
            ],
        )
        .await?;
        return get(client, id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("credential_model_status vanished after update"));
    }

    client
        .execute(
            "INSERT INTO credential_model_statuses \
             (credential_id, channel, model_id, health_kind, health_json, checked_at, \
              last_error, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(credential_id, channel, model_id) DO UPDATE SET \
              health_kind=excluded.health_kind, health_json=excluded.health_json, \
              checked_at=excluded.checked_at, last_error=excluded.last_error, \
              updated_at=excluded.updated_at",
            &[
                arg_integer(input.credential_id),
                arg_text(&input.channel),
                arg_text(&input.model_id),
                arg_text(&input.health_kind),
                arg_opt_text(health.as_deref()),
                crate::store::persistence::libsql::util::arg_opt_i64(input.checked_at),
                arg_opt_text(input.last_error.as_deref()),
                arg_integer(now),
                arg_integer(now),
            ],
        )
        .await
        .map_err(|e| anyhow::anyhow!("libsql upsert credential_model_status: {e}"))?;

    query_one(
        client,
        &format!(
            "SELECT {COLS} FROM credential_model_statuses \
             WHERE credential_id = ? AND channel = ? AND model_id = ?"
        ),
        &[
            arg_integer(input.credential_id),
            arg_text(&input.channel),
            arg_text(&input.model_id),
        ],
    )
    .await?
    .as_ref()
    .map(decode)
    .transpose()?
    .ok_or_else(|| anyhow::anyhow!("credential_model_status vanished after upsert"))
}

pub async fn delete(client: &LibsqlClient, id: i64) -> anyhow::Result<bool> {
    let n = exec(
        client,
        "DELETE FROM credential_model_statuses WHERE id = ?",
        &[arg_integer(id)],
    )
    .await?;
    Ok(n > 0)
}

pub async fn delete_by_credential(client: &LibsqlClient, credential_id: i64) -> anyhow::Result<()> {
    exec(
        client,
        "DELETE FROM credential_model_statuses WHERE credential_id = ?",
        &[arg_integer(credential_id)],
    )
    .await?;
    Ok(())
}
