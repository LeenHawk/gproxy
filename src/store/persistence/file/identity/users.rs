//! File-backend user ops over `users.json`. `name` is unique.

use std::path::{Path, PathBuf};

use crate::store::persistence::records::{Scope, User, UserInput};

use crate::store::persistence::file::table::{self, now_secs};

fn path(root: &Path) -> PathBuf {
    root.join("users.json")
}

pub(crate) async fn list(root: &Path) -> anyhow::Result<Vec<User>> {
    Ok(table::load::<User>(&path(root)).await?.rows)
}

pub(crate) async fn get(root: &Path, id: i64) -> anyhow::Result<Option<User>> {
    Ok(table::load::<User>(&path(root))
        .await?
        .rows
        .into_iter()
        .find(|u| u.id == id))
}

pub(crate) async fn get_by_name(root: &Path, name: &str) -> anyhow::Result<Option<User>> {
    Ok(table::load::<User>(&path(root))
        .await?
        .rows
        .into_iter()
        .find(|u| u.name == name))
}

pub(crate) async fn upsert(root: &Path, input: UserInput) -> anyhow::Result<User> {
    let file = path(root);
    let mut t = table::load::<User>(&file).await?;
    let now = now_secs();

    if let Some(existing) = t.rows.iter().find(|u| u.name == input.name)
        && Some(existing.id) != input.id
    {
        return Err(crate::store::persistence::ConflictError::new(format!(
            "user name already exists: {}",
            input.name
        ))
        .into());
    }

    let stored = match input.id {
        Some(id) => {
            if let Some(row) = t.rows.iter_mut().find(|u| u.id == id) {
                row.name = input.name;
                row.org_id = input.org_id;
                row.team_id = input.team_id;
                row.password = input.password;
                row.enabled = input.enabled;
                row.is_admin = input.is_admin;
                row.updated_at = now;
                row.clone()
            } else {
                if id >= t.next_id {
                    t.next_id = id + 1;
                }
                let user = User {
                    id,
                    name: input.name,
                    org_id: input.org_id,
                    team_id: input.team_id,
                    password: input.password,
                    enabled: input.enabled,
                    is_admin: input.is_admin,
                    created_at: now,
                    updated_at: now,
                };
                t.rows.push(user.clone());
                user
            }
        }
        None => {
            let id = t.next_id;
            t.next_id += 1;
            let user = User {
                id,
                name: input.name,
                org_id: input.org_id,
                team_id: input.team_id,
                password: input.password,
                enabled: input.enabled,
                is_admin: input.is_admin,
                created_at: now,
                updated_at: now,
            };
            t.rows.push(user.clone());
            user
        }
    };

    table::store(&file, &t).await?;
    Ok(stored)
}

pub(crate) async fn delete(root: &Path, id: i64) -> anyhow::Result<bool> {
    // cascade: keys and scope-bound permissions / rate limits / quotas.
    super::user_keys::delete_by_user(root, id).await?;
    crate::store::persistence::file::authz::route_permissions::delete_by_scope(
        root,
        Scope::User,
        id,
    )
    .await?;
    crate::store::persistence::file::authz::rate_limits::delete_by_scope(root, Scope::User, id)
        .await?;
    crate::store::persistence::file::authz::quotas::delete_by_scope(root, Scope::User, id).await?;

    let file = path(root);
    let mut t = table::load::<User>(&file).await?;
    let before = t.rows.len();
    t.rows.retain(|u| u.id != id);
    let removed = t.rows.len() != before;
    if removed {
        table::store(&file, &t).await?;
    }
    Ok(removed)
}

pub(crate) async fn delete_by_org(root: &Path, org_id: i64) -> anyhow::Result<()> {
    // cascade each removed user's keys + scope rows.
    let ids: Vec<i64> = table::load::<User>(&path(root))
        .await?
        .rows
        .into_iter()
        .filter(|u| u.org_id == org_id)
        .map(|u| u.id)
        .collect();
    for uid in &ids {
        super::user_keys::delete_by_user(root, *uid).await?;
        crate::store::persistence::file::authz::route_permissions::delete_by_scope(
            root,
            Scope::User,
            *uid,
        )
        .await?;
        crate::store::persistence::file::authz::rate_limits::delete_by_scope(
            root,
            Scope::User,
            *uid,
        )
        .await?;
        crate::store::persistence::file::authz::quotas::delete_by_scope(root, Scope::User, *uid)
            .await?;
    }

    let file = path(root);
    let mut t = table::load::<User>(&file).await?;
    let before = t.rows.len();
    t.rows.retain(|u| u.org_id != org_id);
    if t.rows.len() != before {
        table::store(&file, &t).await?;
    }
    Ok(())
}

pub(crate) async fn clear_team(root: &Path, team_id: i64) -> anyhow::Result<()> {
    let file = path(root);
    let mut t = table::load::<User>(&file).await?;
    let mut changed = false;
    for u in t.rows.iter_mut() {
        if u.team_id == Some(team_id) {
            u.team_id = None;
            changed = true;
        }
    }
    if changed {
        table::store(&file, &t).await?;
    }
    Ok(())
}
