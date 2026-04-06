use sea_orm::*;

use crate::query::*;
use crate::seaorm::SeaOrmStorage;
use crate::seaorm::entities::*;

/// Database query methods for SeaOrmStorage.
///
/// **Important**: These queries are primarily used during **bootstrap** to load
/// data into memory.  At runtime, the API layer should read from AppState's
/// in-memory caches (ArcSwap) for performance.  Only mutations (create/update/
/// delete) hit the database at runtime, and those go through the async write
/// worker.
impl SeaOrmStorage {
    pub async fn get_global_settings(&self) -> Result<Option<GlobalSettingsRow>, DbErr> {
        let row = global_settings::Entity::find().one(&self.db).await?;
        let Some(row) = row else {
            return Ok(None);
        };
        Ok(Some(GlobalSettingsRow {
            id: row.id,
            host: row.host,
            port: row.port,
            proxy: row.proxy,
            spoof_emulation: row.spoof_emulation,
            update_source: row.update_source,
            dsn: row.dsn,
            data_dir: row.data_dir,
            enable_usage: row.enable_usage,
            enable_upstream_log: row.enable_upstream_log,
            enable_upstream_log_body: row.enable_upstream_log_body,
            enable_downstream_log: row.enable_downstream_log,
            enable_downstream_log_body: row.enable_downstream_log_body,
            updated_at: row.updated_at,
        }))
    }

    pub async fn list_providers(
        &self,
        query: &ProviderQuery,
    ) -> Result<Vec<ProviderQueryRow>, DbErr> {
        let mut select = providers::Entity::find();
        if let Scope::Eq(ref v) = query.channel {
            select = select.filter(providers::Column::Channel.eq(v.clone()));
        } else if let Scope::In(ref v) = query.channel {
            select = select.filter(providers::Column::Channel.is_in(v.clone()));
        }
        if let Scope::Eq(ref v) = query.name {
            select = select.filter(providers::Column::Name.eq(v.clone()));
        } else if let Scope::In(ref v) = query.name {
            select = select.filter(providers::Column::Name.is_in(v.clone()));
        }
        if let Some(limit) = query.limit {
            select = select.limit(limit);
        }
        let rows = select.all(&self.db).await?;
        Ok(rows
            .into_iter()
            .map(|r| ProviderQueryRow {
                id: r.id,
                name: r.name,
                channel: r.channel,
                settings_json: r.settings_json,
                dispatch_json: r.dispatch_json,
                created_at: r.created_at,
                updated_at: r.updated_at,
            })
            .collect())
    }

