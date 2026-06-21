//! File-backend provider-model ops over `provider_models.json`.

use std::path::{Path, PathBuf};

use crate::store::persistence::records::{ProviderModel, ProviderModelInput};

use crate::store::persistence::file::table::{self, now_secs};

fn path(root: &Path) -> PathBuf {
    root.join("provider_models.json")
}

pub(crate) async fn list(root: &Path, provider_id: i64) -> anyhow::Result<Vec<ProviderModel>> {
    Ok(table::load::<ProviderModel>(&path(root))
        .await?
        .rows
        .into_iter()
        .filter(|m| m.provider_id == provider_id)
        .collect())
}

pub(crate) async fn upsert(
    root: &Path,
    input: ProviderModelInput,
) -> anyhow::Result<ProviderModel> {
    let file = path(root);
    let mut t = table::load::<ProviderModel>(&file).await?;
    let now = now_secs();

    let stored = match input.id {
        Some(id) => {
            if let Some(row) = t.rows.iter_mut().find(|m| m.id == id) {
                row.provider_id = input.provider_id;
                row.model_id = input.model_id;
                row.display_name = input.display_name;
                row.pricing_json = input.pricing_json;
                row.variants_json = input.variants_json;
                row.enabled = input.enabled;
                row.updated_at = now;
                row.clone()
            } else {
                if id >= t.next_id {
                    t.next_id = id + 1;
                }
                let model = ProviderModel {
                    id,
                    provider_id: input.provider_id,
                    model_id: input.model_id,
                    display_name: input.display_name,
                    pricing_json: input.pricing_json,
                    variants_json: input.variants_json,
                    enabled: input.enabled,
                    created_at: now,
                    updated_at: now,
                };
                t.rows.push(model.clone());
                model
            }
        }
        None => {
            let id = t.next_id;
            t.next_id += 1;
            let model = ProviderModel {
                id,
                provider_id: input.provider_id,
                model_id: input.model_id,
                display_name: input.display_name,
                pricing_json: input.pricing_json,
                variants_json: input.variants_json,
                enabled: input.enabled,
                created_at: now,
                updated_at: now,
            };
            t.rows.push(model.clone());
            model
        }
    };

    table::store(&file, &t).await?;
    Ok(stored)
}

pub(crate) async fn delete(root: &Path, id: i64) -> anyhow::Result<bool> {
    let file = path(root);
    let mut t = table::load::<ProviderModel>(&file).await?;
    let before = t.rows.len();
    t.rows.retain(|m| m.id != id);
    let removed = t.rows.len() != before;
    if removed {
        table::store(&file, &t).await?;
    }
    Ok(removed)
}

pub(crate) async fn delete_by_provider(root: &Path, provider_id: i64) -> anyhow::Result<()> {
    let file = path(root);
    let mut t = table::load::<ProviderModel>(&file).await?;
    let before = t.rows.len();
    t.rows.retain(|m| m.provider_id != provider_id);
    if t.rows.len() != before {
        table::store(&file, &t).await?;
    }
    Ok(())
}
