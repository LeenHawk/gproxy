//! Team ops for the libSQL edge backend. Unique per `(org_id, name)`.

use crate::store::libsql::{LibsqlClient, arg_integer, arg_text};
use crate::store::persistence::libsql::row::{Row, col_bool, col_i64, col_str};
use crate::store::persistence::libsql::util::{
    arg_bool, arg_opt_i64, exec, last_rowid, now_secs, query, query_one,
};
use crate::store::persistence::records::{Scope, Team, TeamInput};

const COLS: &str = "id, org_id, name, enabled, created_at, updated_at";

fn decode(row: &Row) -> anyhow::Result<Team> {
    Ok(Team {
        id: col_i64(row, 0)?,
        org_id: col_i64(row, 1)?,
        name: col_str(row, 2)?,
        enabled: col_bool(row, 3)?,
        created_at: col_i64(row, 4)?,
        updated_at: col_i64(row, 5)?,
    })
}

pub async fn get(client: &LibsqlClient, id: i64) -> anyhow::Result<Option<Team>> {
    query_one(
        client,
        &format!("SELECT {COLS} FROM teams WHERE id = ?"),
        &[arg_integer(id)],
    )
    .await?
    .as_ref()
    .map(decode)
    .transpose()
}

pub async fn list(client: &LibsqlClient, org_id: i64) -> anyhow::Result<Vec<Team>> {
    query(
        client,
        &format!("SELECT {COLS} FROM teams WHERE org_id = ?"),
        &[arg_integer(org_id)],
    )
    .await?
    .iter()
    .map(decode)
    .collect()
}

pub async fn upsert(client: &LibsqlClient, input: TeamInput) -> anyhow::Result<Team> {
    let now = now_secs();

    // Enforce uniqueness on (org_id, name).
    if let Some(row) = query_one(
        client,
        "SELECT id FROM teams WHERE org_id = ? AND name = ?",
        &[arg_integer(input.org_id), arg_text(&input.name)],
    )
    .await?
    {
        let existing = col_i64(&row, 0)?;
        if Some(existing) != input.id {
            return Err(crate::store::persistence::ConflictError::new(format!(
                "team name already exists in org {}: {}",
                input.org_id, input.name
            ))
            .into());
        }
    }

    let id = match input.id {
        Some(id) if get(client, id).await?.is_some() => {
            client
                .execute(
                    "UPDATE teams SET org_id=?, name=?, enabled=?, updated_at=? WHERE id=?",
                    &[
                        arg_integer(input.org_id),
                        arg_text(&input.name),
                        arg_bool(input.enabled),
                        arg_integer(now),
                        arg_integer(id),
                    ],
                )
                .await
                .map_err(|e| {
                    crate::store::persistence::libsql::conflict_if_unique(e, || {
                        format!(
                            "team name already exists in org {}: {}",
                            input.org_id, input.name
                        )
                    })
                })?;
            id
        }
        maybe_id => {
            let qr = client
                .execute(
                    "INSERT INTO teams (id, org_id, name, enabled, created_at, updated_at) \
                     VALUES (?, ?, ?, ?, ?, ?)",
                    &[
                        arg_opt_i64(maybe_id),
                        arg_integer(input.org_id),
                        arg_text(&input.name),
                        arg_bool(input.enabled),
                        arg_integer(now),
                        arg_integer(now),
                    ],
                )
                .await
                .map_err(|e| {
                    crate::store::persistence::libsql::conflict_if_unique(e, || {
                        format!(
                            "team name already exists in org {}: {}",
                            input.org_id, input.name
                        )
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
        .ok_or_else(|| anyhow::anyhow!("team vanished after upsert"))
}

pub async fn delete(client: &LibsqlClient, id: i64) -> anyhow::Result<bool> {
    // cascade: detach members and drop team-scoped authz rows.
    super::users::clear_team(client, id).await?;
    crate::store::persistence::libsql::authz::delete_scope_rows(client, Scope::Team, id).await?;

    let n = exec(client, "DELETE FROM teams WHERE id = ?", &[arg_integer(id)]).await?;
    Ok(n > 0)
}

pub async fn delete_by_org(client: &LibsqlClient, org_id: i64) -> anyhow::Result<()> {
    let teams = list(client, org_id).await?;
    for t in teams {
        super::users::clear_team(client, t.id).await?;
        crate::store::persistence::libsql::authz::delete_scope_rows(client, Scope::Team, t.id)
            .await?;
    }
    exec(
        client,
        "DELETE FROM teams WHERE org_id = ?",
        &[arg_integer(org_id)],
    )
    .await?;
    Ok(())
}
