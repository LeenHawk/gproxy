//! Credential ops for the libSQL edge backend.

use crate::store::libsql::{LibsqlClient, arg_integer, arg_text};
use crate::store::persistence::libsql::row::{
    Row, col_bool, col_i64, col_json, col_opt_i64, col_opt_json, col_opt_str, col_str,
};
use crate::store::persistence::libsql::util::{
    arg_bool, arg_opt_i64, arg_opt_text, exec, last_rowid, now_secs, query, query_one,
};
use crate::store::persistence::records::{Credential, CredentialInput};

const COLS: &str = "id, provider_id, name, kind, secret_json, weight, rpm_limit, tpm_limit, \
     proxy_url, tls_fingerprint, enabled, created_at, updated_at";

fn decode(row: &Row) -> anyhow::Result<Credential> {
    Ok(Credential {
        id: col_i64(row, 0)?,
        provider_id: col_i64(row, 1)?,
        name: col_opt_str(row, 2)?,
        kind: col_str(row, 3)?,
        secret_json: col_json(row, 4)?,
        weight: col_i64(row, 5)?,
        rpm_limit: col_opt_i64(row, 6)?,
        tpm_limit: col_opt_i64(row, 7)?,
        proxy_url: col_opt_str(row, 8)?,
        tls_fingerprint: col_opt_json(row, 9)?,
        enabled: col_bool(row, 10)?,
        created_at: col_i64(row, 11)?,
        updated_at: col_i64(row, 12)?,
    })
}

pub async fn list(client: &LibsqlClient, provider_id: i64) -> anyhow::Result<Vec<Credential>> {
    query(
        client,
        &format!("SELECT {COLS} FROM credentials WHERE provider_id = ?"),
        &[arg_integer(provider_id)],
    )
    .await?
    .iter()
    .map(decode)
    .collect()
}

pub async fn get(client: &LibsqlClient, id: i64) -> anyhow::Result<Option<Credential>> {
    query_one(
        client,
        &format!("SELECT {COLS} FROM credentials WHERE id = ?"),
        &[arg_integer(id)],
    )
    .await?
    .as_ref()
    .map(decode)
    .transpose()
}

pub async fn upsert(client: &LibsqlClient, input: CredentialInput) -> anyhow::Result<Credential> {
    let now = now_secs();
    let secret = serde_json::to_string(&input.secret_json)?;
    let tls = input
        .tls_fingerprint
        .as_ref()
        .map(serde_json::to_string)
        .transpose()?;

    let id = match input.id {
        Some(id) if get(client, id).await?.is_some() => {
            exec(
                client,
                "UPDATE credentials SET provider_id=?, name=?, kind=?, secret_json=?, weight=?, \
                 rpm_limit=?, tpm_limit=?, proxy_url=?, tls_fingerprint=?, enabled=?, updated_at=? \
                 WHERE id=?",
                &[
                    arg_integer(input.provider_id),
                    arg_opt_text(input.name.as_deref()),
                    arg_text(&input.kind),
                    arg_text(&secret),
                    arg_integer(input.weight),
                    arg_opt_i64(input.rpm_limit),
                    arg_opt_i64(input.tpm_limit),
                    arg_opt_text(input.proxy_url.as_deref()),
                    arg_opt_text(tls.as_deref()),
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
                    "INSERT INTO credentials \
                     (id, provider_id, name, kind, secret_json, weight, rpm_limit, tpm_limit, \
                      proxy_url, tls_fingerprint, enabled, created_at, updated_at) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    &[
                        arg_opt_i64(maybe_id),
                        arg_integer(input.provider_id),
                        arg_opt_text(input.name.as_deref()),
                        arg_text(&input.kind),
                        arg_text(&secret),
                        arg_integer(input.weight),
                        arg_opt_i64(input.rpm_limit),
                        arg_opt_i64(input.tpm_limit),
                        arg_opt_text(input.proxy_url.as_deref()),
                        arg_opt_text(tls.as_deref()),
                        arg_bool(input.enabled),
                        arg_integer(now),
                        arg_integer(now),
                    ],
                )
                .await
                .map_err(|e| anyhow::anyhow!("libsql insert credential: {e}"))?;
            match maybe_id {
                Some(id) => id,
                None => last_rowid(&qr)?,
            }
        }
    };

    get(client, id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("credential vanished after upsert"))
}

pub async fn update_secret_if_current(
    client: &LibsqlClient,
    id: i64,
    provider_id: i64,
    expected_updated_at: i64,
    secret_json: serde_json::Value,
) -> anyhow::Result<bool> {
    let now = now_secs();
    let secret = serde_json::to_string(&secret_json)?;
    let n = exec(
        client,
        "UPDATE credentials SET secret_json=?, updated_at=? \
         WHERE id=? AND provider_id=? AND enabled=? AND updated_at=?",
        &[
            arg_text(&secret),
            arg_integer(now),
            arg_integer(id),
            arg_integer(provider_id),
            arg_bool(true),
            arg_integer(expected_updated_at),
        ],
    )
    .await?;
    Ok(n > 0)
}

pub async fn delete(client: &LibsqlClient, id: i64) -> anyhow::Result<bool> {
    super::credential_statuses::delete_by_credential(client, id).await?;
    let n = exec(
        client,
        "DELETE FROM credentials WHERE id = ?",
        &[arg_integer(id)],
    )
    .await?;
    Ok(n > 0)
}

pub async fn delete_by_provider(client: &LibsqlClient, provider_id: i64) -> anyhow::Result<()> {
    exec(
        client,
        "DELETE FROM credentials WHERE provider_id = ?",
        &[arg_integer(provider_id)],
    )
    .await?;
    Ok(())
}
