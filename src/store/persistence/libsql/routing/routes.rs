//! Route ops for the libSQL edge backend.

use crate::store::libsql::{LibsqlClient, arg_integer, arg_text};
use crate::store::persistence::libsql::row::{
    Row, col_bool, col_i64, col_opt_json, col_opt_str, col_str,
};
use crate::store::persistence::libsql::util::{
    arg_bool, arg_opt_i64, arg_opt_text, exec, last_rowid, now_secs, query, query_one,
};
use crate::store::persistence::records::{Route, RouteInput};

const COLS: &str =
    "id, name, strategy, enabled, description, settings_json, created_at, updated_at";

fn decode(row: &Row) -> anyhow::Result<Route> {
    Ok(Route {
        id: col_i64(row, 0)?,
        name: col_str(row, 1)?,
        strategy: col_str(row, 2)?,
        enabled: col_bool(row, 3)?,
        description: col_opt_str(row, 4)?,
        settings_json: col_opt_json(row, 5)?,
        created_at: col_i64(row, 6)?,
        updated_at: col_i64(row, 7)?,
    })
}

pub async fn list(client: &LibsqlClient) -> anyhow::Result<Vec<Route>> {
    query(client, &format!("SELECT {COLS} FROM routes"), &[])
        .await?
        .iter()
        .map(decode)
        .collect()
}

pub async fn get(client: &LibsqlClient, id: i64) -> anyhow::Result<Option<Route>> {
    query_one(
        client,
        &format!("SELECT {COLS} FROM routes WHERE id = ?"),
        &[arg_integer(id)],
    )
    .await?
    .as_ref()
    .map(decode)
    .transpose()
}

pub async fn get_by_name(client: &LibsqlClient, name: &str) -> anyhow::Result<Option<Route>> {
    query_one(
        client,
        &format!("SELECT {COLS} FROM routes WHERE name = ?"),
        &[arg_text(name)],
    )
    .await?
    .as_ref()
    .map(decode)
    .transpose()
}

pub async fn upsert(client: &LibsqlClient, input: RouteInput) -> anyhow::Result<Route> {
    let now = now_secs();
    let settings = input
        .settings_json
        .as_ref()
        .map(serde_json::to_string)
        .transpose()?;

    let id = match input.id {
        Some(id) if get(client, id).await?.is_some() => {
            client
                .execute(
                    "UPDATE routes SET name=?, strategy=?, enabled=?, description=?, settings_json=?, \
                     updated_at=? WHERE id=?",
                    &[
                        arg_text(&input.name),
                        arg_text(&input.strategy),
                        arg_bool(input.enabled),
                        arg_opt_text(input.description.as_deref()),
                        arg_opt_text(settings.as_deref()),
                        arg_integer(now),
                        arg_integer(id),
                    ],
                )
                .await
                .map_err(|e| {
                    crate::store::persistence::libsql::conflict_if_unique(e, || {
                        format!("route name already exists: {}", input.name)
                    })
                })?;
            id
        }
        maybe_id => {
            let qr = client
                .execute(
                    "INSERT INTO routes (id, name, strategy, enabled, description, settings_json, \
                     created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                    &[
                        arg_opt_i64(maybe_id),
                        arg_text(&input.name),
                        arg_text(&input.strategy),
                        arg_bool(input.enabled),
                        arg_opt_text(input.description.as_deref()),
                        arg_opt_text(settings.as_deref()),
                        arg_integer(now),
                        arg_integer(now),
                    ],
                )
                .await
                .map_err(|e| {
                    crate::store::persistence::libsql::conflict_if_unique(e, || {
                        format!("route name already exists: {}", input.name)
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
        .ok_or_else(|| anyhow::anyhow!("route vanished after upsert"))
}

pub async fn delete(client: &LibsqlClient, id: i64) -> anyhow::Result<bool> {
    // cascade: members and aliases of this route.
    super::route_members::delete_by_route(client, id).await?;
    super::aliases::delete_by_route(client, id).await?;
    let n = exec(
        client,
        "DELETE FROM routes WHERE id = ?",
        &[arg_integer(id)],
    )
    .await?;
    Ok(n > 0)
}