    pub async fn list_credentials(
        &self,
        query: &CredentialQuery,
    ) -> Result<Vec<CredentialQueryRow>, DbErr> {
        let mut select = credentials::Entity::find();
        if let Scope::Eq(ref v) = query.id {
            select = select.filter(credentials::Column::Id.eq(*v));
        } else if let Scope::In(ref v) = query.id {
            select = select.filter(credentials::Column::Id.is_in(v.clone()));
        }
        if let Scope::Eq(ref v) = query.provider_id {
            select = select.filter(credentials::Column::ProviderId.eq(*v));
        } else if let Scope::In(ref v) = query.provider_id {
            select = select.filter(credentials::Column::ProviderId.is_in(v.clone()));
        }
        if let Scope::Eq(ref v) = query.kind {
            select = select.filter(credentials::Column::Kind.eq(v.clone()));
        }
        if let Scope::Eq(ref v) = query.enabled {
            select = select.filter(credentials::Column::Enabled.eq(*v));
        }
        if let Some(ref contains) = query.name_contains {
            select = select.filter(credentials::Column::Name.contains(contains));
        }
        if let Some(limit) = query.limit {
            select = select.limit(limit);
        }
        if let Some(offset) = query.offset {
            select = select.offset(offset);
        }
        let rows = select
            .order_by_asc(credentials::Column::Id)
            .all(&self.db)
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| {
                let secret = self.decrypt_json(r.secret_json);
                CredentialQueryRow {
                    id: r.id,
                    provider_id: r.provider_id,
                    name: r.name,
                    kind: r.kind,
                    secret_json: secret,
                    enabled: r.enabled,
                    created_at: r.created_at,
                    updated_at: r.updated_at,
                }
            })
            .collect())
    }

    pub async fn count_credentials(
        &self,
        query: &CredentialQuery,
    ) -> Result<CredentialQueryCount, DbErr> {
        let mut select = credentials::Entity::find();
        if let Scope::Eq(ref v) = query.provider_id {
            select = select.filter(credentials::Column::ProviderId.eq(*v));
        }
        if let Scope::Eq(ref v) = query.kind {
            select = select.filter(credentials::Column::Kind.eq(v.clone()));
        }
        if let Scope::Eq(ref v) = query.enabled {
            select = select.filter(credentials::Column::Enabled.eq(*v));
        }
        let count = select.count(&self.db).await?;
        Ok(CredentialQueryCount { count })
    }

    pub async fn list_credential_statuses(
        &self,
        query: &CredentialStatusQuery,
    ) -> Result<Vec<CredentialStatusQueryRow>, DbErr> {
        let mut select = credential_statuses::Entity::find();
        if let Scope::Eq(ref v) = query.id {
            select = select.filter(credential_statuses::Column::Id.eq(*v));
        }
        if let Scope::Eq(ref v) = query.credential_id {
            select = select.filter(credential_statuses::Column::CredentialId.eq(*v));
        }
        if let Scope::Eq(ref v) = query.channel {
            select = select.filter(credential_statuses::Column::Channel.eq(v.clone()));
        }
        if let Scope::Eq(ref v) = query.health_kind {
            select = select.filter(credential_statuses::Column::HealthKind.eq(v.clone()));
        }
        if let Some(limit) = query.limit {
            select = select.limit(limit);
        }
        if let Some(offset) = query.offset {
            select = select.offset(offset);
        }
        let rows = select.all(&self.db).await?;
        Ok(rows
            .into_iter()
            .map(|r| CredentialStatusQueryRow {
                id: r.id,
                credential_id: r.credential_id,
                channel: r.channel,
                health_kind: r.health_kind,
                health_json: r.health_json,
                checked_at: r.checked_at,
                last_error: r.last_error,
                updated_at: r.updated_at,
            })
            .collect())
    }

    pub async fn list_users(&self, query: &UserQuery) -> Result<Vec<UserQueryRow>, DbErr> {
        let mut select = users::Entity::find();
        if let Scope::Eq(ref v) = query.id {
            select = select.filter(users::Column::Id.eq(*v));
        }
        if let Scope::Eq(ref v) = query.name {
            select = select.filter(users::Column::Name.eq(v.clone()));
        }
        let rows = select.all(&self.db).await?;
        Ok(rows
            .into_iter()
            .map(|r| {
                let password = r
                    .password
                    .map(|p| self.decrypt_string(&p))
                    .unwrap_or_default();
                UserQueryRow {
                    id: r.id,
                    name: r.name,
                    password,
                    enabled: r.enabled,
                    is_admin: r.is_admin,
                }
            })
            .collect())
    }

    pub async fn list_user_keys(
        &self,
        query: &UserKeyQuery,
    ) -> Result<Vec<UserKeyQueryRow>, DbErr> {
        let mut select = user_keys::Entity::find();
        if let Scope::Eq(ref v) = query.id {
            select = select.filter(user_keys::Column::Id.eq(*v));
        }
        if let Scope::Eq(ref v) = query.user_id {
            select = select.filter(user_keys::Column::UserId.eq(*v));
        }
        if let Scope::Eq(ref v) = query.enabled {
            select = select.filter(user_keys::Column::Enabled.eq(*v));
        }
        let rows = select.all(&self.db).await?;
        Ok(rows
            .into_iter()
            .map(|r| {
                let api_key = self.decrypt_string(&r.api_key);
                UserKeyQueryRow {
                    id: r.id,
                    user_id: r.user_id,
                    api_key,
                    label: r.label,
                    enabled: r.enabled,
                }
            })
            .collect())
    }

    pub async fn list_user_keys_for_memory(&self) -> Result<Vec<UserKeyMemoryRow>, DbErr> {
        let rows = user_keys::Entity::find().all(&self.db).await?;
        Ok(rows
            .into_iter()
            .map(|r| {
                let api_key = self.decrypt_string(&r.api_key);
                UserKeyMemoryRow {
                    id: r.id,
                    user_id: r.user_id,
                    api_key,
                    label: r.label,
                    enabled: r.enabled,
                }
            })
            .collect())
    }

    // --- Encryption helpers ---

    fn decrypt_string(&self, raw: &str) -> String {
        match &self.cipher {
            Some(cipher) => cipher
                .decrypt_string(raw)
                .unwrap_or_else(|_| raw.to_string()),
            None => raw.to_string(),
        }
    }

    fn decrypt_json(&self, value: serde_json::Value) -> serde_json::Value {
        match &self.cipher {
            Some(cipher) => cipher.decrypt_json(value.clone()).unwrap_or(value),
            None => value,
        }
    }

    // -----------------------------------------------------------------------
    // Models
    // -----------------------------------------------------------------------

    pub async fn list_models(&self, query: &ModelQuery) -> Result<Vec<ModelQueryRow>, DbErr> {
        let mut select = models::Entity::find();
        if let Scope::Eq(ref v) = query.id {
            select = select.filter(models::Column::Id.eq(*v));
        }
        if let Scope::Eq(ref v) = query.provider_id {
            select = select.filter(models::Column::ProviderId.eq(*v));
        }
        if let Scope::Eq(ref v) = query.model_id {
            select = select.filter(models::Column::ModelId.eq(v.clone()));
        }
        if let Scope::Eq(ref v) = query.enabled {
            select = select.filter(models::Column::Enabled.eq(*v));
        }
        if let Some(limit) = query.limit {
            select = select.limit(limit);
        }
        if let Some(offset) = query.offset {
            select = select.offset(offset);
        }
        let rows = select.all(&self.db).await?;
        Ok(rows
            .into_iter()
            .map(|r| ModelQueryRow {
                id: r.id,
                provider_id: r.provider_id,
                model_id: r.model_id,
                display_name: r.display_name,
                enabled: r.enabled,
                price_each_call: r.price_each_call,
                price_tiers_json: r.price_tiers_json,
                created_at: r.created_at,
                updated_at: r.updated_at,
            })
            .collect())
    }

    // -----------------------------------------------------------------------
    // Model aliases
    // -----------------------------------------------------------------------

    pub async fn list_model_aliases(
        &self,
        query: &ModelAliasQuery,
    ) -> Result<Vec<ModelAliasQueryRow>, DbErr> {
        let mut select = model_aliases::Entity::find();
        if let Scope::Eq(ref v) = query.id {
            select = select.filter(model_aliases::Column::Id.eq(*v));
        }
        if let Scope::Eq(ref v) = query.alias {
            select = select.filter(model_aliases::Column::Alias.eq(v.clone()));
        }
        if let Scope::Eq(ref v) = query.provider_id {
            select = select.filter(model_aliases::Column::ProviderId.eq(*v));
        }
        if let Scope::Eq(ref v) = query.enabled {
            select = select.filter(model_aliases::Column::Enabled.eq(*v));
        }
        if let Some(limit) = query.limit {
            select = select.limit(limit);
        }
        let rows = select.all(&self.db).await?;
        Ok(rows
            .into_iter()
            .map(|r| ModelAliasQueryRow {
                id: r.id,
                alias: r.alias,
                provider_id: r.provider_id,
                model_id: r.model_id,
                enabled: r.enabled,
                created_at: r.created_at,
                updated_at: r.updated_at,
            })
            .collect())
    }

    // -----------------------------------------------------------------------
    // User model permissions
    // -----------------------------------------------------------------------

    pub async fn list_user_model_permissions(
        &self,
        query: &UserModelPermissionQuery,
    ) -> Result<Vec<UserModelPermissionQueryRow>, DbErr> {
        let mut select = user_model_permissions::Entity::find();
        if let Scope::Eq(ref v) = query.id {
            select = select.filter(user_model_permissions::Column::Id.eq(*v));
        }
        if let Scope::Eq(ref v) = query.user_id {
            select = select.filter(user_model_permissions::Column::UserId.eq(*v));
        }
        if let Scope::Eq(ref v) = query.provider_id {
            select = select.filter(user_model_permissions::Column::ProviderId.eq(*v));
        }
        if let Some(limit) = query.limit {
            select = select.limit(limit);
        }
        let rows = select.all(&self.db).await?;
        Ok(rows
            .into_iter()
            .map(|r| UserModelPermissionQueryRow {
                id: r.id,
                user_id: r.user_id,
                provider_id: r.provider_id,
                model_pattern: r.model_pattern,
                created_at: r.created_at,
            })
            .collect())
    }

    // -----------------------------------------------------------------------
    // User file permissions
    // -----------------------------------------------------------------------

    pub async fn list_user_file_permissions(
        &self,
        query: &UserFilePermissionQuery,
    ) -> Result<Vec<UserFilePermissionQueryRow>, DbErr> {
        let mut select = user_file_permissions::Entity::find();
        if let Scope::Eq(ref v) = query.id {
            select = select.filter(user_file_permissions::Column::Id.eq(*v));
        }
        if let Scope::Eq(ref v) = query.user_id {
            select = select.filter(user_file_permissions::Column::UserId.eq(*v));
        }
        if let Scope::Eq(ref v) = query.provider_id {
            select = select.filter(user_file_permissions::Column::ProviderId.eq(*v));
        }
        if let Some(limit) = query.limit {
            select = select.limit(limit);
        }
        let rows = select.all(&self.db).await?;
        Ok(rows
            .into_iter()
            .map(|r| UserFilePermissionQueryRow {
                id: r.id,
                user_id: r.user_id,
                provider_id: r.provider_id,
                created_at: r.created_at,
            })
            .collect())
    }

    // -----------------------------------------------------------------------
    // User rate limits
    // -----------------------------------------------------------------------

    pub async fn list_user_rate_limits(
        &self,
        query: &UserRateLimitQuery,
    ) -> Result<Vec<UserRateLimitQueryRow>, DbErr> {
        let mut select = user_rate_limits::Entity::find();
        if let Scope::Eq(ref v) = query.id {
            select = select.filter(user_rate_limits::Column::Id.eq(*v));
        }
        if let Scope::Eq(ref v) = query.user_id {
            select = select.filter(user_rate_limits::Column::UserId.eq(*v));
        }
        if let Some(limit) = query.limit {
            select = select.limit(limit);
        }
        let rows = select.all(&self.db).await?;
        Ok(rows
            .into_iter()
            .map(|r| UserRateLimitQueryRow {
                id: r.id,
                user_id: r.user_id,
                model_pattern: r.model_pattern,
                rpm: r.rpm,
                rpd: r.rpd,
                total_tokens: r.total_tokens,
                created_at: r.created_at,
                updated_at: r.updated_at,
            })
            .collect())
    }

    // -----------------------------------------------------------------------
    // User quotas
    // -----------------------------------------------------------------------

    pub async fn list_user_quotas(&self) -> Result<Vec<UserQuotaRow>, DbErr> {
        let rows = user_token_usage::Entity::find().all(&self.db).await?;
        Ok(rows
            .into_iter()
            .map(|r| UserQuotaRow {
                user_id: r.user_id,
                quota: r.quota,
                cost_used: r.cost_used,
                updated_at: r.updated_at,
            })
            .collect())
    }

    // -----------------------------------------------------------------------
    // Files
    // -----------------------------------------------------------------------

    pub async fn list_user_credential_files(
        &self,
        query: &UserCredentialFileQuery,
    ) -> Result<Vec<UserCredentialFileQueryRow>, DbErr> {
        let mut select = user_credential_files::Entity::find();
        if let Scope::Eq(ref v) = query.user_id {
            select = select.filter(user_credential_files::Column::UserId.eq(*v));
        } else if let Scope::In(ref v) = query.user_id {
            select = select.filter(user_credential_files::Column::UserId.is_in(v.clone()));
        }
        if let Scope::Eq(ref v) = query.user_key_id {
            select = select.filter(user_credential_files::Column::UserKeyId.eq(*v));
        } else if let Scope::In(ref v) = query.user_key_id {
            select = select.filter(user_credential_files::Column::UserKeyId.is_in(v.clone()));
        }
        if let Scope::Eq(ref v) = query.provider_id {
            select = select.filter(user_credential_files::Column::ProviderId.eq(*v));
        } else if let Scope::In(ref v) = query.provider_id {
            select = select.filter(user_credential_files::Column::ProviderId.is_in(v.clone()));
        }
        if let Scope::Eq(ref v) = query.credential_id {
            select = select.filter(user_credential_files::Column::CredentialId.eq(*v));
        } else if let Scope::In(ref v) = query.credential_id {
            select = select.filter(user_credential_files::Column::CredentialId.is_in(v.clone()));
        }
        if let Scope::Eq(ref v) = query.file_id {
            select = select.filter(user_credential_files::Column::FileId.eq(v.clone()));
        } else if let Scope::In(ref v) = query.file_id {
            select = select.filter(user_credential_files::Column::FileId.is_in(v.clone()));
        }
        if let Scope::Eq(ref v) = query.active {
            select = select.filter(user_credential_files::Column::Active.eq(*v));
        }
        if let Some(limit) = query.limit {
            select = select.limit(limit);
        }
        if let Some(offset) = query.offset {
            select = select.offset(offset);
        }
        let rows = select.all(&self.db).await?;
        Ok(rows
            .into_iter()
            .map(|r| UserCredentialFileQueryRow {
                id: r.id,
                user_id: r.user_id,
                user_key_id: r.user_key_id,
                provider_id: r.provider_id,
                credential_id: r.credential_id,
                file_id: r.file_id,
                active: r.active,
                created_at: r.created_at,
                updated_at: r.updated_at,
                deleted_at: r.deleted_at,
            })
            .collect())
    }

    pub async fn list_claude_files(
        &self,
        query: &ClaudeFileQuery,
    ) -> Result<Vec<ClaudeFileQueryRow>, DbErr> {
        let mut select = claude_files::Entity::find();
        if let Scope::Eq(ref v) = query.provider_id {
            select = select.filter(claude_files::Column::ProviderId.eq(*v));
        } else if let Scope::In(ref v) = query.provider_id {
            select = select.filter(claude_files::Column::ProviderId.is_in(v.clone()));
        }
        if let Scope::Eq(ref v) = query.file_id {
            select = select.filter(claude_files::Column::FileId.eq(v.clone()));
        } else if let Scope::In(ref v) = query.file_id {
            select = select.filter(claude_files::Column::FileId.is_in(v.clone()));
        }
        if let Some(limit) = query.limit {
            select = select.limit(limit);
        }
        if let Some(offset) = query.offset {
            select = select.offset(offset);
        }
        let rows = select.all(&self.db).await?;
        Ok(rows
            .into_iter()
            .map(|r| ClaudeFileQueryRow {
                id: r.id,
                provider_id: r.provider_id,
                file_id: r.file_id,
                file_created_at: r.file_created_at,
                filename: r.filename,
                mime_type: r.mime_type,
                size_bytes: r.size_bytes,
                downloadable: r.downloadable,
                raw_json: r.raw_json,
                created_at: r.created_at,
                updated_at: r.updated_at,
            })
            .collect())
    }
}
