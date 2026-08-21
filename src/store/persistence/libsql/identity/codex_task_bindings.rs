use crate::store::libsql::{LibsqlClient, arg_integer, arg_text};
use crate::store::persistence::libsql::row::{Row, col_i64, col_opt_str, col_str};
use crate::store::persistence::libsql::util::exec;
use crate::store::persistence::libsql::util::{arg_opt_text, now_secs, query, query_one};
use crate::store::persistence::records::{CodexTaskBinding, CodexTaskBindingInput};

const COLS: &str = "id, provider_id, task_id, credential_id, owner_user_id, environment_id, summary_json, created_at, updated_at";

fn decode(row: &Row) -> anyhow::Result<CodexTaskBinding> {
    Ok(CodexTaskBinding {
        id: col_i64(row, 0)?,
        provider_id: col_i64(row, 1)?,
        task_id: col_str(row, 2)?,
        credential_id: col_i64(row, 3)?,
        owner_user_id: col_i64(row, 4)?,
        environment_id: col_opt_str(row, 5)?,
        summary_json: serde_json::from_str(&col_str(row, 6)?)?,
        created_at: col_i64(row, 7)?,
        updated_at: col_i64(row, 8)?,
    })
}

pub async fn get(
    client: &LibsqlClient,
    provider_id: i64,
    task_id: &str,
) -> anyhow::Result<Option<CodexTaskBinding>> {
    query_one(
        client,
        &format!("SELECT {COLS} FROM codex_task_bindings WHERE provider_id = ? AND task_id = ?"),
        &[arg_integer(provider_id), arg_text(task_id)],
    )
    .await?
    .as_ref()
    .map(decode)
    .transpose()
}

pub async fn list(
    client: &LibsqlClient,
    provider_id: i64,
    owner_user_id: i64,
) -> anyhow::Result<Vec<CodexTaskBinding>> {
    query(client, &format!("SELECT {COLS} FROM codex_task_bindings WHERE provider_id = ? AND owner_user_id = ? ORDER BY updated_at DESC, id DESC"), &[arg_integer(provider_id), arg_integer(owner_user_id)])
        .await?.iter().map(decode).collect()
}

pub async fn upsert(
    client: &LibsqlClient,
    input: CodexTaskBindingInput,
) -> anyhow::Result<CodexTaskBinding> {
    let now = now_secs();
    let summary = serde_json::to_string(&input.summary_json)?;
    client.execute(
        "INSERT INTO codex_task_bindings (provider_id, task_id, credential_id, owner_user_id, environment_id, summary_json, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(provider_id, task_id) DO UPDATE SET credential_id=excluded.credential_id, owner_user_id=excluded.owner_user_id, environment_id=excluded.environment_id, summary_json=excluded.summary_json, updated_at=excluded.updated_at",
        &[arg_integer(input.provider_id), arg_text(&input.task_id), arg_integer(input.credential_id), arg_integer(input.owner_user_id), arg_opt_text(input.environment_id.as_deref()), arg_text(&summary), arg_integer(now), arg_integer(now)],
    ).await.map_err(|error| anyhow::anyhow!("libsql codex task binding upsert: {error}"))?;
    get(client, input.provider_id, &input.task_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("codex task binding vanished after upsert"))
}

pub async fn delete_by_user(client: &LibsqlClient, owner_user_id: i64) -> anyhow::Result<()> {
    exec(
        client,
        "DELETE FROM codex_task_bindings WHERE owner_user_id = ?",
        &[arg_integer(owner_user_id)],
    )
    .await?;
    Ok(())
}
