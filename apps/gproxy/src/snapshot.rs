use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use gproxy_core::{AuthKeyEntry, AuthSnapshot, UserEntry};
use gproxy_provider_core::{
    CredentialEntry, DisallowEntry, DisallowKey, DisallowLevel, DisallowScope, PoolSnapshot,
};
use gproxy_provider_impl::BaseCredential;
use gproxy_storage::StorageSnapshot;
use time::OffsetDateTime;

pub fn build_provider_id_map(snapshot: &StorageSnapshot) -> HashMap<String, i64> {
    let mut map = HashMap::new();
    for provider in &snapshot.providers {
        map.insert(provider.name.clone(), provider.id);
    }
    map
}

pub fn build_provider_name_map(snapshot: &StorageSnapshot) -> HashMap<i64, String> {
    let mut map = HashMap::new();
    for provider in &snapshot.providers {
        map.insert(provider.id, provider.name.clone());
    }
    map
}

pub fn build_auth_snapshot(snapshot: &StorageSnapshot) -> AuthSnapshot {
    let mut keys_by_value = HashMap::new();
    for key in &snapshot.api_keys {
        keys_by_value.insert(
            key.key_value.clone(),
            AuthKeyEntry {
                key_id: key.id,
                user_id: key.user_id,
                enabled: key.enabled,
            },
        );
    }

    let mut users_by_id = HashMap::new();
    for user in &snapshot.users {
        users_by_id.insert(
            user.id,
            UserEntry {
                id: user.id,
                name: user.name.clone(),
            },
        );
    }

    AuthSnapshot {
        keys_by_value,
        users_by_id,
    }
}

pub fn build_provider_pools(
    snapshot: &StorageSnapshot,
) -> HashMap<String, PoolSnapshot<BaseCredential>> {
    let mut provider_by_id = HashMap::new();
    let mut provider_config_by_id: HashMap<i64, serde_json::Value> = HashMap::new();
    for provider in &snapshot.providers {
        provider_by_id.insert(provider.id, provider.name.clone());
        provider_config_by_id.insert(provider.id, provider.config_json.clone());
    }

    let mut credentials_by_provider: HashMap<String, Vec<CredentialEntry<BaseCredential>>> =
        HashMap::new();
    let mut credential_provider_by_id: HashMap<i64, String> = HashMap::new();

    for credential in &snapshot.credentials {
        let Some(provider_name) = provider_by_id.get(&credential.provider_id) else {
            continue;
        };

        credential_provider_by_id
            .insert(credential.id, provider_name.clone());

        let weight = if credential.weight >= 0 {
            credential.weight as u32
        } else {
            0
        };

        let provider_config = provider_config_by_id
            .get(&credential.provider_id)
            .unwrap_or(&serde_json::Value::Null);
        let meta = merge_meta(provider_config, &credential.meta_json);
        let entry = CredentialEntry::new(
            credential.id.to_string(),
            credential.enabled,
            weight,
            BaseCredential {
                id: credential.id,
                name: credential.name.clone(),
                secret: credential.secret.clone(),
                meta,
            },
        );

        credentials_by_provider
            .entry(provider_name.clone())
            .or_default()
            .push(entry);
    }

    let mut disallow_by_provider: HashMap<String, HashMap<DisallowKey, DisallowEntry>> =
        HashMap::new();

    for record in &snapshot.disallow {
        let Some(provider_name) = credential_provider_by_id.get(&record.credential_id) else {
            continue;
        };

        let Some(scope) = parse_disallow_scope(
            record.scope_kind.as_str(),
            record.scope_value.as_deref(),
        ) else {
            continue;
        };
        let Some(level) = parse_disallow_level(record.level.as_str()) else {
            continue;
        };

        let entry = DisallowEntry {
            level,
            until: record.until_at.and_then(to_system_time),
            reason: record.reason.clone(),
            updated_at: to_system_time(record.updated_at)
                .unwrap_or(SystemTime::UNIX_EPOCH),
        };

        let key = DisallowKey::new(record.credential_id.to_string(), scope);
        disallow_by_provider
            .entry(provider_name.clone())
            .or_default()
            .insert(key, entry);
    }

    let mut pools = HashMap::new();
    for provider in &snapshot.providers {
        let name = provider.name.clone();
        let credentials = credentials_by_provider.remove(&name).unwrap_or_default();
        let disallow = disallow_by_provider.remove(&name).unwrap_or_default();
        pools.insert(name, PoolSnapshot::new(credentials, disallow));
    }

    pools
}

const CHANNEL_META_KEYS: &[&str] = &[
    "base_url",
    "claude_ai_base_url",
    "console_base_url",
];

fn merge_meta(provider: &serde_json::Value, credential: &serde_json::Value) -> serde_json::Value {
    match credential {
        serde_json::Value::Object(cred_map) => match provider {
            serde_json::Value::Object(provider_map) => {
                let mut merged = provider_map.clone();
                for (key, value) in cred_map {
                    if CHANNEL_META_KEYS.contains(&key.as_str()) {
                        continue;
                    }
                    merged.insert(key.clone(), value.clone());
                }
                serde_json::Value::Object(merged)
            }
            _ => credential.clone(),
        },
        serde_json::Value::Null => provider.clone(),
        _ => credential.clone(),
    }
}

fn parse_disallow_scope(kind: &str, value: Option<&str>) -> Option<DisallowScope> {
    match kind {
        "all_models" | "all" => Some(DisallowScope::AllModels),
        "model" => value.map(|model| DisallowScope::Model(model.to_string())),
        _ => None,
    }
}

fn parse_disallow_level(level: &str) -> Option<DisallowLevel> {
    match level {
        "cooldown" => Some(DisallowLevel::Cooldown),
        "transient" => Some(DisallowLevel::Transient),
        "dead" => Some(DisallowLevel::Dead),
        _ => None,
    }
}

fn to_system_time(value: OffsetDateTime) -> Option<SystemTime> {
    let ts = value.unix_timestamp();
    if ts < 0 {
        return None;
    }
    Some(SystemTime::UNIX_EPOCH + Duration::from_secs(ts as u64))
}
