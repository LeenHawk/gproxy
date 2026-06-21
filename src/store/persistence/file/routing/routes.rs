//! File-backend route ops over `routes.json`.

use std::path::{Path, PathBuf};

use crate::store::persistence::records::{Route, RouteInput};

use crate::store::persistence::file::table::{self, now_secs};

fn path(root: &Path) -> PathBuf {
    root.join("routes.json")
}

pub(crate) async fn list(root: &Path) -> anyhow::Result<Vec<Route>> {
    Ok(table::load::<Route>(&path(root)).await?.rows)
}

pub(crate) async fn get(root: &Path, id: i64) -> anyhow::Result<Option<Route>> {
    Ok(table::load::<Route>(&path(root))
        .await?
        .rows
        .into_iter()
        .find(|r| r.id == id))
}

pub(crate) async fn get_by_name(root: &Path, name: &str) -> anyhow::Result<Option<Route>> {
    Ok(table::load::<Route>(&path(root))
        .await?
        .rows
        .into_iter()
        .find(|r| r.name == name))
}

pub(crate) async fn upsert(root: &Path, input: RouteInput) -> anyhow::Result<Route> {
    let file = path(root);
    let mut t = table::load::<Route>(&file).await?;
    let now = now_secs();

    if let Some(existing) = t.rows.iter().find(|r| r.name == input.name)
        && Some(existing.id) != input.id
    {
        return Err(crate::store::persistence::ConflictError::new(format!(
            "route name already exists: {}",
            input.name
        ))
        .into());
    }

    let stored = match input.id {
        Some(id) => {
            if let Some(row) = t.rows.iter_mut().find(|r| r.id == id) {
                row.name = input.name;
                row.strategy = input.strategy;
                row.enabled = input.enabled;
                row.description = input.description;
                row.settings_json = input.settings_json;
                row.updated_at = now;
                row.clone()
            } else {
                if id >= t.next_id {
                    t.next_id = id + 1;
                }
                let route = Route {
                    id,
                    name: input.name,
                    strategy: input.strategy,
                    enabled: input.enabled,
                    description: input.description,
                    settings_json: input.settings_json,
                    created_at: now,
                    updated_at: now,
                };
                t.rows.push(route.clone());
                route
            }
        }
        None => {
            let id = t.next_id;
            t.next_id += 1;
            let route = Route {
                id,
                name: input.name,
                strategy: input.strategy,
                enabled: input.enabled,
                description: input.description,
                settings_json: input.settings_json,
                created_at: now,
                updated_at: now,
            };
            t.rows.push(route.clone());
            route
        }
    };

    table::store(&file, &t).await?;
    Ok(stored)
}

pub(crate) async fn delete(root: &Path, id: i64) -> anyhow::Result<bool> {
    // cascade: members and aliases of this route.
    super::route_members::delete_by_route(root, id).await?;
    super::aliases::delete_by_route(root, id).await?;

    let file = path(root);
    let mut t = table::load::<Route>(&file).await?;
    let before = t.rows.len();
    t.rows.retain(|r| r.id != id);
    let removed = t.rows.len() != before;
    if removed {
        table::store(&file, &t).await?;
    }
    Ok(removed)
}
