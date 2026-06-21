//! File-backend rate-limit ops over `rate_limits.json`.

use std::path::{Path, PathBuf};

use crate::store::persistence::records::{RateLimit, RateLimitInput, Scope};

use crate::store::persistence::file::table::{self, now_secs};

fn path(root: &Path) -> PathBuf {
    root.join("rate_limits.json")
}

pub(crate) async fn list(
    root: &Path,
    scope: Scope,
    scope_id: i64,
) -> anyhow::Result<Vec<RateLimit>> {
    Ok(table::load::<RateLimit>(&path(root))
        .await?
        .rows
        .into_iter()
        .filter(|r| r.scope == scope && r.scope_id == scope_id)
        .collect())
}

pub(crate) async fn upsert(root: &Path, input: RateLimitInput) -> anyhow::Result<RateLimit> {
    let file = path(root);
    let mut t = table::load::<RateLimit>(&file).await?;
    let now = now_secs();

    let stored = match input.id {
        Some(id) => {
            if let Some(row) = t.rows.iter_mut().find(|r| r.id == id) {
                row.scope = input.scope;
                row.scope_id = input.scope_id;
                row.route_pattern = input.route_pattern;
                row.rpm = input.rpm;
                row.rpd = input.rpd;
                row.total_tokens = input.total_tokens;
                row.updated_at = now;
                row.clone()
            } else {
                // Insert with the pinned id (bundle import contract — same
                // semantics as the identity tables).
                if id >= t.next_id {
                    t.next_id = id + 1;
                }
                let limit = RateLimit {
                    id,
                    scope: input.scope,
                    scope_id: input.scope_id,
                    route_pattern: input.route_pattern,
                    rpm: input.rpm,
                    rpd: input.rpd,
                    total_tokens: input.total_tokens,
                    created_at: now,
                    updated_at: now,
                };
                t.rows.push(limit.clone());
                limit
            }
        }
        None => {
            let id = t.next_id;
            t.next_id += 1;
            let limit = RateLimit {
                id,
                scope: input.scope,
                scope_id: input.scope_id,
                route_pattern: input.route_pattern,
                rpm: input.rpm,
                rpd: input.rpd,
                total_tokens: input.total_tokens,
                created_at: now,
                updated_at: now,
            };
            t.rows.push(limit.clone());
            limit
        }
    };

    table::store(&file, &t).await?;
    Ok(stored)
}

pub(crate) async fn delete(root: &Path, id: i64) -> anyhow::Result<bool> {
    let file = path(root);
    let mut t = table::load::<RateLimit>(&file).await?;
    let before = t.rows.len();
    t.rows.retain(|r| r.id != id);
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
    let mut t = table::load::<RateLimit>(&file).await?;
    let before = t.rows.len();
    t.rows
        .retain(|r| !(r.scope == scope && r.scope_id == scope_id));
    if t.rows.len() != before {
        table::store(&file, &t).await?;
    }
    Ok(())
}
