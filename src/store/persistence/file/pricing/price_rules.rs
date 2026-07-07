//! File-backend pricing rule ops over `price_rules.json`.

use std::path::{Path, PathBuf};

use crate::store::persistence::file::table::{self, now_secs};
use crate::store::persistence::records::{PriceRule, PriceRuleInput};

pub(crate) fn path(root: &Path) -> PathBuf {
    root.join("price_rules.json")
}

pub(crate) async fn list(root: &Path) -> anyhow::Result<Vec<PriceRule>> {
    Ok(table::load::<PriceRule>(&path(root)).await?.rows)
}

pub(crate) async fn upsert(root: &Path, input: PriceRuleInput) -> anyhow::Result<PriceRule> {
    let file = path(root);
    let mut t = table::load::<PriceRule>(&file).await?;
    let now = now_secs();

    let stored = match input.id {
        Some(id) => {
            if let Some(row) = t.rows.iter_mut().find(|r| r.id == id) {
                row.provider_id = input.provider_id;
                row.match_type = input.match_type;
                row.model_match = input.model_match;
                row.input_price = input.input_price;
                row.output_price = input.output_price;
                row.cache_read_price = input.cache_read_price;
                row.cache_creation_5m_price = input.cache_creation_5m_price;
                row.cache_creation_1h_price = input.cache_creation_1h_price;
                row.image_price = input.image_price;
                row.enabled = input.enabled;
                row.updated_at = now;
                row.clone()
            } else {
                if id >= t.next_id {
                    t.next_id = id + 1;
                }
                let rule = PriceRule {
                    id,
                    provider_id: input.provider_id,
                    match_type: input.match_type,
                    model_match: input.model_match,
                    input_price: input.input_price,
                    output_price: input.output_price,
                    cache_read_price: input.cache_read_price,
                    cache_creation_5m_price: input.cache_creation_5m_price,
                    cache_creation_1h_price: input.cache_creation_1h_price,
                    image_price: input.image_price,
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
            let rule = PriceRule {
                id,
                provider_id: input.provider_id,
                match_type: input.match_type,
                model_match: input.model_match,
                input_price: input.input_price,
                output_price: input.output_price,
                cache_read_price: input.cache_read_price,
                cache_creation_5m_price: input.cache_creation_5m_price,
                cache_creation_1h_price: input.cache_creation_1h_price,
                image_price: input.image_price,
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
    let mut t = table::load::<PriceRule>(&file).await?;
    let before = t.rows.len();
    t.rows.retain(|r| r.id != id);
    let removed = t.rows.len() != before;
    if removed {
        table::store(&file, &t).await?;
    }
    Ok(removed)
}
