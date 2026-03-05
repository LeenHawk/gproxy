use gproxy_storage::{
    Scope, StorageWriteEvent, StorageWriteSender, UsageQuery, UsageQueryCount, UsageQueryRow,
    UsageSummary, UserKeyQueryRow, UserKeyWrite,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::error::AdminApiError;
use crate::{MemoryUser, MemoryUserKey, generate_user_api_key};

const MAX_GENERATE_KEY_ATTEMPTS: usize = 32;

pub fn extract_api_key(provided: Option<&str>) -> Result<&str, AdminApiError> {
    provided.ok_or(AdminApiError::Unauthorized)
}

pub fn authenticate_user_password(
    users: &[MemoryUser],
    username: &str,
    password: &str,
) -> Result<MemoryUser, AdminApiError> {
    let name = username.trim();
    let pass = password.trim();
    if name.is_empty() || pass.is_empty() {
        return Err(AdminApiError::Unauthorized);
    }
    let user = users
        .iter()
        .find(|item| item.enabled && item.name == name)
        .ok_or(AdminApiError::Unauthorized)?;
    if user.password != pass {
        return Err(AdminApiError::Unauthorized);
    }
    Ok(user.clone())
}

pub async fn authenticate_user_key(
    users: &[MemoryUser],
    keys: &HashMap<String, MemoryUserKey>,
    api_key: &str,
) -> Result<MemoryUserKey, AdminApiError> {
    let key = keys.get(api_key).ok_or(AdminApiError::Unauthorized)?;
    if !key.enabled {
        return Err(AdminApiError::Unauthorized);
    }
    let user = users
        .iter()
        .find(|item| item.id == key.user_id)
        .ok_or(AdminApiError::Unauthorized)?;
    if !user.enabled {
        return Err(AdminApiError::Unauthorized);
    }
    Ok(key.clone())
}

pub async fn query_my_user_keys(
    users: &[MemoryUser],
    keys: &HashMap<String, MemoryUserKey>,
    api_key: &str,
) -> Result<Vec<UserKeyQueryRow>, AdminApiError> {
    let me = authenticate_user_key(users, keys, api_key).await?;
    let rows: Vec<UserKeyQueryRow> = keys
        .values()
        .filter(|row| row.user_id == me.user_id)
        .map(|row| UserKeyQueryRow {
            id: row.id,
            user_id: row.user_id,
            api_key: row.api_key.clone(),
        })
        .collect();
    Ok(rows)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpsertMyKeyInput {
    pub id: Option<i64>,
    pub api_key: String,
    pub label: Option<String>,
    pub enabled: bool,
}

pub async fn upsert_my_user_key(
    writer: &StorageWriteSender,
    current_api_key: &str,
    users: &[MemoryUser],
    keys: &HashMap<String, MemoryUserKey>,
    input: UpsertMyKeyInput,
) -> Result<UserKeyWrite, AdminApiError> {
    let me = authenticate_user_key(users, keys, current_api_key).await?;
    let id = input
        .id
        .ok_or_else(|| AdminApiError::InvalidInput("id is required".to_string()))?;
    let existing_by_id = input
        .id
        .and_then(|key_id| keys.values().find(|row| row.id == key_id));
    if let Some(existing) = existing_by_id
        && existing.user_id != me.user_id
    {
        return Err(AdminApiError::Forbidden);
    }

    let api_key = if let Some(existing) = existing_by_id {
        existing.api_key.clone()
    } else {
        generate_unique_user_api_key(keys)?
    };

    let payload = UserKeyWrite {
        id,
        user_id: me.user_id,
        api_key,
        label: input.label,
        enabled: input.enabled,
    };

    writer
        .enqueue(StorageWriteEvent::UpsertUserKey(payload.clone()))
        .await?;
    Ok(payload)
}

pub fn generate_unique_user_api_key(
    keys: &HashMap<String, MemoryUserKey>,
) -> Result<String, AdminApiError> {
    for _ in 0..MAX_GENERATE_KEY_ATTEMPTS {
        let candidate = generate_user_api_key();
        if !keys.contains_key(candidate.as_str()) {
            return Ok(candidate);
        }
    }
    Err(AdminApiError::InvalidInput(
        "failed to generate unique user key".to_string(),
    ))
}

pub async fn delete_my_user_key(
    writer: &StorageWriteSender,
    current_api_key: &str,
    users: &[MemoryUser],
    keys: &HashMap<String, MemoryUserKey>,
    key_id: i64,
) -> Result<(), AdminApiError> {
    let me = authenticate_user_key(users, keys, current_api_key).await?;
    let target = keys
        .values()
        .find(|row| row.id == key_id)
        .ok_or_else(|| AdminApiError::NotFound(format!("user_key {key_id}")))?;

    if target.user_id != me.user_id {
        return Err(AdminApiError::Forbidden);
    }

    writer
        .enqueue(StorageWriteEvent::DeleteUserKey { id: key_id })
        .await?;
    Ok(())
}

pub async fn query_my_usages(
    storage: &gproxy_storage::SeaOrmStorage,
    users: &[MemoryUser],
    keys: &HashMap<String, MemoryUserKey>,
    current_api_key: &str,
    mut query: UsageQuery,
) -> Result<Vec<UsageQueryRow>, AdminApiError> {
    let me = authenticate_user_key(users, keys, current_api_key).await?;
    query.user_id = Scope::Eq(me.user_id);
    Ok(storage.query_usages(&query).await?)
}

pub async fn summarize_my_usages(
    storage: &gproxy_storage::SeaOrmStorage,
    users: &[MemoryUser],
    keys: &HashMap<String, MemoryUserKey>,
    current_api_key: &str,
    mut query: UsageQuery,
) -> Result<UsageSummary, AdminApiError> {
    let me = authenticate_user_key(users, keys, current_api_key).await?;
    query.user_id = Scope::Eq(me.user_id);
    Ok(storage.summarize_usages(&query).await?)
}

pub async fn count_my_usages(
    storage: &gproxy_storage::SeaOrmStorage,
    users: &[MemoryUser],
    keys: &HashMap<String, MemoryUserKey>,
    current_api_key: &str,
    mut query: UsageQuery,
) -> Result<UsageQueryCount, AdminApiError> {
    let me = authenticate_user_key(users, keys, current_api_key).await?;
    query.user_id = Scope::Eq(me.user_id);
    Ok(storage.count_usages(&query).await?)
}
