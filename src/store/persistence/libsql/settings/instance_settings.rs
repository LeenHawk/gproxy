//! Instance-settings ops for the libSQL edge backend. Unique `instance_name`.

use crate::store::libsql::{LibsqlClient, arg_integer, arg_text};
use crate::store::persistence::libsql::row::{
    Row, col_bool, col_i64, col_opt_bool, col_opt_i64, col_opt_str, col_str,
};
use crate::store::persistence::libsql::util::{
    arg_bool, arg_opt_bool, arg_opt_i64, arg_opt_text, last_rowid, now_secs, query, query_one,
};
use crate::store::persistence::records::{InstanceSettings, InstanceSettingsInput};

const COLS: &str = "id, instance_name, proxy, spoof_emulation, enable_usage, enable_upstream_log, \
     enable_upstream_log_body, enable_downstream_log, enable_downstream_log_body, \
     disable_log_redaction, enable_tokenizer_download, update_channel, retention_days, \
     created_at, updated_at";

fn decode(row: &Row) -> anyhow::Result<InstanceSettings> {
    Ok(InstanceSettings {
        id: col_i64(row, 0)?,
        instance_name: col_str(row, 1)?,
        proxy: col_opt_str(row, 2)?,
        spoof_emulation: col_opt_bool(row, 3)?,
        enable_usage: col_bool(row, 4)?,
        enable_upstream_log: col_bool(row, 5)?,
        enable_upstream_log_body: col_bool(row, 6)?,
        enable_downstream_log: col_bool(row, 7)?,
        enable_downstream_log_body: col_bool(row, 8)?,
        disable_log_redaction: col_bool(row, 9)?,
        enable_tokenizer_download: col_bool(row, 10)?,
        update_channel: col_opt_str(row, 11)?,
        retention_days: col_opt_i64(row, 12)?,
        created_at: col_i64(row, 13)?,
        updated_at: col_i64(row, 14)?,
    })
}

async fn get_by_id(client: &LibsqlClient, id: i64) -> anyhow::Result<Option<InstanceSettings>> {
    query_one(
        client,
        &format!("SELECT {COLS} FROM instance_settings WHERE id = ?"),
        &[arg_integer(id)],
    )
    .await?
    .as_ref()
    .map(decode)
    .transpose()
}

pub async fn list(client: &LibsqlClient) -> anyhow::Result<Vec<InstanceSettings>> {
    query(
        client,
        &format!("SELECT {COLS} FROM instance_settings"),
        &[],
    )
    .await?
    .iter()
    .map(decode)
    .collect()
}

pub async fn get(
    client: &LibsqlClient,
    instance_name: &str,
) -> anyhow::Result<Option<InstanceSettings>> {
    query_one(
        client,
        &format!("SELECT {COLS} FROM instance_settings WHERE instance_name = ?"),
        &[arg_text(instance_name)],
    )
    .await?
    .as_ref()
    .map(decode)
    .transpose()
}

pub async fn upsert(
    client: &LibsqlClient,
    input: InstanceSettingsInput,
) -> anyhow::Result<InstanceSettings> {
    let now = now_secs();

    let id = match input.id {
        Some(id) if get_by_id(client, id).await?.is_some() => {
            client
                .execute(
                    "UPDATE instance_settings SET instance_name=?, proxy=?, spoof_emulation=?, \
                     enable_usage=?, enable_upstream_log=?, enable_upstream_log_body=?, \
                     enable_downstream_log=?, enable_downstream_log_body=?, disable_log_redaction=?, \
                     enable_tokenizer_download=?, update_channel=?, retention_days=?, updated_at=? \
                     WHERE id=?",
                    &[
                        arg_text(&input.instance_name),
                        arg_opt_text(input.proxy.as_deref()),
                        arg_opt_bool(input.spoof_emulation),
                        arg_bool(input.enable_usage),
                        arg_bool(input.enable_upstream_log),
                        arg_bool(input.enable_upstream_log_body),
                        arg_bool(input.enable_downstream_log),
                        arg_bool(input.enable_downstream_log_body),
                        arg_bool(input.disable_log_redaction),
                        arg_bool(input.enable_tokenizer_download),
                        arg_opt_text(input.update_channel.as_deref()),
                        arg_opt_i64(input.retention_days),
                        arg_integer(now),
                        arg_integer(id),
                    ],
                )
                .await
                .map_err(|e| {
                    crate::store::persistence::libsql::conflict_if_unique(e, || {
                        format!("instance name already exists: {}", input.instance_name)
                    })
                })?;
            id
        }
        maybe_id => {
            let qr = client
                .execute(
                    "INSERT INTO instance_settings \
                     (id, instance_name, proxy, spoof_emulation, enable_usage, \
                      enable_upstream_log, enable_upstream_log_body, enable_downstream_log, \
                      enable_downstream_log_body, disable_log_redaction, \
                      enable_tokenizer_download, update_channel, retention_days, \
                      created_at, updated_at) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    &[
                        arg_opt_i64(maybe_id),
                        arg_text(&input.instance_name),
                        arg_opt_text(input.proxy.as_deref()),
                        arg_opt_bool(input.spoof_emulation),
                        arg_bool(input.enable_usage),
                        arg_bool(input.enable_upstream_log),
                        arg_bool(input.enable_upstream_log_body),
                        arg_bool(input.enable_downstream_log),
                        arg_bool(input.enable_downstream_log_body),
                        arg_bool(input.disable_log_redaction),
                        arg_bool(input.enable_tokenizer_download),
                        arg_opt_text(input.update_channel.as_deref()),
                        arg_opt_i64(input.retention_days),
                        arg_integer(now),
                        arg_integer(now),
                    ],
                )
                .await
                .map_err(|e| {
                    crate::store::persistence::libsql::conflict_if_unique(e, || {
                        format!("instance name already exists: {}", input.instance_name)
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
        .ok_or_else(|| anyhow::anyhow!("instance_settings vanished after upsert"))
}
