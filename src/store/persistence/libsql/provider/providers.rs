//! Provider ops for the libSQL edge backend. Mirrors `db/ops/provider/providers`.

use crate::store::libsql::{LibsqlClient, arg_integer, arg_text};
use crate::store::persistence::libsql::row::{
    Row, col_bool, col_i64, col_json, col_opt_json, col_opt_str, col_str,
};
use crate::store::persistence::libsql::util::{
    arg_bool, arg_opt_text, exec, last_rowid, now_secs, query, query_one,
};
use crate::store::persistence::records::{Provider, ProviderInput};

const COLS: &str = "id, name, channel, label, settings_json, credential_strategy, \
     proxy_url, tls_fingerprint, enabled, created_at, updated_at";

fn decode(row: &Row) -> anyhow::Result<Provider> {
    Ok(Provider {
        id: col_i64(row, 0)?,
        name: col_str(row, 1)?,
        channel: col_str(row, 2)?,
        label: col_opt_str(row, 3)?,
        settings_json: col_json(row, 4)?,
        credential_strategy: col_str(row, 5)?,
        proxy_url: col_opt_str(row, 6)?,
        tls_fingerprint: col_opt_json(row, 7)?,
        enabled: col_bool(row, 8)?,
        created_at: col_i64(row, 9)?,
        updated_at: col_i64(row, 10)?,
    })
}

pub async fn list(client: &LibsqlClient) -> anyhow::Result<Vec<Provider>> {
    query(client, &format!("SELECT {COLS} FROM providers"), &[])
        .await?
        .iter()
        .map(decode)
        .collect()
}

pub async fn get(client: &LibsqlClient, id: i64) -> anyhow::Result<Option<Provider>> {
    query_one(
        client,
        &format!("SELECT {COLS} FROM providers WHERE id = ?"),
        &[arg_integer(id)],
    )
    .await?
    .as_ref()
    .map(decode)
    .transpose()
}

pub async fn get_by_name(client: &LibsqlClient, name: &str) -> anyhow::Result<Option<Provider>> {
    query_one(
        client,
        &format!("SELECT {COLS} FROM providers WHERE name = ?"),
        &[arg_text(name)],
    )
    .await?
    .as_ref()
    .map(decode)
    .transpose()
}

pub async fn upsert(client: &LibsqlClient, input: ProviderInput) -> anyhow::Result<Provider> {
    let now = now_secs();
    let settings = serde_json::to_string(&input.settings_json)?;
    let tls = input
        .tls_fingerprint
        .as_ref()
        .map(serde_json::to_string)
        .transpose()?;

    let id = match input.id {
        Some(id) if get(client, id).await?.is_some() => {
            client
                .execute(
                    "UPDATE providers SET name=?, channel=?, label=?, settings_json=?, \
                     credential_strategy=?, proxy_url=?, tls_fingerprint=?, enabled=?, updated_at=? \
                     WHERE id=?",
                    &[
                        arg_text(&input.name),
                        arg_text(&input.channel),
                        arg_opt_text(input.label.as_deref()),
                        arg_text(&settings),
                        arg_text(&input.credential_strategy),
                        arg_opt_text(input.proxy_url.as_deref()),
                        arg_opt_text(tls.as_deref()),
                        arg_bool(input.enabled),
                        arg_integer(now),
                        arg_integer(id),
                    ],
                )
                .await
                .map_err(|e| {
                    crate::store::persistence::libsql::conflict_if_unique(e, || {
                        format!("provider name already exists: {}", input.name)
                    })
                })?;
            id
        }
        maybe_id => {
            let qr = client
                .execute(
                    "INSERT INTO providers \
                     (id, name, channel, label, settings_json, credential_strategy, \
                      proxy_url, tls_fingerprint, enabled, created_at, updated_at) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    &[
                        crate::store::persistence::libsql::util::arg_opt_i64(maybe_id),
                        arg_text(&input.name),
                        arg_text(&input.channel),
                        arg_opt_text(input.label.as_deref()),
                        arg_text(&settings),
                        arg_text(&input.credential_strategy),
                        arg_opt_text(input.proxy_url.as_deref()),
                        arg_opt_text(tls.as_deref()),
                        arg_bool(input.enabled),
                        arg_integer(now),
                        arg_integer(now),
                    ],
                )
                .await
                .map_err(|e| {
                    crate::store::persistence::libsql::conflict_if_unique(e, || {
                        format!("provider name already exists: {}", input.name)
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
        .ok_or_else(|| anyhow::anyhow!("provider vanished after upsert"))
}

pub async fn delete(client: &LibsqlClient, id: i64) -> anyhow::Result<bool> {
    // cascade: credentials (+ their statuses), models, routing rules, rule-set attachments.
    for cred in super::credentials::list(client, id).await? {
        super::credential_statuses::delete_by_credential(client, cred.id).await?;
    }
    super::credentials::delete_by_provider(client, id).await?;
    super::provider_models::delete_by_provider(client, id).await?;
    crate::store::persistence::libsql::transform::routing_rules::delete_by_provider(client, id)
        .await?;
    crate::store::persistence::libsql::transform::provider_rule_sets::delete_by_provider(
        client, id,
    )
    .await?;

    let n = exec(
        client,
        "DELETE FROM providers WHERE id = ?",
        &[arg_integer(id)],
    )
    .await?;
    Ok(n > 0)
}
