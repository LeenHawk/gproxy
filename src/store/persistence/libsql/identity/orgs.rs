//! Org ops for the libSQL edge backend. `name` is unique.

use crate::store::libsql::{LibsqlClient, arg_integer, arg_text};
use crate::store::persistence::libsql::row::{Row, col_bool, col_i64, col_opt_str, col_str};
use crate::store::persistence::libsql::util::{
    arg_bool, arg_opt_i64, arg_opt_text, exec, last_rowid, now_secs, query, query_one,
};
use crate::store::persistence::records::{Org, OrgInput, Scope};

const COLS: &str = "id, name, enabled, description, created_at, updated_at";

fn decode(row: &Row) -> anyhow::Result<Org> {
    Ok(Org {
        id: col_i64(row, 0)?,
        name: col_str(row, 1)?,
        enabled: col_bool(row, 2)?,
        description: col_opt_str(row, 3)?,
        created_at: col_i64(row, 4)?,
        updated_at: col_i64(row, 5)?,
    })
}

pub async fn list(client: &LibsqlClient) -> anyhow::Result<Vec<Org>> {
    query(client, &format!("SELECT {COLS} FROM orgs"), &[])
        .await?
        .iter()
        .map(decode)
        .collect()
}

pub async fn get(client: &LibsqlClient, id: i64) -> anyhow::Result<Option<Org>> {
    query_one(
        client,
        &format!("SELECT {COLS} FROM orgs WHERE id = ?"),
        &[arg_integer(id)],
    )
    .await?
    .as_ref()
    .map(decode)
    .transpose()
}

pub async fn get_by_name(client: &LibsqlClient, name: &str) -> anyhow::Result<Option<Org>> {
    query_one(
        client,
        &format!("SELECT {COLS} FROM orgs WHERE name = ?"),
        &[arg_text(name)],
    )
    .await?
    .as_ref()
    .map(decode)
    .transpose()
}

pub async fn upsert(client: &LibsqlClient, input: OrgInput) -> anyhow::Result<Org> {
    let now = now_secs();

    let id = match input.id {
        Some(id) if get(client, id).await?.is_some() => {
            client
                .execute(
                    "UPDATE orgs SET name=?, enabled=?, description=?, updated_at=? WHERE id=?",
                    &[
                        arg_text(&input.name),
                        arg_bool(input.enabled),
                        arg_opt_text(input.description.as_deref()),
                        arg_integer(now),
                        arg_integer(id),
                    ],
                )
                .await
                .map_err(|e| {
                    crate::store::persistence::libsql::conflict_if_unique(e, || {
                        format!("org name already exists: {}", input.name)
                    })
                })?;
            id
        }
        maybe_id => {
            let qr = client
                .execute(
                    "INSERT INTO orgs (id, name, enabled, description, created_at, updated_at) \
                     VALUES (?, ?, ?, ?, ?, ?)",
                    &[
                        arg_opt_i64(maybe_id),
                        arg_text(&input.name),
                        arg_bool(input.enabled),
                        arg_opt_text(input.description.as_deref()),
                        arg_integer(now),
                        arg_integer(now),
                    ],
                )
                .await
                .map_err(|e| {
                    crate::store::persistence::libsql::conflict_if_unique(e, || {
                        format!("org name already exists: {}", input.name)
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
        .ok_or_else(|| anyhow::anyhow!("org vanished after upsert"))
}

pub async fn delete(client: &LibsqlClient, id: i64) -> anyhow::Result<bool> {
    // cascade: teams, users (which cascade user_keys), and org-scoped authz rows.
    super::teams::delete_by_org(client, id).await?;
    super::users::delete_by_org(client, id).await?;
    crate::store::persistence::libsql::authz::delete_scope_rows(client, Scope::Org, id).await?;

    let n = exec(client, "DELETE FROM orgs WHERE id = ?", &[arg_integer(id)]).await?;
    Ok(n > 0)
}
