//! File-backend org ops over `orgs.json`. `name` is unique.

use std::path::{Path, PathBuf};

use crate::store::persistence::records::{Org, OrgInput, Scope};

use crate::store::persistence::file::table::{self, now_secs};

fn path(root: &Path) -> PathBuf {
    root.join("orgs.json")
}

pub(crate) async fn list(root: &Path) -> anyhow::Result<Vec<Org>> {
    Ok(table::load::<Org>(&path(root)).await?.rows)
}

pub(crate) async fn get(root: &Path, id: i64) -> anyhow::Result<Option<Org>> {
    Ok(table::load::<Org>(&path(root))
        .await?
        .rows
        .into_iter()
        .find(|o| o.id == id))
}

pub(crate) async fn get_by_name(root: &Path, name: &str) -> anyhow::Result<Option<Org>> {
    Ok(table::load::<Org>(&path(root))
        .await?
        .rows
        .into_iter()
        .find(|o| o.name == name))
}

pub(crate) async fn upsert(root: &Path, input: OrgInput) -> anyhow::Result<Org> {
    let file = path(root);
    let mut t = table::load::<Org>(&file).await?;
    let now = now_secs();

    if let Some(existing) = t.rows.iter().find(|o| o.name == input.name)
        && Some(existing.id) != input.id
    {
        return Err(crate::store::persistence::ConflictError::new(format!(
            "org name already exists: {}",
            input.name
        ))
        .into());
    }

    let stored = match input.id {
        Some(id) => {
            if let Some(row) = t.rows.iter_mut().find(|o| o.id == id) {
                row.name = input.name;
                row.enabled = input.enabled;
                row.description = input.description;
                row.updated_at = now;
                row.clone()
            } else {
                if id >= t.next_id {
                    t.next_id = id + 1;
                }
                let org = Org {
                    id,
                    name: input.name,
                    enabled: input.enabled,
                    description: input.description,
                    created_at: now,
                    updated_at: now,
                };
                t.rows.push(org.clone());
                org
            }
        }
        None => {
            let id = t.next_id;
            t.next_id += 1;
            let org = Org {
                id,
                name: input.name,
                enabled: input.enabled,
                description: input.description,
                created_at: now,
                updated_at: now,
            };
            t.rows.push(org.clone());
            org
        }
    };

    table::store(&file, &t).await?;
    Ok(stored)
}

pub(crate) async fn delete(root: &Path, id: i64) -> anyhow::Result<bool> {
    // cascade: teams, users (which cascade user_keys), and scope-bound rows.
    super::teams::delete_by_org(root, id).await?;
    super::users::delete_by_org(root, id).await?;
    crate::store::persistence::file::authz::route_permissions::delete_by_scope(
        root,
        Scope::Org,
        id,
    )
    .await?;
    crate::store::persistence::file::authz::rate_limits::delete_by_scope(root, Scope::Org, id)
        .await?;
    crate::store::persistence::file::authz::quotas::delete_by_scope(root, Scope::Org, id).await?;

    let file = path(root);
    let mut t = table::load::<Org>(&file).await?;
    let before = t.rows.len();
    t.rows.retain(|o| o.id != id);
    let removed = t.rows.len() != before;
    if removed {
        table::store(&file, &t).await?;
    }
    Ok(removed)
}
