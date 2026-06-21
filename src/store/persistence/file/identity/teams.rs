//! File-backend team ops over `teams.json`. Unique per `(org_id, name)`.

use std::path::{Path, PathBuf};

use crate::store::persistence::records::{Scope, Team, TeamInput};

use crate::store::persistence::file::table::{self, now_secs};

fn path(root: &Path) -> PathBuf {
    root.join("teams.json")
}

pub(crate) async fn list(root: &Path, org_id: i64) -> anyhow::Result<Vec<Team>> {
    Ok(table::load::<Team>(&path(root))
        .await?
        .rows
        .into_iter()
        .filter(|t| t.org_id == org_id)
        .collect())
}

pub(crate) async fn get(root: &Path, id: i64) -> anyhow::Result<Option<Team>> {
    Ok(table::load::<Team>(&path(root))
        .await?
        .rows
        .into_iter()
        .find(|t| t.id == id))
}

pub(crate) async fn upsert(root: &Path, input: TeamInput) -> anyhow::Result<Team> {
    let file = path(root);
    let mut t = table::load::<Team>(&file).await?;
    let now = now_secs();

    if let Some(existing) = t
        .rows
        .iter()
        .find(|r| r.org_id == input.org_id && r.name == input.name)
        && Some(existing.id) != input.id
    {
        return Err(crate::store::persistence::ConflictError::new(format!(
            "team name already exists in org {}: {}",
            input.org_id, input.name
        ))
        .into());
    }

    let stored = match input.id {
        Some(id) => {
            if let Some(row) = t.rows.iter_mut().find(|r| r.id == id) {
                row.org_id = input.org_id;
                row.name = input.name;
                row.enabled = input.enabled;
                row.updated_at = now;
                row.clone()
            } else {
                if id >= t.next_id {
                    t.next_id = id + 1;
                }
                let team = Team {
                    id,
                    org_id: input.org_id,
                    name: input.name,
                    enabled: input.enabled,
                    created_at: now,
                    updated_at: now,
                };
                t.rows.push(team.clone());
                team
            }
        }
        None => {
            let id = t.next_id;
            t.next_id += 1;
            let team = Team {
                id,
                org_id: input.org_id,
                name: input.name,
                enabled: input.enabled,
                created_at: now,
                updated_at: now,
            };
            t.rows.push(team.clone());
            team
        }
    };

    table::store(&file, &t).await?;
    Ok(stored)
}

pub(crate) async fn delete(root: &Path, id: i64) -> anyhow::Result<bool> {
    // cascade: detach members and drop scope-bound rows for this team.
    super::users::clear_team(root, id).await?;
    crate::store::persistence::file::authz::route_permissions::delete_by_scope(
        root,
        Scope::Team,
        id,
    )
    .await?;
    crate::store::persistence::file::authz::rate_limits::delete_by_scope(root, Scope::Team, id)
        .await?;
    crate::store::persistence::file::authz::quotas::delete_by_scope(root, Scope::Team, id).await?;

    let file = path(root);
    let mut t = table::load::<Team>(&file).await?;
    let before = t.rows.len();
    t.rows.retain(|r| r.id != id);
    let removed = t.rows.len() != before;
    if removed {
        table::store(&file, &t).await?;
    }
    Ok(removed)
}

pub(crate) async fn delete_by_org(root: &Path, org_id: i64) -> anyhow::Result<()> {
    let ids: Vec<i64> = table::load::<Team>(&path(root))
        .await?
        .rows
        .into_iter()
        .filter(|t| t.org_id == org_id)
        .map(|t| t.id)
        .collect();
    for tid in &ids {
        super::users::clear_team(root, *tid).await?;
        crate::store::persistence::file::authz::route_permissions::delete_by_scope(
            root,
            Scope::Team,
            *tid,
        )
        .await?;
        crate::store::persistence::file::authz::rate_limits::delete_by_scope(
            root,
            Scope::Team,
            *tid,
        )
        .await?;
        crate::store::persistence::file::authz::quotas::delete_by_scope(root, Scope::Team, *tid)
            .await?;
    }

    let file = path(root);
    let mut t = table::load::<Team>(&file).await?;
    let before = t.rows.len();
    t.rows.retain(|r| r.org_id != org_id);
    if t.rows.len() != before {
        table::store(&file, &t).await?;
    }
    Ok(())
}
