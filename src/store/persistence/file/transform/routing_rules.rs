//! File-backend routing-rule ops over `routing_rules.json`.

use std::path::{Path, PathBuf};

use crate::store::persistence::records::{RoutingRule, RoutingRuleInput};

use crate::store::persistence::file::table::{self, now_secs};

fn path(root: &Path) -> PathBuf {
    root.join("routing_rules.json")
}

pub(crate) async fn list(root: &Path, provider_id: i64) -> anyhow::Result<Vec<RoutingRule>> {
    Ok(table::load::<RoutingRule>(&path(root))
        .await?
        .rows
        .into_iter()
        .filter(|r| r.provider_id == provider_id)
        .collect())
}

pub(crate) async fn get(root: &Path, id: i64) -> anyhow::Result<Option<RoutingRule>> {
    Ok(table::load::<RoutingRule>(&path(root))
        .await?
        .rows
        .into_iter()
        .find(|r| r.id == id))
}

pub(crate) async fn upsert(root: &Path, input: RoutingRuleInput) -> anyhow::Result<RoutingRule> {
    let file = path(root);
    let mut t = table::load::<RoutingRule>(&file).await?;
    let now = now_secs();

    if let Some(existing) = t.rows.iter().find(|r| {
        r.provider_id == input.provider_id && r.operation == input.operation && r.kind == input.kind
    }) && Some(existing.id) != input.id
    {
        return Err(crate::store::persistence::ConflictError::new(format!(
            "routing rule already exists for provider {} ({}, {})",
            input.provider_id, input.operation, input.kind
        ))
        .into());
    }

    let stored = match input.id {
        Some(id) => {
            if let Some(row) = t.rows.iter_mut().find(|r| r.id == id) {
                row.provider_id = input.provider_id;
                row.operation = input.operation;
                row.kind = input.kind;
                row.implementation = input.implementation;
                row.dest_operation = input.dest_operation;
                row.dest_kind = input.dest_kind;
                row.sort_order = input.sort_order;
                row.enabled = input.enabled;
                row.updated_at = now;
                row.clone()
            } else {
                if id >= t.next_id {
                    t.next_id = id + 1;
                }
                let rule = RoutingRule {
                    id,
                    provider_id: input.provider_id,
                    operation: input.operation,
                    kind: input.kind,
                    implementation: input.implementation,
                    dest_operation: input.dest_operation,
                    dest_kind: input.dest_kind,
                    sort_order: input.sort_order,
                    enabled: input.enabled,
                    created_at: now,
                    updated_at: now,
                };
                t.rows.push(rule.clone());
                rule
            }
        }
        None => {
            let id = t.next_id;
            t.next_id += 1;
            let rule = RoutingRule {
                id,
                provider_id: input.provider_id,
                operation: input.operation,
                kind: input.kind,
                implementation: input.implementation,
                dest_operation: input.dest_operation,
                dest_kind: input.dest_kind,
                sort_order: input.sort_order,
                enabled: input.enabled,
                created_at: now,
                updated_at: now,
            };
            t.rows.push(rule.clone());
            rule
        }
    };

    table::store(&file, &t).await?;
    Ok(stored)
}

pub(crate) async fn delete(root: &Path, id: i64) -> anyhow::Result<bool> {
    let file = path(root);
    let mut t = table::load::<RoutingRule>(&file).await?;
    let before = t.rows.len();
    t.rows.retain(|r| r.id != id);
    let removed = t.rows.len() != before;
    if removed {
        table::store(&file, &t).await?;
    }
    Ok(removed)
}

pub(crate) async fn delete_by_provider(root: &Path, provider_id: i64) -> anyhow::Result<()> {
    let file = path(root);
    let mut t = table::load::<RoutingRule>(&file).await?;
    let before = t.rows.len();
    t.rows.retain(|r| r.provider_id != provider_id);
    if t.rows.len() != before {
        table::store(&file, &t).await?;
    }
    Ok(())
}
