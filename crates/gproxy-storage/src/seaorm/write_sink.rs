use sea_orm::sea_query::OnConflict;
use sea_orm::*;
use time::OffsetDateTime;

use gproxy_core::api_key_digest;

use crate::seaorm::SeaOrmStorage;
use crate::seaorm::entities::*;
use crate::write::*;

const UPSERT_CHUNK_SIZE: usize = 256;

impl SeaOrmStorage {
    pub async fn apply_write_event(&self, event: StorageWriteEvent) -> Result<(), DbErr> {
        let mut batch = StorageWriteBatch::default();
        batch.apply(event);
        self.apply_write_batch(batch).await
    }

    pub async fn apply_write_batch(&self, batch: StorageWriteBatch) -> Result<(), DbErr> {
        self.apply_batch(batch).await
    }

    async fn apply_batch(&self, batch: StorageWriteBatch) -> Result<(), DbErr> {
        if batch.is_empty() {
            return Ok(());
        }
        let txn = self.db.begin().await?;

        // --- Batch deletes (dependency order, single query per table) ---
        if !batch.credential_statuses_delete.is_empty() {
            credential_statuses::Entity::delete_many()
                .filter(credential_statuses::Column::Id.is_in(batch.credential_statuses_delete))
                .exec(&txn)
                .await?;
        }
        if !batch.credentials_delete.is_empty() {
            credentials::Entity::delete_many()
                .filter(credentials::Column::Id.is_in(batch.credentials_delete))
                .exec(&txn)
                .await?;
        }
        if !batch.providers_delete.is_empty() {
            providers::Entity::delete_many()
                .filter(providers::Column::Id.is_in(batch.providers_delete))
                .exec(&txn)
                .await?;
        }
        if !batch.user_keys_delete.is_empty() {
            user_keys::Entity::delete_many()
                .filter(user_keys::Column::Id.is_in(batch.user_keys_delete))
                .exec(&txn)
                .await?;
        }
        if !batch.users_delete.is_empty() {
            users::Entity::delete_many()
                .filter(users::Column::Id.is_in(batch.users_delete))
                .exec(&txn)
                .await?;
        }
        if !batch.models_delete.is_empty() {
            models::Entity::delete_many()
                .filter(models::Column::Id.is_in(batch.models_delete))
                .exec(&txn)
                .await?;
        }
        if !batch.user_model_permissions_delete.is_empty() {
            user_model_permissions::Entity::delete_many()
                .filter(
                    user_model_permissions::Column::Id.is_in(batch.user_model_permissions_delete),
                )
                .exec(&txn)
                .await?;
        }
        if !batch.user_file_permissions_delete.is_empty() {
            user_file_permissions::Entity::delete_many()
                .filter(user_file_permissions::Column::Id.is_in(batch.user_file_permissions_delete))
                .exec(&txn)
                .await?;
        }
        if !batch.user_rate_limits_delete.is_empty() {
            user_rate_limits::Entity::delete_many()
                .filter(user_rate_limits::Column::Id.is_in(batch.user_rate_limits_delete))
                .exec(&txn)
                .await?;
        }

        // --- Upserts ---

        // Global settings
        if let Some(gs) = batch.global_settings {
            let now = OffsetDateTime::now_utc();
            let model = global_settings::ActiveModel {
                id: Set(1),
                host: Set(gs.host),
                port: Set(gs.port as i32),
                proxy: Set(gs.proxy),
                spoof_emulation: Set(Some(gs.spoof_emulation)),
                dsn: Set(gs.dsn),
                data_dir: Set(gs.data_dir),
                enable_usage: Set(gs.enable_usage),
                enable_upstream_log: Set(gs.enable_upstream_log),
                enable_upstream_log_body: Set(gs.enable_upstream_log_body),
                enable_downstream_log: Set(gs.enable_downstream_log),
                enable_downstream_log_body: Set(gs.enable_downstream_log_body),
                updated_at: Set(now),
            };
            global_settings::Entity::insert(model)
                .on_conflict(
                    OnConflict::column(global_settings::Column::Id)
                        .update_columns([
                            global_settings::Column::Host,
                            global_settings::Column::Port,
                            global_settings::Column::Proxy,
                            global_settings::Column::SpoofEmulation,
                            global_settings::Column::Dsn,
                            global_settings::Column::DataDir,
                            global_settings::Column::EnableUsage,
                            global_settings::Column::EnableUpstreamLog,
                            global_settings::Column::EnableUpstreamLogBody,
                            global_settings::Column::EnableDownstreamLog,
                            global_settings::Column::EnableDownstreamLogBody,
                            global_settings::Column::UpdatedAt,
                        ])
                        .to_owned(),
                )
                .exec(&txn)
                .await?;
        }

        // Providers
        for chunk in batch
            .providers_upsert
            .values()
            .collect::<Vec<_>>()
            .chunks(UPSERT_CHUNK_SIZE)
        {
            let models: Vec<providers::ActiveModel> = chunk
                .iter()
                .map(|p| {
                    let settings = serde_json::from_str(&p.settings_json).unwrap_or_default();
                    let dispatch = serde_json::from_str(&p.dispatch_json).unwrap_or_default();
                    let now = OffsetDateTime::now_utc();
                    providers::ActiveModel {
                        id: Set(p.id),
                        name: Set(p.name.clone()),
                        channel: Set(p.channel.clone()),
                        settings_json: Set(settings),
                        dispatch_json: Set(dispatch),
                        created_at: Set(now),
                        updated_at: Set(now),
                    }
                })
                .collect();
            providers::Entity::insert_many(models)
                .on_conflict(
                    OnConflict::column(providers::Column::Id)
                        .update_columns([
                            providers::Column::Name,
                            providers::Column::Channel,
                            providers::Column::SettingsJson,
                            providers::Column::DispatchJson,
                            providers::Column::UpdatedAt,
                        ])
                        .to_owned(),
                )
                .exec(&txn)
                .await?;
        }

        // Credentials
        for chunk in batch
            .credentials_upsert
            .values()
            .collect::<Vec<_>>()
            .chunks(UPSERT_CHUNK_SIZE)
        {
            let models: Vec<credentials::ActiveModel> = chunk
                .iter()
                .map(|c| {
                    let secret: serde_json::Value =
                        serde_json::from_str(&c.secret_json).unwrap_or_default();
                    let encrypted = self.encrypt_json_for_write(&secret);
                    let now = OffsetDateTime::now_utc();
                    credentials::ActiveModel {
                        id: Set(c.id),
                        provider_id: Set(c.provider_id),
                        name: Set(c.name.clone()),
                        kind: Set(c.kind.clone()),
                        secret_json: Set(encrypted),
                        enabled: Set(c.enabled),
                        created_at: Set(now),
                        updated_at: Set(now),
                    }
                })
                .collect();
            credentials::Entity::insert_many(models)
                .on_conflict(
                    OnConflict::column(credentials::Column::Id)
                        .update_columns([
                            credentials::Column::ProviderId,
                            credentials::Column::Name,
                            credentials::Column::Kind,
                            credentials::Column::SecretJson,
                            credentials::Column::Enabled,
                            credentials::Column::UpdatedAt,
                        ])
                        .to_owned(),
                )
                .exec(&txn)
                .await?;
        }

        // Credential statuses
        for chunk in batch
            .credential_statuses_upsert
            .values()
            .collect::<Vec<_>>()
            .chunks(UPSERT_CHUNK_SIZE)
        {
            let models: Vec<credential_statuses::ActiveModel> = chunk
                .iter()
                .map(|s| {
                    let health_json = s
                        .health_json
                        .as_deref()
                        .and_then(|j| serde_json::from_str(j).ok());
                    let checked_at = s.checked_at_unix_ms.map(unix_ms_to_datetime);
                    let now = OffsetDateTime::now_utc();
                    credential_statuses::ActiveModel {
                        id: s.id.map(Set).unwrap_or(NotSet),
                        credential_id: Set(s.credential_id),
                        channel: Set(s.channel.clone()),
                        health_kind: Set(s.health_kind.clone()),
                        health_json: Set(health_json),
                        checked_at: Set(checked_at),
                        last_error: Set(s.last_error.clone()),
                        updated_at: Set(now),
                    }
                })
                .collect();
            credential_statuses::Entity::insert_many(models)
                .on_conflict(
                    OnConflict::columns([
                        credential_statuses::Column::CredentialId,
                        credential_statuses::Column::Channel,
                    ])
                    .update_columns([
                        credential_statuses::Column::HealthKind,
                        credential_statuses::Column::HealthJson,
                        credential_statuses::Column::CheckedAt,
                        credential_statuses::Column::LastError,
                        credential_statuses::Column::UpdatedAt,
                    ])
                    .to_owned(),
                )
                .exec(&txn)
                .await?;
        }

        // Users
        for chunk in batch
            .users_upsert
            .values()
            .collect::<Vec<_>>()
            .chunks(UPSERT_CHUNK_SIZE)
        {
            let models: Vec<users::ActiveModel> = chunk
                .iter()
                .map(|u| {
                    let password = self.encrypt_string_for_write(&u.password);
                    let now = OffsetDateTime::now_utc();
                    users::ActiveModel {
                        id: Set(u.id),
                        name: Set(u.name.clone()),
                        password: Set(Some(password)),
                        enabled: Set(u.enabled),
                        is_admin: Set(u.is_admin),
                        created_at: Set(now),
                        updated_at: Set(now),
                    }
                })
                .collect();
            users::Entity::insert_many(models)
                .on_conflict(
                    OnConflict::column(users::Column::Id)
                        .update_columns([
                            users::Column::Name,
                            users::Column::Password,
                            users::Column::Enabled,
                            users::Column::IsAdmin,
                            users::Column::UpdatedAt,
                        ])
                        .to_owned(),
                )
                .exec(&txn)
                .await?;
        }

        // User keys
        for chunk in batch
            .user_keys_upsert
            .values()
            .collect::<Vec<_>>()
            .chunks(UPSERT_CHUNK_SIZE)
        {
            let models: Vec<user_keys::ActiveModel> = chunk
                .iter()
                .map(|k| {
                    let api_key = self.encrypt_string_for_write(&k.api_key);
                    let api_key_digest = api_key_digest(&k.api_key);
                    let now = OffsetDateTime::now_utc();
                    user_keys::ActiveModel {
                        id: Set(k.id),
                        user_id: Set(k.user_id),
                        api_key_ciphertext: Set(api_key),
                        api_key_digest: Set(api_key_digest),
                        label: Set(k.label.clone()),
                        enabled: Set(k.enabled),
                        created_at: Set(now),
                        updated_at: Set(now),
                    }
                })
                .collect();
            user_keys::Entity::insert_many(models)
                .on_conflict(
                    OnConflict::column(user_keys::Column::Id)
                        .update_columns([
                            user_keys::Column::UserId,
                            user_keys::Column::ApiKeyCiphertext,
                            user_keys::Column::ApiKeyDigest,
                            user_keys::Column::Label,
                            user_keys::Column::Enabled,
                            user_keys::Column::UpdatedAt,
                        ])
                        .to_owned(),
                )
                .exec(&txn)
                .await?;
        }

        // User credential files
        for chunk in batch
            .user_credential_files_upsert
            .values()
            .collect::<Vec<_>>()
            .chunks(UPSERT_CHUNK_SIZE)
        {
            let models: Vec<user_credential_files::ActiveModel> = chunk
                .iter()
                .map(|f| {
                    let created_at = unix_ms_to_datetime(f.created_at_unix_ms);
                    let updated_at = unix_ms_to_datetime(f.updated_at_unix_ms);
                    let deleted_at = f.deleted_at_unix_ms.map(unix_ms_to_datetime);
                    user_credential_files::ActiveModel {
                        id: NotSet,
                        user_id: Set(f.user_id),
                        user_key_id: Set(f.user_key_id),
                        provider_id: Set(f.provider_id),
                        credential_id: Set(f.credential_id),
                        file_id: Set(f.file_id.clone()),
                        active: Set(f.active),
                        created_at: Set(created_at),
                        updated_at: Set(updated_at),
                        deleted_at: Set(deleted_at),
                    }
                })
                .collect();
            user_credential_files::Entity::insert_many(models)
                .on_conflict(
                    OnConflict::columns([
                        user_credential_files::Column::UserId,
                        user_credential_files::Column::ProviderId,
                        user_credential_files::Column::FileId,
                    ])
                    .update_columns([
                        user_credential_files::Column::UserKeyId,
                        user_credential_files::Column::CredentialId,
                        user_credential_files::Column::Active,
                        user_credential_files::Column::UpdatedAt,
                        user_credential_files::Column::DeletedAt,
                    ])
                    .to_owned(),
                )
                .exec(&txn)
                .await?;
        }

        // Claude file metadata
        for chunk in batch
            .claude_files_upsert
            .values()
            .collect::<Vec<_>>()
            .chunks(UPSERT_CHUNK_SIZE)
        {
            let models: Vec<claude_files::ActiveModel> = chunk
                .iter()
                .map(|f| {
                    let raw_json = serde_json::from_str(&f.raw_json).unwrap_or_default();
                    let now = unix_ms_to_datetime(f.updated_at_unix_ms);
                    claude_files::ActiveModel {
                        id: NotSet,
                        provider_id: Set(f.provider_id),
                        file_id: Set(f.file_id.clone()),
                        file_created_at: Set(f.file_created_at.clone()),
                        filename: Set(f.filename.clone()),
                        mime_type: Set(f.mime_type.clone()),
                        size_bytes: Set(f.size_bytes),
                        downloadable: Set(f.downloadable),
                        raw_json: Set(raw_json),
                        created_at: Set(now),
                        updated_at: Set(now),
                    }
                })
                .collect();
            claude_files::Entity::insert_many(models)
                .on_conflict(
                    OnConflict::columns([
                        claude_files::Column::ProviderId,
                        claude_files::Column::FileId,
                    ])
                    .update_columns([
                        claude_files::Column::FileCreatedAt,
                        claude_files::Column::Filename,
                        claude_files::Column::MimeType,
                        claude_files::Column::SizeBytes,
                        claude_files::Column::Downloadable,
                        claude_files::Column::RawJson,
                        claude_files::Column::UpdatedAt,
                    ])
                    .to_owned(),
                )
                .exec(&txn)
                .await?;
        }

        // Models
        for chunk in batch
            .models_upsert
            .values()
            .collect::<Vec<_>>()
            .chunks(UPSERT_CHUNK_SIZE)
        {
            let items: Vec<models::ActiveModel> = chunk
                .iter()
                .map(|m| {
                    let now = OffsetDateTime::now_utc();
                    models::ActiveModel {
                        id: Set(m.id),
                        provider_id: Set(m.provider_id),
                        model_id: Set(m.model_id.clone()),
                        display_name: Set(m.display_name.clone()),
                        enabled: Set(m.enabled),
                        price_each_call: Set(m.price_each_call),
                        price_tiers_json: Set(m.price_tiers_json.clone()),
                        pricing_json: Set(m.pricing_json.clone()),
                        alias_of: Set(m.alias_of),
                        created_at: Set(now),
                        updated_at: Set(now),
                    }
                })
                .collect();
            models::Entity::insert_many(items)
                .on_conflict(
                    OnConflict::column(models::Column::Id)
                        .update_columns([
                            models::Column::ProviderId,
                            models::Column::ModelId,
                            models::Column::DisplayName,
                            models::Column::Enabled,
                            models::Column::PriceEachCall,
                            models::Column::PriceTiersJson,
                            models::Column::AliasOf,
                            models::Column::UpdatedAt,
                        ])
                        .to_owned(),
                )
                .exec(&txn)
                .await?;
        }

        // User model permissions
        for chunk in batch
            .user_model_permissions_upsert
            .values()
            .collect::<Vec<_>>()
            .chunks(UPSERT_CHUNK_SIZE)
        {
            let items: Vec<user_model_permissions::ActiveModel> = chunk
                .iter()
                .map(|p| {
                    let now = OffsetDateTime::now_utc();
                    user_model_permissions::ActiveModel {
                        id: Set(p.id),
                        user_id: Set(p.user_id),
                        provider_id: Set(p.provider_id),
                        model_pattern: Set(p.model_pattern.clone()),
                        created_at: Set(now),
                    }
                })
                .collect();
            user_model_permissions::Entity::insert_many(items)
                .on_conflict(
                    OnConflict::column(user_model_permissions::Column::Id)
                        .update_columns([
                            user_model_permissions::Column::UserId,
                            user_model_permissions::Column::ProviderId,
                            user_model_permissions::Column::ModelPattern,
                        ])
                        .to_owned(),
                )
                .exec(&txn)
                .await?;
        }

        // User rate limits
        for chunk in batch
            .user_rate_limits_upsert
            .values()
            .collect::<Vec<_>>()
            .chunks(UPSERT_CHUNK_SIZE)
        {
            let items: Vec<user_rate_limits::ActiveModel> = chunk
                .iter()
                .map(|r| {
                    let now = OffsetDateTime::now_utc();
                    user_rate_limits::ActiveModel {
                        id: Set(r.id),
                        user_id: Set(r.user_id),
                        model_pattern: Set(r.model_pattern.clone()),
                        rpm: Set(r.rpm),
                        rpd: Set(r.rpd),
                        total_tokens: Set(r.total_tokens),
                        created_at: Set(now),
                        updated_at: Set(now),
                    }
                })
                .collect();
            user_rate_limits::Entity::insert_many(items)
                .on_conflict(
                    OnConflict::column(user_rate_limits::Column::Id)
                        .update_columns([
                            user_rate_limits::Column::UserId,
                            user_rate_limits::Column::ModelPattern,
                            user_rate_limits::Column::Rpm,
                            user_rate_limits::Column::Rpd,
                            user_rate_limits::Column::TotalTokens,
                            user_rate_limits::Column::UpdatedAt,
                        ])
                        .to_owned(),
                )
                .exec(&txn)
                .await?;
        }

        // User file permissions
        for chunk in batch
            .user_file_permissions_upsert
            .values()
            .collect::<Vec<_>>()
            .chunks(UPSERT_CHUNK_SIZE)
        {
            let items: Vec<user_file_permissions::ActiveModel> = chunk
                .iter()
                .map(|p| {
                    let now = OffsetDateTime::now_utc();
                    user_file_permissions::ActiveModel {
                        id: Set(p.id),
                        user_id: Set(p.user_id),
                        provider_id: Set(p.provider_id),
                        created_at: Set(now),
                    }
                })
                .collect();
            user_file_permissions::Entity::insert_many(items)
                .on_conflict(
                    OnConflict::column(user_file_permissions::Column::Id)
                        .update_columns([
                            user_file_permissions::Column::UserId,
                            user_file_permissions::Column::ProviderId,
                        ])
                        .to_owned(),
                )
                .exec(&txn)
                .await?;
        }

        // User quotas
        for q in batch.user_quotas_upsert.values() {
            let now = OffsetDateTime::now_utc();
            let model = user_token_usage::ActiveModel {
                id: Set(0),
                user_id: Set(q.user_id),
                quota: Set(q.quota),
                cost_used: Set(q.cost_used),
                updated_at: Set(now),
            };
            user_token_usage::Entity::insert(model)
                .on_conflict(
                    OnConflict::column(user_token_usage::Column::UserId)
                        .update_columns([
                            user_token_usage::Column::Quota,
                            user_token_usage::Column::CostUsed,
                            user_token_usage::Column::UpdatedAt,
                        ])
                        .to_owned(),
                )
                .exec(&txn)
                .await?;
        }

        // Downstream requests (insert only, no update)
        for chunk in batch.downstream_requests_upsert.chunks(UPSERT_CHUNK_SIZE) {
            let models: Vec<downstream_requests::ActiveModel> = chunk
                .iter()
                .map(|r| {
                    let headers: serde_json::Value =
                        serde_json::from_str(&r.request_headers_json).unwrap_or_default();
                    let resp_headers: serde_json::Value =
                        serde_json::from_str(&r.response_headers_json).unwrap_or_default();
                    downstream_requests::ActiveModel {
                        trace_id: Set(r.trace_id),
                        at: Set(unix_ms_to_datetime(r.at_unix_ms)),
                        internal: Set(r.internal),
                        user_id: Set(r.user_id),
                        user_key_id: Set(r.user_key_id),
                        request_method: Set(r.request_method.clone()),
                        request_headers_json: Set(headers),
                        request_path: Set(r.request_path.clone()),
                        request_query: Set(r.request_query.clone()),
                        request_body: Set(r.request_body.clone()),
                        response_status: Set(r.response_status),
                        response_headers_json: Set(resp_headers),
                        response_body: Set(r.response_body.clone()),
                        created_at: Set(OffsetDateTime::now_utc()),
                    }
                })
                .collect();
            downstream_requests::Entity::insert_many(models)
                .exec(&txn)
                .await?;
        }

        // Upstream requests (insert only)
        for chunk in batch.upstream_requests_upsert.chunks(UPSERT_CHUNK_SIZE) {
            let models: Vec<upstream_requests::ActiveModel> = chunk
                .iter()
                .map(|r| {
                    let headers: serde_json::Value =
                        serde_json::from_str(&r.request_headers_json).unwrap_or_default();
                    let resp_headers: serde_json::Value =
                        serde_json::from_str(&r.response_headers_json).unwrap_or_default();
                    upstream_requests::ActiveModel {
                        downstream_trace_id: Set(r.downstream_trace_id),
                        at: Set(unix_ms_to_datetime(r.at_unix_ms)),
                        internal: Set(r.internal),
                        provider_id: Set(r.provider_id),
                        credential_id: Set(r.credential_id),
                        request_method: Set(r.request_method.clone()),
                        request_headers_json: Set(headers),
                        request_url: Set(r.request_url.clone()),
                        request_body: Set(r.request_body.clone()),
                        response_status: Set(r.response_status),
                        response_headers_json: Set(resp_headers),
                        response_body: Set(r.response_body.clone()),
                        created_at: Set(OffsetDateTime::now_utc()),
                        ..Default::default()
                    }
                })
                .collect();
            upstream_requests::Entity::insert_many(models)
                .exec(&txn)
                .await?;
        }

        // Usages (insert only)
        for chunk in batch.usages_upsert.chunks(UPSERT_CHUNK_SIZE) {
            let models: Vec<usages::ActiveModel> = chunk
                .iter()
                .map(|u| usages::ActiveModel {
                    downstream_trace_id: Set(u.downstream_trace_id),
                    at: Set(unix_ms_to_datetime(u.at_unix_ms)),
                    provider_id: Set(u.provider_id),
                    credential_id: Set(u.credential_id),
                    user_id: Set(u.user_id),
                    user_key_id: Set(u.user_key_id),
                    operation: Set(u.operation.clone()),
                    protocol: Set(u.protocol.clone()),
                    model: Set(u.model.clone()),
                    input_tokens: Set(u.input_tokens),
                    output_tokens: Set(u.output_tokens),
                    cache_read_input_tokens: Set(u.cache_read_input_tokens),
                    cache_creation_input_tokens: Set(u.cache_creation_input_tokens),
                    cache_creation_input_tokens_5min: Set(u.cache_creation_input_tokens_5min),
                    cache_creation_input_tokens_1h: Set(u.cache_creation_input_tokens_1h),
                    cost: Set(u.cost),
                    created_at: Set(OffsetDateTime::now_utc()),
                    ..Default::default()
                })
                .collect();
            usages::Entity::insert_many(models).exec(&txn).await?;
        }

        txn.commit().await?;
        Ok(())
    }

    fn encrypt_string_for_write(&self, plaintext: &str) -> String {
        match &self.cipher {
            Some(cipher) => cipher
                .encrypt_string(plaintext)
                .unwrap_or_else(|_| plaintext.to_string()),
            None => plaintext.to_string(),
        }
    }

    fn encrypt_json_for_write(&self, value: &serde_json::Value) -> serde_json::Value {
        match &self.cipher {
            Some(cipher) => cipher.encrypt_json(value).unwrap_or_else(|_| value.clone()),
            None => value.clone(),
        }
    }
}

fn unix_ms_to_datetime(ms: i64) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp_nanos(ms as i128 * 1_000_000)
        .unwrap_or(OffsetDateTime::UNIX_EPOCH)
}
