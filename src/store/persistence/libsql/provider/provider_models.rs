//! Provider-model ops for the libSQL edge backend.

use crate::store::libsql::{LibsqlClient, arg_integer, arg_text};
use crate::store::persistence::libsql::row::{
    Row, col_bool, col_i64, col_opt_bool, col_opt_json, col_opt_str, col_str,
};
use crate::store::persistence::libsql::util::{
    arg_bool, arg_opt_bool, arg_opt_i64, arg_opt_text, exec, last_rowid, now_secs, query, query_one,
};
use crate::store::persistence::records::{ProviderModel, ProviderModelInput};

const COLS: &str = "id, provider_id, model_id, display_name, variants_json, \
     context_window, max_output_tokens, thinking_supported, \
     thinking_adaptive_supported, thinking_enabled_supported, enabled, created_at, updated_at";

fn decode(row: &Row) -> anyhow::Result<ProviderModel> {
    Ok(ProviderModel {
        id: col_i64(row, 0)?,
        provider_id: col_i64(row, 1)?,
        model_id: col_str(row, 2)?,
        display_name: col_opt_str(row, 3)?,
        variants_json: col_opt_json(row, 4)?,
        context_window: crate::store::persistence::libsql::row::col_opt_i64(row, 5)?,
        max_output_tokens: crate::store::persistence::libsql::row::col_opt_i64(row, 6)?,
        thinking_supported: col_opt_bool(row, 7)?,
        thinking_adaptive_supported: col_opt_bool(row, 8)?,
        thinking_enabled_supported: col_opt_bool(row, 9)?,
        enabled: col_bool(row, 10)?,
        created_at: col_i64(row, 11)?,
        updated_at: col_i64(row, 12)?,
    })
}

async fn get(client: &LibsqlClient, id: i64) -> anyhow::Result<Option<ProviderModel>> {
    query_one(
        client,
        &format!("SELECT {COLS} FROM provider_models WHERE id = ?"),
        &[arg_integer(id)],
    )
    .await?
    .as_ref()
    .map(decode)
    .transpose()
}

pub async fn list(client: &LibsqlClient, provider_id: i64) -> anyhow::Result<Vec<ProviderModel>> {
    query(
        client,
        &format!("SELECT {COLS} FROM provider_models WHERE provider_id = ?"),
        &[arg_integer(provider_id)],
    )
    .await?
    .iter()
    .map(decode)
    .collect()
}

pub async fn list_all(client: &LibsqlClient) -> anyhow::Result<Vec<ProviderModel>> {
    query(client, &format!("SELECT {COLS} FROM provider_models"), &[])
        .await?
        .iter()
        .map(decode)
        .collect()
}

pub async fn upsert(
    client: &LibsqlClient,
    input: ProviderModelInput,
) -> anyhow::Result<ProviderModel> {
    let now = now_secs();
    let variants = input
        .variants_json
        .as_ref()
        .map(serde_json::to_string)
        .transpose()?;

    let id = match input.id {
        Some(id) if get(client, id).await?.is_some() => {
            exec(
                client,
                "UPDATE provider_models SET provider_id=?, model_id=?, display_name=?, \
                 variants_json=?, context_window=?, max_output_tokens=?, \
                 thinking_supported=?, thinking_adaptive_supported=?, thinking_enabled_supported=?, \
                 enabled=?, updated_at=? WHERE id=?",
                &[
                    arg_integer(input.provider_id),
                    arg_text(&input.model_id),
                    arg_opt_text(input.display_name.as_deref()),
                    arg_opt_text(variants.as_deref()),
                    arg_opt_i64(input.context_window),
                    arg_opt_i64(input.max_output_tokens),
                    arg_opt_bool(input.thinking_supported),
                    arg_opt_bool(input.thinking_adaptive_supported),
                    arg_opt_bool(input.thinking_enabled_supported),
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
                    "INSERT INTO provider_models \
                     (id, provider_id, model_id, display_name, variants_json, \
                      context_window, max_output_tokens, thinking_supported, \
                      thinking_adaptive_supported, thinking_enabled_supported, enabled, created_at, updated_at) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    &[
                        arg_opt_i64(maybe_id),
                        arg_integer(input.provider_id),
                        arg_text(&input.model_id),
                        arg_opt_text(input.display_name.as_deref()),
                        arg_opt_text(variants.as_deref()),
                        arg_opt_i64(input.context_window),
                        arg_opt_i64(input.max_output_tokens),
                        arg_opt_bool(input.thinking_supported),
                        arg_opt_bool(input.thinking_adaptive_supported),
                        arg_opt_bool(input.thinking_enabled_supported),
                        arg_bool(input.enabled),
                        arg_integer(now),
                        arg_integer(now),
                    ],
                )
                .await
                .map_err(|e| anyhow::anyhow!("libsql insert provider_model: {e}"))?;
            match maybe_id {
                Some(id) => id,
                None => last_rowid(&qr)?,
            }
        }
    };

    get(client, id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("provider_model vanished after upsert"))
}

pub async fn delete(client: &LibsqlClient, id: i64) -> anyhow::Result<bool> {
    let n = exec(
        client,
        "DELETE FROM provider_models WHERE id = ?",
        &[arg_integer(id)],
    )
    .await?;
    Ok(n > 0)
}

pub async fn delete_by_provider(client: &LibsqlClient, provider_id: i64) -> anyhow::Result<()> {
    exec(
        client,
        "DELETE FROM provider_models WHERE provider_id = ?",
        &[arg_integer(provider_id)],
    )
    .await?;
    Ok(())
}
