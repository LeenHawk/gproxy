//! File-backend provider ops over `providers.json`.

use std::path::{Path, PathBuf};

use crate::store::persistence::records::{Provider, ProviderInput};

use crate::store::persistence::file::table::{self, now_secs};

fn path(root: &Path) -> PathBuf {
    root.join("providers.json")
}

pub(crate) async fn list(root: &Path) -> anyhow::Result<Vec<Provider>> {
    Ok(table::load::<Provider>(&path(root)).await?.rows)
}

pub(crate) async fn get(root: &Path, id: i64) -> anyhow::Result<Option<Provider>> {
    Ok(table::load::<Provider>(&path(root))
        .await?
        .rows
        .into_iter()
        .find(|p| p.id == id))
}

pub(crate) async fn get_by_name(root: &Path, name: &str) -> anyhow::Result<Option<Provider>> {
    Ok(table::load::<Provider>(&path(root))
        .await?
        .rows
        .into_iter()
        .find(|p| p.name == name))
}

pub(crate) async fn upsert(root: &Path, input: ProviderInput) -> anyhow::Result<Provider> {
    let file = path(root);
    let mut t = table::load::<Provider>(&file).await?;
    let now = now_secs();

    // Reject name collisions with a different row.
    if let Some(existing) = t.rows.iter().find(|p| p.name == input.name)
        && Some(existing.id) != input.id
    {
        return Err(crate::store::persistence::ConflictError::new(format!(
            "provider name already exists: {}",
            input.name
        ))
        .into());
    }

    let stored = match input.id {
        Some(id) => {
            if let Some(row) = t.rows.iter_mut().find(|p| p.id == id) {
                row.name = input.name;
                row.channel = input.channel;
                row.label = input.label;
                row.settings_json = input.settings_json;
                row.credential_strategy = input.credential_strategy;
                row.proxy_url = input.proxy_url;
                row.tls_fingerprint = input.tls_fingerprint;
                row.enabled = input.enabled;
                row.updated_at = now;
                row.clone()
            } else {
                if id >= t.next_id {
                    t.next_id = id + 1;
                }
                let provider = Provider {
                    id,
                    name: input.name,
                    channel: input.channel,
                    label: input.label,
                    settings_json: input.settings_json,
                    credential_strategy: input.credential_strategy,
                    proxy_url: input.proxy_url,
                    tls_fingerprint: input.tls_fingerprint,
                    enabled: input.enabled,
                    created_at: now,
                    updated_at: now,
                };
                t.rows.push(provider.clone());
                provider
            }
        }
        None => {
            let id = t.next_id;
            t.next_id += 1;
            let provider = Provider {
                id,
                name: input.name,
                channel: input.channel,
                label: input.label,
                settings_json: input.settings_json,
                credential_strategy: input.credential_strategy,
                proxy_url: input.proxy_url,
                tls_fingerprint: input.tls_fingerprint,
                enabled: input.enabled,
                created_at: now,
                updated_at: now,
            };
            t.rows.push(provider.clone());
            provider
        }
    };

    table::store(&file, &t).await?;
    Ok(stored)
}

pub(crate) async fn delete(root: &Path, id: i64) -> anyhow::Result<bool> {
    // cascade: this provider's credentials (and their statuses) and models.
    for cred in super::credentials::list(root, id).await? {
        super::credential_statuses::delete_by_credential(root, cred.id).await?;
    }
    super::credentials::delete_by_provider(root, id).await?;
    super::provider_models::delete_by_provider(root, id).await?;

    // §8-B2 rules cascade.
    crate::store::persistence::file::transform::routing_rules::delete_by_provider(root, id).await?;
    crate::store::persistence::file::transform::provider_rule_sets::delete_by_provider(root, id)
        .await?;

    let file = path(root);
    let mut t = table::load::<Provider>(&file).await?;
    let before = t.rows.len();
    t.rows.retain(|p| p.id != id);
    let removed = t.rows.len() != before;
    if removed {
        table::store(&file, &t).await?;
    }
    Ok(removed)
}
