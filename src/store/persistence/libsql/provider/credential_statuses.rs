//! Credential-status ops for the libSQL edge backend.
//! Upsert is keyed by explicit id, else by unique `(credential_id, channel)`.

use crate::store::libsql::{LibsqlClient, arg_integer, arg_text};
use crate::store::persistence::libsql::row::{
    Row, col_i64, col_opt_i64, col_opt_json, col_opt_str, col_str,
};
use crate::store::persistence::libsql::util::{
    arg_opt_text, exec, last_rowid, now_secs, query, query_one,
};
use crate::store::persistence::records::{CredentialStatus, CredentialStatusInput};

const COLS: &str = "id, credential_id, channel, health_kind, health_json, checked_at, \
     last_error, created_at, updated_at";

fn decode(row: &Row) -> anyhow::Result<CredentialStatus> {
    Ok(CredentialStatus {
        id: col_i64(row, 0)?,
        credential_id: col_i64(row, 1)?,
        channel: col_str(row, 2)?,
        health_kind: col_str(row, 3)?,
        health_json: col_opt_json(row, 4)?,
        checked_at: col_opt_i64(row, 5)?,
        last_error: col_opt_str(row, 6)?,
        created_at: col_i64(row, 7)?,
        updated_at: col_i64(row, 8)?,
    })
}

async fn get(client: &LibsqlClient, id: i64) -> anyhow::Result<Option<CredentialStatus>> {
    query_one(
        client,
        &format!("SELECT {COLS} FROM credential_statuses WHERE id = ?"),
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
) -> anyhow::Result<Vec<CredentialStatus>> {
    query(
        client,
        &format!("SELECT {COLS} FROM credential_statuses WHERE credential_id = ?"),
        &[arg_integer(credential_id)],
    )
    .await?
    .iter()
    .map(decode)
    .collect()
}

pub async fn list_all(client: &LibsqlClient) -> anyhow::Result<Vec<CredentialStatus>> {
    query(
        client,
        &format!("SELECT {COLS} FROM credential_statuses"),
        &[],
    )
    .await?
    .iter()
    .map(decode)
    .collect()
}

pub async fn upsert(
    client: &LibsqlClient,
    input: CredentialStatusInput,
) -> anyhow::Result<CredentialStatus> {
    let now = now_secs();
    let health = input
        .health_json
        .as_ref()
        .map(serde_json::to_string)
        .transpose()?;

    // Locate by explicit id, else by (credential_id, channel) uniqueness.
    let existing_id = match input.id {
        Some(id) => get(client, id).await?.map(|r| r.id),
        None => query_one(
            client,
            "SELECT id FROM credential_statuses WHERE credential_id = ? AND channel = ?",
            &[arg_integer(input.credential_id), arg_text(&input.channel)],
        )
        .await?
        .as_ref()
        .map(|r| col_i64(r, 0))
        .transpose()?,
    };

    let id = match existing_id {
        Some(id) => {
            exec(
                client,
                "UPDATE credential_statuses SET credential_id=?, channel=?, health_kind=?, \
                 health_json=?, checked_at=?, last_error=?, updated_at=? WHERE id=?",
                &[
                    arg_integer(input.credential_id),
                    arg_text(&input.channel),
                    arg_text(&input.health_kind),
                    arg_opt_text(health.as_deref()),
                    crate::store::persistence::libsql::util::arg_opt_i64(input.checked_at),
                    arg_opt_text(input.last_error.as_deref()),
                    arg_integer(now),
                    arg_integer(id),
                ],
            )
            .await?;
            id
        }
        None => {
            let qr = client
                .execute(
                    "INSERT INTO credential_statuses \
                     (credential_id, channel, health_kind, health_json, checked_at, last_error, \
                      created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                    &[
                        arg_integer(input.credential_id),
                        arg_text(&input.channel),
                        arg_text(&input.health_kind),
                        arg_opt_text(health.as_deref()),
                        crate::store::persistence::libsql::util::arg_opt_i64(input.checked_at),
                        arg_opt_text(input.last_error.as_deref()),
                        arg_integer(now),
                        arg_integer(now),
                    ],
                )
                .await
                .map_err(|e| anyhow::anyhow!("libsql insert credential_status: {e}"))?;
            last_rowid(&qr)?
        }
    };

    get(client, id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("credential_status vanished after upsert"))
}

pub async fn delete(client: &LibsqlClient, id: i64) -> anyhow::Result<bool> {
    let n = exec(
        client,
        "DELETE FROM credential_statuses WHERE id = ?",
        &[arg_integer(id)],
    )
    .await?;
    Ok(n > 0)
}

pub async fn delete_by_credential(client: &LibsqlClient, credential_id: i64) -> anyhow::Result<()> {
    exec(
        client,
        "DELETE FROM credential_statuses WHERE credential_id = ?",
        &[arg_integer(credential_id)],
    )
    .await?;
    Ok(())
}
