//! File-backend rule ops over `rules.json`.

use std::path::{Path, PathBuf};

use crate::store::persistence::records::{Rule, RuleInput};

use crate::store::persistence::file::table::{self, now_secs};

fn path(root: &Path) -> PathBuf {
    root.join("rules.json")
}

pub(crate) async fn list(root: &Path, rule_set_id: i64) -> anyhow::Result<Vec<Rule>> {
    Ok(table::load::<Rule>(&path(root))
        .await?
        .rows
        .into_iter()
        .filter(|r| r.rule_set_id == rule_set_id)
        .collect())
}

pub(crate) async fn get(root: &Path, id: i64) -> anyhow::Result<Option<Rule>> {
    Ok(table::load::<Rule>(&path(root))
        .await?
        .rows
        .into_iter()
        .find(|r| r.id == id))
}

pub(crate) async fn upsert(root: &Path, input: RuleInput) -> anyhow::Result<Rule> {
    let file = path(root);
    let mut t = table::load::<Rule>(&file).await?;
    let now = now_secs();

    let stored = match input.id {
        Some(id) => {
            if let Some(row) = t.rows.iter_mut().find(|r| r.id == id) {
                row.rule_set_id = input.rule_set_id;
                row.kind = input.kind;
                row.config_json = input.config_json;
                row.filter_model_pattern = input.filter_model_pattern;
                row.filter_operation_keys = input.filter_operation_keys;
                row.sort_order = input.sort_order;
                row.enabled = input.enabled;
                row.updated_at = now;
                row.clone()
            } else {
                if id >= t.next_id {
                    t.next_id = id + 1;
                }
                let rule = Rule {
                    id,
                    rule_set_id: input.rule_set_id,
                    kind: input.kind,
                    config_json: input.config_json,
                    filter_model_pattern: input.filter_model_pattern,
                    filter_operation_keys: input.filter_operation_keys,
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
            let rule = Rule {
                id,
                rule_set_id: input.rule_set_id,
                kind: input.kind,
                config_json: input.config_json,
                filter_model_pattern: input.filter_model_pattern,
                filter_operation_keys: input.filter_operation_keys,
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
    let mut t = table::load::<Rule>(&file).await?;
    let before = t.rows.len();
    t.rows.retain(|r| r.id != id);
    let removed = t.rows.len() != before;
    if removed {
        table::store(&file, &t).await?;
    }
    Ok(removed)
}

pub(crate) async fn delete_by_rule_set(root: &Path, rule_set_id: i64) -> anyhow::Result<()> {
    let file = path(root);
    let mut t = table::load::<Rule>(&file).await?;
    let before = t.rows.len();
    t.rows.retain(|r| r.rule_set_id != rule_set_id);
    if t.rows.len() != before {
        table::store(&file, &t).await?;
    }
    Ok(())
}
