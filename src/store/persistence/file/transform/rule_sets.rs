//! File-backend rule-set ops over `rule_sets.json`.

use std::path::{Path, PathBuf};

use crate::store::persistence::records::{RuleSet, RuleSetInput};

use crate::store::persistence::file::table::{self, now_secs};

fn path(root: &Path) -> PathBuf {
    root.join("rule_sets.json")
}

pub(crate) async fn list(root: &Path) -> anyhow::Result<Vec<RuleSet>> {
    Ok(table::load::<RuleSet>(&path(root)).await?.rows)
}

pub(crate) async fn get(root: &Path, id: i64) -> anyhow::Result<Option<RuleSet>> {
    Ok(table::load::<RuleSet>(&path(root))
        .await?
        .rows
        .into_iter()
        .find(|r| r.id == id))
}

pub(crate) async fn get_by_name(root: &Path, name: &str) -> anyhow::Result<Option<RuleSet>> {
    Ok(table::load::<RuleSet>(&path(root))
        .await?
        .rows
        .into_iter()
        .find(|r| r.name == name))
}

pub(crate) async fn upsert(root: &Path, input: RuleSetInput) -> anyhow::Result<RuleSet> {
    let file = path(root);
    let mut t = table::load::<RuleSet>(&file).await?;
    let now = now_secs();

    if let Some(existing) = t.rows.iter().find(|r| r.name == input.name)
        && Some(existing.id) != input.id
    {
        return Err(crate::store::persistence::ConflictError::new(format!(
            "rule set name already exists: {}",
            input.name
        ))
        .into());
    }

    let stored = match input.id {
        Some(id) => {
            if let Some(row) = t.rows.iter_mut().find(|r| r.id == id) {
                row.name = input.name;
                row.enabled = input.enabled;
                row.description = input.description;
                row.updated_at = now;
                row.clone()
            } else {
                if id >= t.next_id {
                    t.next_id = id + 1;
                }
                let set = RuleSet {
                    id,
                    name: input.name,
                    enabled: input.enabled,
                    description: input.description,
                    created_at: now,
                    updated_at: now,
                };
                t.rows.push(set.clone());
                set
            }
        }
        None => {
            let id = t.next_id;
            t.next_id += 1;
            let set = RuleSet {
                id,
                name: input.name,
                enabled: input.enabled,
                description: input.description,
                created_at: now,
                updated_at: now,
            };
            t.rows.push(set.clone());
            set
        }
    };

    table::store(&file, &t).await?;
    Ok(stored)
}

pub(crate) async fn delete(root: &Path, id: i64) -> anyhow::Result<bool> {
    // cascade: this set's rules and its provider attachments (not the providers).
    super::rules::delete_by_rule_set(root, id).await?;
    super::provider_rule_sets::delete_by_rule_set(root, id).await?;

    let file = path(root);
    let mut t = table::load::<RuleSet>(&file).await?;
    let before = t.rows.len();
    t.rows.retain(|r| r.id != id);
    let removed = t.rows.len() != before;
    if removed {
        table::store(&file, &t).await?;
    }
    Ok(removed)
}
