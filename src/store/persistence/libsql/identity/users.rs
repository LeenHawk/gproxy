//! User ops for the libSQL edge backend. `name` is unique.

use crate::store::libsql::{LibsqlClient, arg_integer, arg_text};
use crate::store::persistence::libsql::row::{
    Row, col_bool, col_i64, col_opt_i64, col_opt_str, col_str,
};
use crate::store::persistence::libsql::util::{
    arg_bool, arg_opt_i64, arg_opt_text, exec, last_rowid, now_secs, query, query_one,
};
use crate::store::persistence::records::{Scope, User, UserInput};

const COLS: &str = "id, name, org_id, team_id, password, enabled, is_admin, created_at, updated_at";

fn decode(row: &Row) -> anyhow::Result<User> {
    Ok(User {
        id: col_i64(row, 0)?,
        name: col_str(row, 1)?,
        org_id: col_i64(row, 2)?,
        team_id: col_opt_i64(row, 3)?,
        password: col_opt_str(row, 4)?,
        enabled: col_bool(row, 5)?,
        is_admin: col_bool(row, 6)?,
        created_at: col_i64(row, 7)?,
        updated_at: col_i64(row, 8)?,
    })
}

pub async fn list(client: &LibsqlClient) -> anyhow::Result<Vec<User>> {
    query(client, &format!("SELECT {COLS} FROM users"), &[])
        .await?
        .iter()
        .map(decode)
        .collect()
}

pub async fn get(client: &LibsqlClient, id: i64) -> anyhow::Result<Option<User>> {
    query_one(
        client,
        &format!("SELECT {COLS} FROM users WHERE id = ?"),
        &[arg_integer(id)],
    )
    .await?
    .as_ref()
    .map(decode)
    .transpose()
}

pub async fn get_by_name(client: &LibsqlClient, name: &str) -> anyhow::Result<Option<User>> {
    query_one(
        client,
        &format!("SELECT {COLS} FROM users WHERE name = ?"),
        &[arg_text(name)],
    )
    .await?
    .as_ref()
    .map(decode)
    .transpose()
}

pub async fn upsert(client: &LibsqlClient, input: UserInput) -> anyhow::Result<User> {
    let now = now_secs();

    let id = match input.id {
        Some(id) if get(client, id).await?.is_some() => {
            client
                .execute(
                    "UPDATE users SET name=?, org_id=?, team_id=?, password=?, enabled=?, is_admin=?, \
                     updated_at=? WHERE id=?",
                    &[
                        arg_text(&input.name),
                        arg_integer(input.org_id),
                        arg_opt_i64(input.team_id),
                        arg_opt_text(input.password.as_deref()),
                        arg_bool(input.enabled),
                        arg_bool(input.is_admin),
                        arg_integer(now),
                        arg_integer(id),
                    ],
                )
                .await
                .map_err(|e| {
                    crate::store::persistence::libsql::conflict_if_unique(e, || {
                        format!("user name already exists: {}", input.name)
                    })
                })?;
            id
        }
        maybe_id => {
            let qr = client
                .execute(
                    "INSERT INTO users \
                     (id, name, org_id, team_id, password, enabled, is_admin, created_at, \
                      updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    &[
                        arg_opt_i64(maybe_id),
                        arg_text(&input.name),
                        arg_integer(input.org_id),
                        arg_opt_i64(input.team_id),
                        arg_opt_text(input.password.as_deref()),
                        arg_bool(input.enabled),
                        arg_bool(input.is_admin),
                        arg_integer(now),
                        arg_integer(now),
                    ],
                )
                .await
                .map_err(|e| {
                    crate::store::persistence::libsql::conflict_if_unique(e, || {
                        format!("user name already exists: {}", input.name)
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
        .ok_or_else(|| anyhow::anyhow!("user vanished after upsert"))
}

pub async fn delete(client: &LibsqlClient, id: i64) -> anyhow::Result<bool> {
    // cascade: keys and user-scoped authz rows.
    super::user_keys::delete_by_user(client, id).await?;
    crate::store::persistence::libsql::authz::delete_scope_rows(client, Scope::User, id).await?;

    let n = exec(client, "DELETE FROM users WHERE id = ?", &[arg_integer(id)]).await?;
    Ok(n > 0)
}

pub async fn delete_by_org(client: &LibsqlClient, org_id: i64) -> anyhow::Result<()> {
    let rows = query(
        client,
        "SELECT id FROM users WHERE org_id = ?",
        &[arg_integer(org_id)],
    )
    .await?;
    for r in &rows {
        let uid = col_i64(r, 0)?;
        super::user_keys::delete_by_user(client, uid).await?;
        crate::store::persistence::libsql::authz::delete_scope_rows(client, Scope::User, uid)
            .await?;
    }
    exec(
        client,
        "DELETE FROM users WHERE org_id = ?",
        &[arg_integer(org_id)],
    )
    .await?;
    Ok(())
}

pub async fn clear_team(client: &LibsqlClient, team_id: i64) -> anyhow::Result<()> {
    let now = now_secs();
    exec(
        client,
        "UPDATE users SET team_id = NULL, updated_at = ? WHERE team_id = ?",
        &[arg_integer(now), arg_integer(team_id)],
    )
    .await?;
    Ok(())
}
