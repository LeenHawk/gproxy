//! File-backend route-member ops over `route_members.json`.

use std::path::{Path, PathBuf};

use crate::store::persistence::records::{RouteMember, RouteMemberInput};

use crate::store::persistence::file::table::{self, now_secs};

fn path(root: &Path) -> PathBuf {
    root.join("route_members.json")
}

pub(crate) async fn list(root: &Path, route_id: i64) -> anyhow::Result<Vec<RouteMember>> {
    Ok(table::load::<RouteMember>(&path(root))
        .await?
        .rows
        .into_iter()
        .filter(|m| m.route_id == route_id)
        .collect())
}

pub(crate) async fn upsert(root: &Path, input: RouteMemberInput) -> anyhow::Result<RouteMember> {
    let file = path(root);
    let mut t = table::load::<RouteMember>(&file).await?;
    let now = now_secs();

    let stored = match input.id {
        Some(id) => {
            if let Some(row) = t.rows.iter_mut().find(|m| m.id == id) {
                row.route_id = input.route_id;
                row.provider_id = input.provider_id;
                row.upstream_model_id = input.upstream_model_id;
                row.weight = input.weight;
                row.tier = input.tier;
                row.enabled = input.enabled;
                row.updated_at = now;
                row.clone()
            } else {
                if id >= t.next_id {
                    t.next_id = id + 1;
                }
                let member = RouteMember {
                    id,
                    route_id: input.route_id,
                    provider_id: input.provider_id,
                    upstream_model_id: input.upstream_model_id,
                    weight: input.weight,
                    tier: input.tier,
                    enabled: input.enabled,
                    created_at: now,
                    updated_at: now,
                };
                t.rows.push(member.clone());
                member
            }
        }
        None => {
            let id = t.next_id;
            t.next_id += 1;
            let member = RouteMember {
                id,
                route_id: input.route_id,
                provider_id: input.provider_id,
                upstream_model_id: input.upstream_model_id,
                weight: input.weight,
                tier: input.tier,
                enabled: input.enabled,
                created_at: now,
                updated_at: now,
            };
            t.rows.push(member.clone());
            member
        }
    };

    table::store(&file, &t).await?;
    Ok(stored)
}

pub(crate) async fn delete(root: &Path, id: i64) -> anyhow::Result<bool> {
    let file = path(root);
    let mut t = table::load::<RouteMember>(&file).await?;
    let before = t.rows.len();
    t.rows.retain(|m| m.id != id);
    let removed = t.rows.len() != before;
    if removed {
        table::store(&file, &t).await?;
    }
    Ok(removed)
}

pub(crate) async fn delete_by_route(root: &Path, route_id: i64) -> anyhow::Result<()> {
    let file = path(root);
    let mut t = table::load::<RouteMember>(&file).await?;
    let before = t.rows.len();
    t.rows.retain(|m| m.route_id != route_id);
    if t.rows.len() != before {
        table::store(&file, &t).await?;
    }
    Ok(())
}
