use std::collections::BTreeMap;

use bytes::Bytes;
use gproxy_store::records::{CredentialInput, UserKeyInput};
use serde::Serialize;

use crate::dto::*;
use crate::route::Entity;
use crate::{AdminError, State};

#[derive(Default)]
pub(super) struct IdMaps {
    pub organizations: BTreeMap<i64, i64>,
    pub teams: BTreeMap<i64, i64>,
    pub users: BTreeMap<i64, i64>,
    pub providers: BTreeMap<i64, i64>,
    pub credentials: BTreeMap<i64, i64>,
    pub user_keys: BTreeMap<i64, i64>,
    pub routes: BTreeMap<i64, i64>,
    pub price_rules: BTreeMap<i64, i64>,
    pub rule_sets: BTreeMap<i64, i64>,
}

pub(super) async fn import_credentials(
    state: &impl State,
    values: Vec<ExportCredentialDto>,
    source: Option<&ExportSourceKeyDto>,
    source_master_key: Option<&str>,
    maps: &mut IdMaps,
) -> Result<(u64, u64), AdminError> {
    let mut imported = 0;
    let mut skipped = 0;
    for value in values {
        let Some(secret) = value.secret else {
            skipped += 1;
            continue;
        };
        let source = source.ok_or_else(|| {
            AdminError::BadRequest("config-only export contains a credential secret".into())
        })?;
        let secret = state.open_imported_credential(&secret.into(), source, source_master_key)?;
        let config = value.config;
        if let Some(fingerprint) = &config.tls_fingerprint {
            fingerprint
                .validate()
                .map_err(|error| AdminError::BadRequest(error.into()))?;
        }
        let id = state
            .store()
            .insert_credential(&CredentialInput {
                provider_id: mapped(&maps.providers, config.provider_id)?,
                label: config
                    .label
                    .or_else(|| crate::default_credential_label(&config.kind, &secret)),
                kind: config.kind,
                envelope: state.seal_credential(&secret)?,
                enabled: config.enabled,
                weight: config.weight,
                rpm_limit: config.rpm_limit,
                tpm_limit: config.tpm_limit,
                proxy_url: config.proxy_url,
                tls_fingerprint: config
                    .tls_fingerprint
                    .map(serde_json::to_value)
                    .transpose()
                    .map_err(|error| AdminError::BadRequest(error.to_string()))?,
            })
            .await?;
        maps.credentials.insert(config.id, id);
        imported += 1;
    }
    Ok((imported, skipped))
}

pub(super) async fn import_user_keys(
    state: &impl State,
    values: Vec<ExportUserKeyDto>,
    source: Option<&ExportSourceKeyDto>,
    source_master_key: Option<&str>,
    maps: &mut IdMaps,
) -> Result<(u64, u64), AdminError> {
    let mut imported = 0;
    let mut skipped = 0;
    let existing = state.store().control_snapshot().await?.user_keys;
    for value in values {
        let user_id = mapped(&maps.users, value.config.user_id)?;
        if let Some(current) = existing.iter().find(|current| {
            current.user_id == user_id
                && current.digest_version == value.digest_version
                && current.digest == value.digest
        }) {
            maps.user_keys.insert(value.config.id, current.id);
            continue;
        }
        let Some(secret) = value.secret else {
            skipped += 1;
            continue;
        };
        let source = source.ok_or_else(|| {
            AdminError::BadRequest("config-only export contains a user-key secret".into())
        })?;
        let envelope = state.reseal_imported_user_key(&secret.into(), source, source_master_key)?;
        let config = value.config;
        let id = state
            .store()
            .insert_user_key(&UserKeyInput {
                user_id,
                digest: value.digest,
                digest_version: value.digest_version,
                prefix: config.prefix.ok_or_else(|| {
                    AdminError::BadRequest("exported user key has no prefix".into())
                })?,
                envelope,
                label: config.label,
                expires_at: config.expires_at,
                enabled: config.enabled,
            })
            .await?;
        maps.user_keys.insert(config.id, id);
        imported += 1;
    }
    Ok((imported, skipped))
}

pub(super) async fn create(
    state: &impl State,
    entity: Entity,
    value: &impl Serialize,
) -> Result<i64, AdminError> {
    let body = serde_json::to_vec(value)
        .map(Bytes::from)
        .map_err(|error| AdminError::BadRequest(error.to_string()))?;
    let response = super::super::create(state, entity, &body).await?;
    serde_json::from_slice::<IdResponse>(response.body())
        .map(|value| value.id)
        .map_err(|error| AdminError::Internal(error.to_string()))
}

pub(super) async fn map_create(
    state: &impl State,
    entity: Entity,
    old: i64,
    value: &impl Serialize,
    map: &mut BTreeMap<i64, i64>,
) -> Result<(), AdminError> {
    map.insert(old, create(state, entity, value).await?);
    Ok(())
}

pub(super) fn mapped(map: &BTreeMap<i64, i64>, id: i64) -> Result<i64, AdminError> {
    map.get(&id)
        .copied()
        .ok_or_else(|| AdminError::BadRequest(format!("export references missing id {id}")))
}

pub(super) fn optional(
    map: &BTreeMap<i64, i64>,
    id: Option<i64>,
) -> Result<Option<i64>, AdminError> {
    id.map(|id| mapped(map, id)).transpose()
}

pub(super) fn subject(maps: &IdMaps, kind: &str, id: i64) -> Result<Option<i64>, AdminError> {
    match kind {
        "organization" => mapped(&maps.organizations, id).map(Some),
        "team" => mapped(&maps.teams, id).map(Some),
        "user" => mapped(&maps.users, id).map(Some),
        "user_key" => Ok(maps.user_keys.get(&id).copied()),
        _ => Err(AdminError::BadRequest(
            "export contains an unknown quota subject kind".into(),
        )),
    }
}
