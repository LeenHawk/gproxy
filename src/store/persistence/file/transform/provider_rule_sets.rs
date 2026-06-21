//! File-backend provider ↔ rule-set attachment ops over
//! `provider_rule_sets.json`.

use std::path::{Path, PathBuf};

use crate::store::persistence::records::{ProviderRuleSet, ProviderRuleSetInput};

use crate::store::persistence::file::table::{self, now_secs};

fn path(root: &Path) -> PathBuf {
    root.join("provider_rule_sets.json")
}

pub(crate) async fn list(root: &Path, provider_id: i64) -> anyhow::Result<Vec<ProviderRuleSet>> {
    Ok(table::load::<ProviderRuleSet>(&path(root))
        .await?
        .rows
        .into_iter()
        .filter(|r| r.provider_id == provider_id)
        .collect())
}

pub(crate) async fn upsert(
    root: &Path,
    input: ProviderRuleSetInput,
) -> anyhow::Result<ProviderRuleSet> {
    let file = path(root);
    let mut t = table::load::<ProviderRuleSet>(&file).await?;
    let now = now_secs();

    let stored = match input.id {
        Some(id) => {
            if let Some(row) = t.rows.iter_mut().find(|r| r.id == id) {
                row.provider_id = input.provider_id;
                row.rule_set_id = input.rule_set_id;
                row.sort_order = input.sort_order;
                row.enabled = input.enabled;
                row.updated_at = now;
                row.clone()
            } else {
                if id >= t.next_id {
                    t.next_id = id + 1;
                }
                let attach = ProviderRuleSet {
                    id,
                    provider_id: input.provider_id,
                    rule_set_id: input.rule_set_id,
                    sort_order: input.sort_order,
                    enabled: input.enabled,
                    created_at: now,
                    updated_at: now,
                };
                t.rows.push(attach.clone());
                attach
            }
        }
        None => {
            let id = t.next_id;
            t.next_id += 1;
            let attach = ProviderRuleSet {
                id,
                provider_id: input.provider_id,
                rule_set_id: input.rule_set_id,
                sort_order: input.sort_order,
                enabled: input.enabled,
                created_at: now,
                updated_at: now,
            };
            t.rows.push(attach.clone());
            attach
        }
    };

    table::store(&file, &t).await?;
    Ok(stored)
}

pub(crate) async fn delete(root: &Path, id: i64) -> anyhow::Result<bool> {
    let file = path(root);
    let mut t = table::load::<ProviderRuleSet>(&file).await?;
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
    let mut t = table::load::<ProviderRuleSet>(&file).await?;
    let before = t.rows.len();
    t.rows.retain(|r| r.provider_id != provider_id);
    if t.rows.len() != before {
        table::store(&file, &t).await?;
    }
    Ok(())
}

pub(crate) async fn delete_by_rule_set(root: &Path, rule_set_id: i64) -> anyhow::Result<()> {
    let file = path(root);
    let mut t = table::load::<ProviderRuleSet>(&file).await?;
    let before = t.rows.len();
    t.rows.retain(|r| r.rule_set_id != rule_set_id);
    if t.rows.len() != before {
        table::store(&file, &t).await?;
    }
    Ok(())
}
