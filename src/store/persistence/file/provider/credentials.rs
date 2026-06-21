//! File-backend credential ops over `credentials.json`.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::store::persistence::records::{Credential, CredentialInput};

use crate::store::persistence::file::table::{self, now_secs};

fn path(root: &Path) -> PathBuf {
    root.join("credentials.json")
}

pub(crate) async fn list(root: &Path, provider_id: i64) -> anyhow::Result<Vec<Credential>> {
    Ok(table::load::<Credential>(&path(root))
        .await?
        .rows
        .into_iter()
        .filter(|c| c.provider_id == provider_id)
        .collect())
}

pub(crate) async fn get(root: &Path, id: i64) -> anyhow::Result<Option<Credential>> {
    Ok(table::load::<Credential>(&path(root))
        .await?
        .rows
        .into_iter()
        .find(|c| c.id == id))
}

pub(crate) async fn upsert(root: &Path, input: CredentialInput) -> anyhow::Result<Credential> {
    let file = path(root);
    let mut t = table::load::<Credential>(&file).await?;
    let now = now_secs();

    let stored = match input.id {
        Some(id) => {
            if let Some(row) = t.rows.iter_mut().find(|c| c.id == id) {
                row.provider_id = input.provider_id;
                row.name = input.name;
                row.kind = input.kind;
                row.secret_json = input.secret_json;
                row.weight = input.weight;
                row.rpm_limit = input.rpm_limit;
                row.tpm_limit = input.tpm_limit;
                row.proxy_url = input.proxy_url;
                row.tls_fingerprint = input.tls_fingerprint;
                row.enabled = input.enabled;
                row.updated_at = now;
                row.clone()
            } else {
                if id >= t.next_id {
                    t.next_id = id + 1;
                }
                let cred = Credential {
                    id,
                    provider_id: input.provider_id,
                    name: input.name,
                    kind: input.kind,
                    secret_json: input.secret_json,
                    weight: input.weight,
                    rpm_limit: input.rpm_limit,
                    tpm_limit: input.tpm_limit,
                    proxy_url: input.proxy_url,
                    tls_fingerprint: input.tls_fingerprint,
                    enabled: input.enabled,
                    created_at: now,
                    updated_at: now,
                };
                t.rows.push(cred.clone());
                cred
            }
        }
        None => {
            let id = t.next_id;
            t.next_id += 1;
            let cred = Credential {
                id,
                provider_id: input.provider_id,
                name: input.name,
                kind: input.kind,
                secret_json: input.secret_json,
                weight: input.weight,
                rpm_limit: input.rpm_limit,
                tpm_limit: input.tpm_limit,
                proxy_url: input.proxy_url,
                tls_fingerprint: input.tls_fingerprint,
                enabled: input.enabled,
                created_at: now,
                updated_at: now,
            };
            t.rows.push(cred.clone());
            cred
        }
    };

    table::store(&file, &t).await?;
    Ok(stored)
}

pub(crate) async fn update_secret_if_current(
    root: &Path,
    id: i64,
    provider_id: i64,
    expected_updated_at: i64,
    secret_json: Value,
) -> anyhow::Result<bool> {
    let file = path(root);
    let mut t = table::load::<Credential>(&file).await?;
    let Some(row) = t.rows.iter_mut().find(|c| {
        c.id == id
            && c.provider_id == provider_id
            && c.enabled
            && c.updated_at == expected_updated_at
    }) else {
        return Ok(false);
    };
    row.secret_json = secret_json;
    row.updated_at = now_secs();
    table::store(&file, &t).await?;
    Ok(true)
}

pub(crate) async fn delete(root: &Path, id: i64) -> anyhow::Result<bool> {
    // cascade: drop this credential's status snapshots first.
    super::credential_statuses::delete_by_credential(root, id).await?;

    let file = path(root);
    let mut t = table::load::<Credential>(&file).await?;
    let before = t.rows.len();
    t.rows.retain(|c| c.id != id);
    let removed = t.rows.len() != before;
    if removed {
        table::store(&file, &t).await?;
    }
    Ok(removed)
}

pub(crate) async fn delete_by_provider(root: &Path, provider_id: i64) -> anyhow::Result<()> {
    let file = path(root);
    let mut t = table::load::<Credential>(&file).await?;
    let before = t.rows.len();
    t.rows.retain(|c| c.provider_id != provider_id);
    if t.rows.len() != before {
        table::store(&file, &t).await?;
    }
    Ok(())
}
