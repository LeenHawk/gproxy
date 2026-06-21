//! User-key ops for the libSQL edge backend. `api_key_digest` is unique.

use crate::store::libsql::{LibsqlClient, arg_integer, arg_text};
use crate::store::persistence::libsql::row::{Row, col_bool, col_i64, col_opt_str, col_str};
use crate::store::persistence::libsql::util::{
    arg_bool, arg_opt_i64, arg_opt_text, exec, last_rowid, now_secs, query, query_one,
};
use crate::store::persistence::records::{UserKey, UserKeyInput};

const COLS: &str = "id, user_id, api_key_ciphertext, api_key_digest, label, enabled, \
     created_at, updated_at";

fn decode(row: &Row) -> anyhow::Result<UserKey> {
    Ok(UserKey {
        id: col_i64(row, 0)?,
        user_id: col_i64(row, 1)?,
        api_key_ciphertext: col_str(row, 2)?,
        api_key_digest: col_str(row, 3)?,
        label: col_opt_str(row, 4)?,
        enabled: col_bool(row, 5)?,
        created_at: col_i64(row, 6)?,
        updated_at: col_i64(row, 7)?,
    })
}

pub async fn get(client: &LibsqlClient, id: i64) -> anyhow::Result<Option<UserKey>> {
    query_one(
        client,
        &format!("SELECT {COLS} FROM user_keys WHERE id = ?"),
        &[arg_integer(id)],
    )
    .await?
    .as_ref()
    .map(decode)
    .transpose()
}

pub async fn list(client: &LibsqlClient, user_id: i64) -> anyhow::Result<Vec<UserKey>> {
    query(
        client,
        &format!("SELECT {COLS} FROM user_keys WHERE user_id = ?"),
        &[arg_integer(user_id)],
    )
    .await?
    .iter()
    .map(decode)
    .collect()
}

pub async fn find_by_digest(
    client: &LibsqlClient,
    digest: &str,
) -> anyhow::Result<Option<UserKey>> {
    query_one(
        client,
        &format!("SELECT {COLS} FROM user_keys WHERE api_key_digest = ?"),
        &[arg_text(digest)],
    )
    .await?
    .as_ref()
    .map(decode)
    .transpose()
}

pub async fn upsert(client: &LibsqlClient, input: UserKeyInput) -> anyhow::Result<UserKey> {
    let now = now_secs();

    let id = match input.id {
        Some(id) if get(client, id).await?.is_some() => {
            client
                .execute(
                    "UPDATE user_keys SET user_id=?, api_key_ciphertext=?, api_key_digest=?, label=?, \
                     enabled=?, updated_at=? WHERE id=?",
                    &[
                        arg_integer(input.user_id),
                        arg_text(&input.api_key_ciphertext),
                        arg_text(&input.api_key_digest),
                        arg_opt_text(input.label.as_deref()),
                        arg_bool(input.enabled),
                        arg_integer(now),
                        arg_integer(id),
                    ],
                )
                .await
                .map_err(|e| {
                    crate::store::persistence::libsql::conflict_if_unique(e, || {
                        format!("user key digest already exists: {}", input.api_key_digest)
                    })
                })?;
            id
        }
        maybe_id => {
            let qr = client
                .execute(
                    "INSERT INTO user_keys \
                     (id, user_id, api_key_ciphertext, api_key_digest, label, enabled, \
                      created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                    &[
                        arg_opt_i64(maybe_id),
                        arg_integer(input.user_id),
                        arg_text(&input.api_key_ciphertext),
                        arg_text(&input.api_key_digest),
                        arg_opt_text(input.label.as_deref()),
                        arg_bool(input.enabled),
                        arg_integer(now),
                        arg_integer(now),
                    ],
                )
                .await
                .map_err(|e| {
                    crate::store::persistence::libsql::conflict_if_unique(e, || {
                        format!("user key digest already exists: {}", input.api_key_digest)
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
        .ok_or_else(|| anyhow::anyhow!("user_key vanished after upsert"))
}

pub async fn delete(client: &LibsqlClient, id: i64) -> anyhow::Result<bool> {
    let n = exec(
        client,
        "DELETE FROM user_keys WHERE id = ?",
        &[arg_integer(id)],
    )
    .await?;
    Ok(n > 0)
}

pub async fn delete_by_user(client: &LibsqlClient, user_id: i64) -> anyhow::Result<()> {
    exec(
        client,
        "DELETE FROM user_keys WHERE user_id = ?",
        &[arg_integer(user_id)],
    )
    .await?;
    Ok(())
}
