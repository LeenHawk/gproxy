//! File-backend route-permission ops over `route_permissions.json`.

use std::path::{Path, PathBuf};

use crate::store::persistence::records::{RoutePermission, RoutePermissionInput, Scope};

use crate::store::persistence::file::table::{self, now_secs};

fn path(root: &Path) -> PathBuf {
    root.join("route_permissions.json")
}

pub(crate) async fn list(
    root: &Path,
    scope: Scope,
    scope_id: i64,
) -> anyhow::Result<Vec<RoutePermission>> {
    Ok(table::load::<RoutePermission>(&path(root))
        .await?
        .rows
        .into_iter()
        .filter(|p| p.scope == scope && p.scope_id == scope_id)
        .collect())
}

pub(crate) async fn upsert(
    root: &Path,
    input: RoutePermissionInput,
) -> anyhow::Result<RoutePermission> {
    let file = path(root);
    let mut t = table::load::<RoutePermission>(&file).await?;
    let now = now_secs();

    let stored = match input.id {
        Some(id) => {
            if let Some(row) = t.rows.iter_mut().find(|p| p.id == id) {
                row.scope = input.scope;
                row.scope_id = input.scope_id;
                row.route_pattern = input.route_pattern;
                row.updated_at = now;
                row.clone()
            } else {
                // Insert with the pinned id (bundle import contract — same
                // semantics as the identity tables).
                if id >= t.next_id {
                    t.next_id = id + 1;
                }
                let perm = RoutePermission {
                    id,
                    scope: input.scope,
                    scope_id: input.scope_id,
                    route_pattern: input.route_pattern,
                    created_at: now,
                    updated_at: now,
                };
                t.rows.push(perm.clone());
                perm
            }
        }
        None => {
            let id = t.next_id;
            t.next_id += 1;
            let perm = RoutePermission {
                id,
                scope: input.scope,
                scope_id: input.scope_id,
                route_pattern: input.route_pattern,
                created_at: now,
                updated_at: now,
            };
            t.rows.push(perm.clone());
            perm
        }
    };

    table::store(&file, &t).await?;
    Ok(stored)
}

pub(crate) async fn delete(root: &Path, id: i64) -> anyhow::Result<bool> {
    let file = path(root);
    let mut t = table::load::<RoutePermission>(&file).await?;
    let before = t.rows.len();
    t.rows.retain(|p| p.id != id);
    let removed = t.rows.len() != before;
    if removed {
        table::store(&file, &t).await?;
    }
    Ok(removed)
}

pub(crate) async fn delete_by_scope(
    root: &Path,
    scope: Scope,
    scope_id: i64,
) -> anyhow::Result<()> {
    let file = path(root);
    let mut t = table::load::<RoutePermission>(&file).await?;
    let before = t.rows.len();
    t.rows
        .retain(|p| !(p.scope == scope && p.scope_id == scope_id));
    if t.rows.len() != before {
        table::store(&file, &t).await?;
    }
    Ok(())
}
