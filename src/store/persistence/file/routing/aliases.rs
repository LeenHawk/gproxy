//! File-backend alias ops over `aliases.json`.

use std::path::{Path, PathBuf};

use crate::store::persistence::records::{Alias, AliasInput};

use crate::store::persistence::file::table::{self, now_secs};

fn path(root: &Path) -> PathBuf {
    root.join("aliases.json")
}

pub(crate) async fn list(root: &Path) -> anyhow::Result<Vec<Alias>> {
    Ok(table::load::<Alias>(&path(root)).await?.rows)
}

pub(crate) async fn get_by_name(root: &Path, alias: &str) -> anyhow::Result<Option<Alias>> {
    Ok(table::load::<Alias>(&path(root))
        .await?
        .rows
        .into_iter()
        .find(|a| a.alias == alias))
}

pub(crate) async fn upsert(root: &Path, input: AliasInput) -> anyhow::Result<Alias> {
    let file = path(root);
    let mut t = table::load::<Alias>(&file).await?;
    let now = now_secs();

    if let Some(existing) = t.rows.iter().find(|a| a.alias == input.alias)
        && Some(existing.id) != input.id
    {
        return Err(crate::store::persistence::ConflictError::new(format!(
            "alias already exists: {}",
            input.alias
        ))
        .into());
    }

    let stored = match input.id {
        Some(id) => {
            if let Some(row) = t.rows.iter_mut().find(|a| a.id == id) {
                row.alias = input.alias;
                row.route_id = input.route_id;
                row.updated_at = now;
                row.clone()
            } else {
                if id >= t.next_id {
                    t.next_id = id + 1;
                }
                let alias = Alias {
                    id,
                    alias: input.alias,
                    route_id: input.route_id,
                    created_at: now,
                    updated_at: now,
                };
                t.rows.push(alias.clone());
                alias
            }
        }
        None => {
            let id = t.next_id;
            t.next_id += 1;
            let alias = Alias {
                id,
                alias: input.alias,
                route_id: input.route_id,
                created_at: now,
                updated_at: now,
            };
            t.rows.push(alias.clone());
            alias
        }
    };

    table::store(&file, &t).await?;
    Ok(stored)
}

pub(crate) async fn delete(root: &Path, id: i64) -> anyhow::Result<bool> {
    let file = path(root);
    let mut t = table::load::<Alias>(&file).await?;
    let before = t.rows.len();
    t.rows.retain(|a| a.id != id);
    let removed = t.rows.len() != before;
    if removed {
        table::store(&file, &t).await?;
    }
    Ok(removed)
}

pub(crate) async fn delete_by_route(root: &Path, route_id: i64) -> anyhow::Result<()> {
    let file = path(root);
    let mut t = table::load::<Alias>(&file).await?;
    let before = t.rows.len();
    t.rows.retain(|a| a.route_id != route_id);
    if t.rows.len() != before {
        table::store(&file, &t).await?;
    }
    Ok(())
}
