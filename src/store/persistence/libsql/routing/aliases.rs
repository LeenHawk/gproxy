//! Alias ops for the libSQL edge backend.

use crate::store::libsql::{LibsqlClient, arg_integer, arg_text};
use crate::store::persistence::libsql::row::{Row, col_i64, col_str};
use crate::store::persistence::libsql::util::{
    arg_opt_i64, exec, last_rowid, now_secs, query, query_one,
};
use crate::store::persistence::records::{Alias, AliasInput};

const COLS: &str = "id, alias, route_id, created_at, updated_at";

fn decode(row: &Row) -> anyhow::Result<Alias> {
    Ok(Alias {
        id: col_i64(row, 0)?,
        alias: col_str(row, 1)?,
        route_id: col_i64(row, 2)?,
        created_at: col_i64(row, 3)?,
        updated_at: col_i64(row, 4)?,
    })
}

async fn get(client: &LibsqlClient, id: i64) -> anyhow::Result<Option<Alias>> {
    query_one(
        client,
        &format!("SELECT {COLS} FROM aliases WHERE id = ?"),
        &[arg_integer(id)],
    )
    .await?
    .as_ref()
    .map(decode)
    .transpose()
}

pub async fn list(client: &LibsqlClient) -> anyhow::Result<Vec<Alias>> {
    query(client, &format!("SELECT {COLS} FROM aliases"), &[])
        .await?
        .iter()
        .map(decode)
        .collect()
}

pub async fn get_by_name(client: &LibsqlClient, value: &str) -> anyhow::Result<Option<Alias>> {
    query_one(
        client,
        &format!("SELECT {COLS} FROM aliases WHERE alias = ?"),
        &[arg_text(value)],
    )
    .await?
    .as_ref()
    .map(decode)
    .transpose()
}

pub async fn upsert(client: &LibsqlClient, input: AliasInput) -> anyhow::Result<Alias> {
    let now = now_secs();

    let id = match input.id {
        Some(id) if get(client, id).await?.is_some() => {
            client
                .execute(
                    "UPDATE aliases SET alias=?, route_id=?, updated_at=? WHERE id=?",
                    &[
                        arg_text(&input.alias),
                        arg_integer(input.route_id),
                        arg_integer(now),
                        arg_integer(id),
                    ],
                )
                .await
                .map_err(|e| {
                    crate::store::persistence::libsql::conflict_if_unique(e, || {
                        format!("alias already exists: {}", input.alias)
                    })
                })?;
            id
        }
        maybe_id => {
            let qr = client
                .execute(
                    "INSERT INTO aliases (id, alias, route_id, created_at, updated_at) \
                     VALUES (?, ?, ?, ?, ?)",
                    &[
                        arg_opt_i64(maybe_id),
                        arg_text(&input.alias),
                        arg_integer(input.route_id),
                        arg_integer(now),
                        arg_integer(now),
                    ],
                )
                .await
                .map_err(|e| {
                    crate::store::persistence::libsql::conflict_if_unique(e, || {
                        format!("alias already exists: {}", input.alias)
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
        .ok_or_else(|| anyhow::anyhow!("alias vanished after upsert"))
}

pub async fn delete(client: &LibsqlClient, id: i64) -> anyhow::Result<bool> {
    let n = exec(
        client,
        "DELETE FROM aliases WHERE id = ?",
        &[arg_integer(id)],
    )
    .await?;
    Ok(n > 0)
}

pub async fn delete_by_route(client: &LibsqlClient, route_id: i64) -> anyhow::Result<()> {
    exec(
        client,
        "DELETE FROM aliases WHERE route_id = ?",
        &[arg_integer(route_id)],
    )
    .await?;
    Ok(())
}
