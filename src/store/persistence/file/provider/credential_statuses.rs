//! File-backend credential-status ops over `credential_statuses.json`.

use std::path::{Path, PathBuf};

use crate::store::persistence::records::{CredentialStatus, CredentialStatusInput};

use crate::store::persistence::file::table::{self, now_secs};

fn path(root: &Path) -> PathBuf {
    root.join("credential_statuses.json")
}

pub(crate) async fn list(root: &Path, credential_id: i64) -> anyhow::Result<Vec<CredentialStatus>> {
    Ok(table::load::<CredentialStatus>(&path(root))
        .await?
        .rows
        .into_iter()
        .filter(|s| s.credential_id == credential_id)
        .collect())
}

pub(crate) async fn list_all(root: &Path) -> anyhow::Result<Vec<CredentialStatus>> {
    Ok(table::load::<CredentialStatus>(&path(root)).await?.rows)
}

pub(crate) async fn upsert(
    root: &Path,
    input: CredentialStatusInput,
) -> anyhow::Result<CredentialStatus> {
    let file = path(root);
    let mut t = table::load::<CredentialStatus>(&file).await?;
    let now = now_secs();

    // Locate by explicit id, else by (credential_id, channel) uniqueness.
    let existing_idx = match input.id {
        Some(id) => t.rows.iter().position(|s| s.id == id),
        None => t
            .rows
            .iter()
            .position(|s| s.credential_id == input.credential_id && s.channel == input.channel),
    };

    let stored = match existing_idx {
        Some(i) => {
            let row = &mut t.rows[i];
            row.credential_id = input.credential_id;
            row.channel = input.channel;
            row.health_kind = input.health_kind;
            row.health_json = input.health_json;
            row.checked_at = input.checked_at;
            row.last_error = input.last_error;
            row.updated_at = now;
            row.clone()
        }
        None => {
            let id = t.next_id;
            t.next_id += 1;
            let status = CredentialStatus {
                id,
                credential_id: input.credential_id,
                channel: input.channel,
                health_kind: input.health_kind,
                health_json: input.health_json,
                checked_at: input.checked_at,
                last_error: input.last_error,
                created_at: now,
                updated_at: now,
            };
            t.rows.push(status.clone());
            status
        }
    };

    table::store(&file, &t).await?;
    Ok(stored)
}

pub(crate) async fn delete(root: &Path, id: i64) -> anyhow::Result<bool> {
    let file = path(root);
    let mut t = table::load::<CredentialStatus>(&file).await?;
    let before = t.rows.len();
    t.rows.retain(|s| s.id != id);
    let removed = t.rows.len() != before;
    if removed {
        table::store(&file, &t).await?;
    }
    Ok(removed)
}

pub(crate) async fn delete_by_credential(root: &Path, credential_id: i64) -> anyhow::Result<()> {
    let file = path(root);
    let mut t = table::load::<CredentialStatus>(&file).await?;
    let before = t.rows.len();
    t.rows.retain(|s| s.credential_id != credential_id);
    if t.rows.len() != before {
        table::store(&file, &t).await?;
    }
    Ok(())
}
