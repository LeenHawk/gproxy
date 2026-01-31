#![allow(clippy::needless_update)]

use std::time::Duration;

use sea_orm::entity::prelude::*;
use sea_orm::sea_query::OnConflict;
use sea_orm::{ActiveValue, EntityTrait, TransactionTrait};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::{self as tokio_time, MissedTickBehavior};
use time::OffsetDateTime;

use crate::entities;
use crate::traffic::{DownstreamTrafficEvent, TrafficStorage, UpstreamTrafficEvent};

#[derive(Debug, Clone)]
pub struct DisallowUpsert {
    pub credential_id: i64,
    pub scope_kind: String,
    pub scope_value: Option<String>,
    pub level: String,
    pub until_at: Option<OffsetDateTime>,
    pub reason: Option<String>,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct ProviderUpsert {
    pub id: i64,
    pub name: String,
    pub config_json: Json,
    pub enabled: bool,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct CredentialUpsert {
    pub id: i64,
    pub provider_id: i64,
    pub name: Option<String>,
    pub secret: Json,
    pub meta_json: Json,
    pub weight: i32,
    pub enabled: bool,
    pub created_at: Option<OffsetDateTime>,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct UserUpsert {
    pub id: i64,
    pub name: Option<String>,
    pub created_at: Option<OffsetDateTime>,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct ApiKeyUpsert {
    pub id: i64,
    pub user_id: i64,
    pub key_value: String,
    pub label: Option<String>,
    pub enabled: bool,
    pub created_at: Option<OffsetDateTime>,
    pub last_used_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone)]
pub struct GlobalConfigUpsert {
    pub id: i64,
    pub config_json: Json,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct ConfigSnapshot {
    pub providers: Vec<ProviderUpsert>,
    pub credentials: Vec<CredentialUpsert>,
    pub users: Vec<UserUpsert>,
    pub api_keys: Vec<ApiKeyUpsert>,
    pub global_config: Vec<GlobalConfigUpsert>,
}

#[derive(Debug, Clone)]
pub enum ControlEvent {
    UpsertDisallow(DisallowUpsert),
}

#[derive(Debug, Clone)]
pub enum ConfigEvent {
    UpsertProvider(ProviderUpsert),
    UpsertCredential(CredentialUpsert),
    UpsertUser(UserUpsert),
    UpsertApiKey(ApiKeyUpsert),
    UpsertGlobalConfig(GlobalConfigUpsert),
    DeleteProvider(i64),
    DeleteCredential(i64),
    DeleteUser(i64),
    DeleteApiKey(i64),
    DeleteGlobalConfig(i64),
    ReplaceSnapshot(ConfigSnapshot),
}

#[derive(Debug, Clone)]
pub struct StorageBusConfig {
    pub control_capacity: usize,
    pub config_capacity: usize,
    pub downstream_capacity: usize,
    pub upstream_capacity: usize,
    pub downstream_batch_size: usize,
    pub upstream_batch_size: usize,
    pub flush_interval: Duration,
    pub retry_delay: Duration,
}

impl Default for StorageBusConfig {
    fn default() -> Self {
        Self {
            control_capacity: 1024,
            config_capacity: 1024,
            downstream_capacity: 65_536,
            upstream_capacity: 65_536,
            downstream_batch_size: 200,
            upstream_batch_size: 200,
            flush_interval: Duration::from_millis(200),
            retry_delay: Duration::from_millis(200),
        }
    }
}

pub struct StorageBus {
    pub control_tx: mpsc::Sender<ControlEvent>,
    pub config_tx: mpsc::Sender<ConfigEvent>,
    pub downstream_tx: mpsc::Sender<DownstreamTrafficEvent>,
    pub upstream_tx: mpsc::Sender<UpstreamTrafficEvent>,
    _handles: Vec<JoinHandle<()>>,
}

impl StorageBus {
    pub fn spawn(storage: TrafficStorage, config: StorageBusConfig) -> Self {
        let (control_tx, control_rx) = mpsc::channel(config.control_capacity);
        let (config_tx, config_rx) = mpsc::channel(config.config_capacity);
        let (downstream_tx, downstream_rx) = mpsc::channel(config.downstream_capacity);
        let (upstream_tx, upstream_rx) = mpsc::channel(config.upstream_capacity);

        let mut handles = Vec::new();
        let control_storage = storage.clone();
        handles.push(tokio::spawn(control_writer(
            control_storage,
            control_rx,
            config.retry_delay,
        )));

        let config_storage = storage.clone();
        handles.push(tokio::spawn(config_writer(
            config_storage,
            config_rx,
            config.retry_delay,
        )));

        let downstream_storage = storage.clone();
        handles.push(tokio::spawn(downstream_writer(
            downstream_storage,
            downstream_rx,
            config.downstream_batch_size,
            config.flush_interval,
            config.retry_delay,
        )));

        let upstream_storage = storage.clone();
        handles.push(tokio::spawn(upstream_writer(
            upstream_storage,
            upstream_rx,
            config.upstream_batch_size,
            config.flush_interval,
            config.retry_delay,
        )));

        Self {
            control_tx,
            config_tx,
            downstream_tx,
            upstream_tx,
            _handles: handles,
        }
    }
}

async fn control_writer(
    storage: TrafficStorage,
    mut rx: mpsc::Receiver<ControlEvent>,
    retry_delay: Duration,
) {
    while let Some(event) = rx.recv().await {
        match event {
            ControlEvent::UpsertDisallow(disallow) => {
                retry_write(
                    "storage control",
                    || upsert_disallow(&storage, disallow.clone()),
                    retry_delay,
                )
                .await;
            }
        }
    }
}

async fn config_writer(
    storage: TrafficStorage,
    mut rx: mpsc::Receiver<ConfigEvent>,
    retry_delay: Duration,
) {
    while let Some(event) = rx.recv().await {
        match event {
            ConfigEvent::UpsertProvider(provider) => {
                retry_write(
                    "storage config",
                    || upsert_provider(&storage, provider.clone()),
                    retry_delay,
                )
                .await;
            }
            ConfigEvent::UpsertCredential(credential) => {
                retry_write(
                    "storage config",
                    || upsert_credential(&storage, credential.clone()),
                    retry_delay,
                )
                .await;
            }
            ConfigEvent::UpsertUser(user) => {
                retry_write(
                    "storage config",
                    || upsert_user(&storage, user.clone()),
                    retry_delay,
                )
                .await;
            }
            ConfigEvent::UpsertApiKey(api_key) => {
                retry_write(
                    "storage config",
                    || upsert_api_key(&storage, api_key.clone()),
                    retry_delay,
                )
                .await;
            }
            ConfigEvent::UpsertGlobalConfig(config) => {
                retry_write(
                    "storage config",
                    || upsert_global_config(&storage, config.clone()),
                    retry_delay,
                )
                .await;
            }
            ConfigEvent::DeleteProvider(provider_id) => {
                retry_write(
                    "storage config",
                    || delete_provider(&storage, provider_id),
                    retry_delay,
                )
                .await;
            }
            ConfigEvent::DeleteCredential(credential_id) => {
                retry_write(
                    "storage config",
                    || delete_credential(&storage, credential_id),
                    retry_delay,
                )
                .await;
            }
            ConfigEvent::DeleteUser(user_id) => {
                retry_write(
                    "storage config",
                    || delete_user(&storage, user_id),
                    retry_delay,
                )
                .await;
            }
            ConfigEvent::DeleteApiKey(key_id) => {
                retry_write(
                    "storage config",
                    || delete_api_key(&storage, key_id),
                    retry_delay,
                )
                .await;
            }
            ConfigEvent::DeleteGlobalConfig(config_id) => {
                retry_write(
                    "storage config",
                    || delete_global_config(&storage, config_id),
                    retry_delay,
                )
                .await;
            }
            ConfigEvent::ReplaceSnapshot(snapshot) => {
                retry_write(
                    "storage config",
                    || replace_snapshot(&storage, snapshot.clone()),
                    retry_delay,
                )
                .await;
            }
        }
    }
}

async fn retry_write<F, Fut>(label: &'static str, mut f: F, retry_delay: Duration)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<(), DbErr>>,
{
    loop {
        match f().await {
            Ok(()) => break,
            Err(err) => {
                eprintln!("{label} write failed: {err}");
                tokio_time::sleep(retry_delay).await;
            }
        }
    }
}

async fn upsert_disallow(storage: &TrafficStorage, disallow: DisallowUpsert) -> Result<(), DbErr> {
    use entities::credential_disallow::Column;

    let active = entities::credential_disallow::ActiveModel {
        id: ActiveValue::NotSet,
        credential_id: ActiveValue::Set(disallow.credential_id),
        scope_kind: ActiveValue::Set(disallow.scope_kind),
        scope_value: ActiveValue::Set(disallow.scope_value),
        level: ActiveValue::Set(disallow.level),
        until_at: ActiveValue::Set(disallow.until_at),
        reason: ActiveValue::Set(disallow.reason),
        updated_at: ActiveValue::Set(disallow.updated_at),
        ..Default::default()
    };

    entities::CredentialDisallow::insert(active)
        .on_conflict(
            OnConflict::columns([Column::CredentialId, Column::ScopeKind, Column::ScopeValue])
                .update_columns([
                    Column::Level,
                    Column::UntilAt,
                    Column::Reason,
                    Column::UpdatedAt,
                ])
                .to_owned(),
        )
        .exec(storage.connection())
        .await?;

    Ok(())
}

async fn upsert_provider(storage: &TrafficStorage, provider: ProviderUpsert) -> Result<(), DbErr> {
    use entities::providers::Column;

    let active = entities::providers::ActiveModel {
        id: ActiveValue::Set(provider.id),
        name: ActiveValue::Set(provider.name),
        config_json: ActiveValue::Set(provider.config_json),
        enabled: ActiveValue::Set(provider.enabled),
        updated_at: ActiveValue::Set(provider.updated_at),
        ..Default::default()
    };

    entities::Providers::insert(active)
        .on_conflict(
            OnConflict::column(Column::Id)
                .update_columns([
                    Column::Name,
                    Column::ConfigJson,
                    Column::Enabled,
                    Column::UpdatedAt,
                ])
                .to_owned(),
        )
        .exec(storage.connection())
        .await?;

    Ok(())
}

async fn upsert_credential(
    storage: &TrafficStorage,
    credential: CredentialUpsert,
) -> Result<(), DbErr> {
    use entities::credentials::Column;

    let created_at = credential
        .created_at
        .unwrap_or_else(OffsetDateTime::now_utc);

    let active = entities::credentials::ActiveModel {
        id: ActiveValue::Set(credential.id),
        provider_id: ActiveValue::Set(credential.provider_id),
        name: ActiveValue::Set(credential.name),
        secret: ActiveValue::Set(credential.secret),
        meta_json: ActiveValue::Set(credential.meta_json),
        weight: ActiveValue::Set(credential.weight),
        enabled: ActiveValue::Set(credential.enabled),
        created_at: ActiveValue::Set(created_at),
        updated_at: ActiveValue::Set(credential.updated_at),
        ..Default::default()
    };

    entities::Credentials::insert(active)
        .on_conflict(
            OnConflict::column(Column::Id)
                .update_columns([
                    Column::ProviderId,
                    Column::Name,
                    Column::Secret,
                    Column::MetaJson,
                    Column::Weight,
                    Column::Enabled,
                    Column::UpdatedAt,
                ])
                .to_owned(),
        )
        .exec(storage.connection())
        .await?;

    Ok(())
}

async fn upsert_user(storage: &TrafficStorage, user: UserUpsert) -> Result<(), DbErr> {
    use entities::users::Column;

    let created_at = user.created_at.unwrap_or_else(OffsetDateTime::now_utc);

    let active = entities::users::ActiveModel {
        id: ActiveValue::Set(user.id),
        name: ActiveValue::Set(user.name),
        created_at: ActiveValue::Set(created_at),
        updated_at: ActiveValue::Set(user.updated_at),
        ..Default::default()
    };

    entities::Users::insert(active)
        .on_conflict(
            OnConflict::column(Column::Id)
                .update_columns([Column::Name, Column::UpdatedAt])
                .to_owned(),
        )
        .exec(storage.connection())
        .await?;

    Ok(())
}

async fn upsert_api_key(storage: &TrafficStorage, api_key: ApiKeyUpsert) -> Result<(), DbErr> {
    use entities::api_keys::Column;

    let created_at = api_key
        .created_at
        .unwrap_or_else(OffsetDateTime::now_utc);

    let active = entities::api_keys::ActiveModel {
        id: ActiveValue::Set(api_key.id),
        user_id: ActiveValue::Set(api_key.user_id),
        key_value: ActiveValue::Set(api_key.key_value),
        label: ActiveValue::Set(api_key.label),
        enabled: ActiveValue::Set(api_key.enabled),
        created_at: ActiveValue::Set(created_at),
        last_used_at: ActiveValue::Set(api_key.last_used_at),
        ..Default::default()
    };

    entities::ApiKeys::insert(active)
        .on_conflict(
            OnConflict::column(Column::Id)
                .update_columns([
                    Column::UserId,
                    Column::KeyValue,
                    Column::Label,
                    Column::Enabled,
                    Column::LastUsedAt,
                ])
                .to_owned(),
        )
        .exec(storage.connection())
        .await?;

    Ok(())
}

async fn upsert_global_config(
    storage: &TrafficStorage,
    config: GlobalConfigUpsert,
) -> Result<(), DbErr> {
    use entities::global_config::Column;

    let active = entities::global_config::ActiveModel {
        id: ActiveValue::Set(config.id),
        config_json: ActiveValue::Set(config.config_json),
        updated_at: ActiveValue::Set(config.updated_at),
        ..Default::default()
    };

    entities::GlobalConfig::insert(active)
        .on_conflict(
            OnConflict::column(Column::Id)
                .update_columns([Column::ConfigJson, Column::UpdatedAt])
                .to_owned(),
        )
        .exec(storage.connection())
        .await?;

    Ok(())
}

async fn delete_provider(storage: &TrafficStorage, provider_id: i64) -> Result<(), DbErr> {
    entities::Providers::delete_by_id(provider_id)
        .exec(storage.connection())
        .await?;
    Ok(())
}

async fn delete_credential(storage: &TrafficStorage, credential_id: i64) -> Result<(), DbErr> {
    entities::Credentials::delete_by_id(credential_id)
        .exec(storage.connection())
        .await?;
    Ok(())
}

async fn delete_user(storage: &TrafficStorage, user_id: i64) -> Result<(), DbErr> {
    entities::Users::delete_by_id(user_id)
        .exec(storage.connection())
        .await?;
    Ok(())
}

async fn delete_api_key(storage: &TrafficStorage, key_id: i64) -> Result<(), DbErr> {
    entities::ApiKeys::delete_by_id(key_id)
        .exec(storage.connection())
        .await?;
    Ok(())
}

async fn delete_global_config(storage: &TrafficStorage, config_id: i64) -> Result<(), DbErr> {
    entities::GlobalConfig::delete_by_id(config_id)
        .exec(storage.connection())
        .await?;
    Ok(())
}

async fn replace_snapshot(
    storage: &TrafficStorage,
    snapshot: ConfigSnapshot,
) -> Result<(), DbErr> {
    let ConfigSnapshot {
        providers,
        credentials,
        users,
        api_keys,
        global_config,
    } = snapshot;

    let result = storage
        .connection()
        .transaction(move |txn| {
            Box::pin(async move {
                entities::ApiKeys::delete_many().exec(txn).await?;
                entities::CredentialDisallow::delete_many().exec(txn).await?;
                entities::Credentials::delete_many().exec(txn).await?;
                entities::Providers::delete_many().exec(txn).await?;
                entities::Users::delete_many().exec(txn).await?;
                entities::GlobalConfig::delete_many().exec(txn).await?;

                if !providers.is_empty() {
                    let models = providers.into_iter().map(|provider| {
                        entities::providers::ActiveModel {
                            id: ActiveValue::Set(provider.id),
                            name: ActiveValue::Set(provider.name),
                            config_json: ActiveValue::Set(provider.config_json),
                            enabled: ActiveValue::Set(provider.enabled),
                            updated_at: ActiveValue::Set(provider.updated_at),
                            ..Default::default()
                        }
                    });
                    entities::Providers::insert_many(models).exec(txn).await?;
                }

                if !users.is_empty() {
                    let models = users.into_iter().map(|user| {
                        let created_at =
                            user.created_at.unwrap_or_else(OffsetDateTime::now_utc);
                        entities::users::ActiveModel {
                            id: ActiveValue::Set(user.id),
                            name: ActiveValue::Set(user.name),
                            created_at: ActiveValue::Set(created_at),
                            updated_at: ActiveValue::Set(user.updated_at),
                            ..Default::default()
                        }
                    });
                    entities::Users::insert_many(models).exec(txn).await?;
                }

                if !credentials.is_empty() {
                    let models = credentials.into_iter().map(|credential| {
                        let created_at =
                            credential.created_at.unwrap_or_else(OffsetDateTime::now_utc);
                        entities::credentials::ActiveModel {
                            id: ActiveValue::Set(credential.id),
                            provider_id: ActiveValue::Set(credential.provider_id),
                            name: ActiveValue::Set(credential.name),
                            secret: ActiveValue::Set(credential.secret),
                            meta_json: ActiveValue::Set(credential.meta_json),
                            weight: ActiveValue::Set(credential.weight),
                            enabled: ActiveValue::Set(credential.enabled),
                            created_at: ActiveValue::Set(created_at),
                            updated_at: ActiveValue::Set(credential.updated_at),
                            ..Default::default()
                        }
                    });
                    entities::Credentials::insert_many(models).exec(txn).await?;
                }

                if !api_keys.is_empty() {
                    let models = api_keys.into_iter().map(|api_key| {
                        let created_at =
                            api_key.created_at.unwrap_or_else(OffsetDateTime::now_utc);
                        entities::api_keys::ActiveModel {
                            id: ActiveValue::Set(api_key.id),
                            user_id: ActiveValue::Set(api_key.user_id),
                            key_value: ActiveValue::Set(api_key.key_value),
                            label: ActiveValue::Set(api_key.label),
                            enabled: ActiveValue::Set(api_key.enabled),
                            created_at: ActiveValue::Set(created_at),
                            last_used_at: ActiveValue::Set(api_key.last_used_at),
                            ..Default::default()
                        }
                    });
                    entities::ApiKeys::insert_many(models).exec(txn).await?;
                }

                if !global_config.is_empty() {
                    let models = global_config.into_iter().map(|config| {
                        entities::global_config::ActiveModel {
                            id: ActiveValue::Set(config.id),
                            config_json: ActiveValue::Set(config.config_json),
                            updated_at: ActiveValue::Set(config.updated_at),
                            ..Default::default()
                        }
                    });
                    entities::GlobalConfig::insert_many(models)
                        .exec(txn)
                        .await?;
                }

                Ok(())
            })
        })
        .await;

    match result {
        Ok(()) => Ok(()),
        Err(sea_orm::TransactionError::Connection(err)) => Err(err),
        Err(sea_orm::TransactionError::Transaction(err)) => Err(err),
    }
}

async fn downstream_writer(
    storage: TrafficStorage,
    mut rx: mpsc::Receiver<DownstreamTrafficEvent>,
    batch_size: usize,
    flush_interval: Duration,
    retry_delay: Duration,
) {
    let mut buffer = Vec::with_capacity(batch_size);
    let mut ticker = tokio_time::interval(flush_interval);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            Some(event) = rx.recv() => {
                buffer.push(event);
                if buffer.len() >= batch_size {
                    flush_downstream(&storage, &mut buffer, retry_delay).await;
                }
            }
            _ = ticker.tick() => {
                if !buffer.is_empty() {
                    flush_downstream(&storage, &mut buffer, retry_delay).await;
                }
            }
            else => {
                if !buffer.is_empty() {
                    flush_downstream(&storage, &mut buffer, retry_delay).await;
                }
                break;
            }
        }
    }
}

async fn upstream_writer(
    storage: TrafficStorage,
    mut rx: mpsc::Receiver<UpstreamTrafficEvent>,
    batch_size: usize,
    flush_interval: Duration,
    retry_delay: Duration,
) {
    let mut buffer = Vec::with_capacity(batch_size);
    let mut ticker = tokio_time::interval(flush_interval);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            Some(event) = rx.recv() => {
                buffer.push(event);
                if buffer.len() >= batch_size {
                    flush_upstream(&storage, &mut buffer, retry_delay).await;
                }
            }
            _ = ticker.tick() => {
                if !buffer.is_empty() {
                    flush_upstream(&storage, &mut buffer, retry_delay).await;
                }
            }
            else => {
                if !buffer.is_empty() {
                    flush_upstream(&storage, &mut buffer, retry_delay).await;
                }
                break;
            }
        }
    }
}


async fn flush_downstream(
    storage: &TrafficStorage,
    buffer: &mut Vec<DownstreamTrafficEvent>,
    retry_delay: Duration,
) {
    let mut batch = Vec::new();
    std::mem::swap(buffer, &mut batch);

    loop {
        let now = OffsetDateTime::now_utc();
        let models = batch
            .iter()
            .cloned()
            .map(|event| {
                let mut active: entities::downstream_traffic::ActiveModel = event.into();
                active.created_at = ActiveValue::Set(now);
                active
            });

        match entities::DownstreamTraffic::insert_many(models)
            .exec(storage.connection())
            .await
        {
            Ok(_) => break,
            Err(err) => {
                eprintln!("downstream traffic write failed: {err}");
                tokio_time::sleep(retry_delay).await;
            }
        }
    }
}

async fn flush_upstream(
    storage: &TrafficStorage,
    buffer: &mut Vec<UpstreamTrafficEvent>,
    retry_delay: Duration,
) {
    let mut batch = Vec::new();
    std::mem::swap(buffer, &mut batch);

    loop {
        let now = OffsetDateTime::now_utc();
        let models = batch
            .iter()
            .cloned()
            .map(|event| {
                let mut active: entities::upstream_traffic::ActiveModel = event.into();
                active.created_at = ActiveValue::Set(now);
                active
            });

        match entities::UpstreamTraffic::insert_many(models)
            .exec(storage.connection())
            .await
        {
            Ok(_) => break,
            Err(err) => {
                eprintln!("upstream traffic write failed: {err}");
                tokio_time::sleep(retry_delay).await;
            }
        }
    }
}
