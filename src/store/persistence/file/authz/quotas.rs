//! File-backend quota ops over `quotas.json`. Unique per `(scope, scope_id)`.

use std::path::{Path, PathBuf};

use crate::store::persistence::records::{Quota, QuotaInput, Scope};

use crate::store::persistence::file::table::{self, now_secs};

fn path(root: &Path) -> PathBuf {
    root.join("quotas.json")
}

pub(crate) async fn get(root: &Path, scope: Scope, scope_id: i64) -> anyhow::Result<Option<Quota>> {
    Ok(table::load::<Quota>(&path(root))
        .await?
        .rows
        .into_iter()
        .find(|q| q.scope == scope && q.scope_id == scope_id))
}

pub(crate) async fn upsert(root: &Path, input: QuotaInput) -> anyhow::Result<Quota> {
    let file = path(root);
    let mut t = table::load::<Quota>(&file).await?;
    let now = now_secs();

    if let Some(existing) = t
        .rows
        .iter()
        .find(|q| q.scope == input.scope && q.scope_id == input.scope_id)
        && Some(existing.id) != input.id
    {
        return Err(crate::store::persistence::ConflictError::new(format!(
            "quota already exists for scope {}:{}",
            input.scope.as_str(),
            input.scope_id
        ))
        .into());
    }

    let stored = match input.id {
        Some(id) => {
            if let Some(row) = t.rows.iter_mut().find(|q| q.id == id) {
                row.scope = input.scope;
                row.scope_id = input.scope_id;
                row.quota_total = input.quota_total;
                // cost_used is billing-owned (accumulated via add_cost); an admin
                // edit of an EXISTING quota must NOT clobber it. Keep row.cost_used.
                // (The insert-with-id branch below still honors input for §18 import.)
                row.updated_at = now;
                row.clone()
            } else {
                // Insert with the pinned id (bundle import contract — same
                // semantics as the identity tables).
                if id >= t.next_id {
                    t.next_id = id + 1;
                }
                let quota = Quota {
                    id,
                    scope: input.scope,
                    scope_id: input.scope_id,
                    quota_total: input.quota_total,
                    cost_used: input.cost_used,
                    created_at: now,
                    updated_at: now,
                };
                t.rows.push(quota.clone());
                quota
            }
        }
        None => {
            let id = t.next_id;
            t.next_id += 1;
            let quota = Quota {
                id,
                scope: input.scope,
                scope_id: input.scope_id,
                quota_total: input.quota_total,
                cost_used: input.cost_used,
                created_at: now,
                updated_at: now,
            };
            t.rows.push(quota.clone());
            quota
        }
    };

    table::store(&file, &t).await?;
    Ok(stored)
}

pub(crate) async fn delete(root: &Path, id: i64) -> anyhow::Result<bool> {
    let file = path(root);
    let mut t = table::load::<Quota>(&file).await?;
    let before = t.rows.len();
    t.rows.retain(|q| q.id != id);
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
    let mut t = table::load::<Quota>(&file).await?;
    let before = t.rows.len();
    t.rows
        .retain(|q| !(q.scope == scope && q.scope_id == scope_id));
    if t.rows.len() != before {
        table::store(&file, &t).await?;
    }
    Ok(())
}

/// Atomically add `delta` to the `cost_used` of the `(scope, scope_id)` row.
/// No-op when the row is absent. The caller holds the backend write lock, so
/// this load → mutate → store is atomic for the single-process file backend.
pub(crate) async fn add_cost(
    root: &Path,
    scope: Scope,
    scope_id: i64,
    delta: rust_decimal::Decimal,
) -> anyhow::Result<()> {
    let file = path(root);
    let mut t = table::load::<Quota>(&file).await?;
    if let Some(row) = t
        .rows
        .iter_mut()
        .find(|q| q.scope == scope && q.scope_id == scope_id)
    {
        row.cost_used += delta;
        row.updated_at = now_secs();
        table::store(&file, &t).await?;
    }
    Ok(())
}
