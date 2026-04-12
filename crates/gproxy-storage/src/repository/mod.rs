mod seaorm_impl;

use std::future::Future;

use sea_orm::DbErr;

use crate::write::*;

/// Persists provider aggregate root writes.
pub trait ProviderRepository: Send + Sync {
    /// Creates or updates a provider record.
    fn upsert_provider(
        &self,
        provider: ProviderWrite,
    ) -> impl Future<Output = Result<(), DbErr>> + Send;

    /// Deletes a provider record by id.
    fn delete_provider(&self, id: i64) -> impl Future<Output = Result<(), DbErr>> + Send;
}

/// Persists credential aggregate root writes.
pub trait CredentialRepository: Send + Sync {
    /// Creates or updates a credential record.
    fn upsert_credential(
        &self,
        credential: CredentialWrite,
    ) -> impl Future<Output = Result<(), DbErr>> + Send;

    /// Deletes a credential record by id.
    fn delete_credential(&self, id: i64) -> impl Future<Output = Result<(), DbErr>> + Send;

    /// Creates or updates a credential status record.
    fn upsert_credential_status(
        &self,
        status: CredentialStatusWrite,
    ) -> impl Future<Output = Result<(), DbErr>> + Send;

    /// Deletes a credential status record by id.
    fn delete_credential_status(&self, id: i64) -> impl Future<Output = Result<(), DbErr>> + Send;
}

/// Persists user aggregate root writes.
pub trait UserRepository: Send + Sync {
    /// Creates or updates a user record.
    fn upsert_user(&self, user: UserWrite) -> impl Future<Output = Result<(), DbErr>> + Send;

    /// Deletes a user record by id.
    fn delete_user(&self, id: i64) -> impl Future<Output = Result<(), DbErr>> + Send;

    /// Creates or updates a user key record.
    fn upsert_user_key(
        &self,
        user_key: UserKeyWrite,
    ) -> impl Future<Output = Result<(), DbErr>> + Send;

    /// Deletes a user key record by id.
    fn delete_user_key(&self, id: i64) -> impl Future<Output = Result<(), DbErr>> + Send;
}

/// Persists model aggregate root writes.
pub trait ModelRepository: Send + Sync {
    /// Creates or updates a model record.
    fn upsert_model(&self, model: ModelWrite) -> impl Future<Output = Result<(), DbErr>> + Send;

    /// Deletes a model record by id.
    fn delete_model(&self, id: i64) -> impl Future<Output = Result<(), DbErr>> + Send;
}

/// Persists permission aggregate root writes.
pub trait PermissionRepository: Send + Sync {
    /// Creates or updates a user model permission record.
    fn upsert_user_permission(
        &self,
        permission: UserModelPermissionWrite,
    ) -> impl Future<Output = Result<(), DbErr>> + Send;

    /// Deletes a user model permission record by id.
    fn delete_user_permission(&self, id: i64) -> impl Future<Output = Result<(), DbErr>> + Send;

    /// Creates or updates a user file permission record.
    fn upsert_user_file_permission(
        &self,
        permission: UserFilePermissionWrite,
    ) -> impl Future<Output = Result<(), DbErr>> + Send;

    /// Deletes a user file permission record by id.
    fn delete_user_file_permission(
        &self,
        id: i64,
    ) -> impl Future<Output = Result<(), DbErr>> + Send;

    /// Creates or updates a user rate limit record.
    fn upsert_user_rate_limit(
        &self,
        rate_limit: UserRateLimitWrite,
    ) -> impl Future<Output = Result<(), DbErr>> + Send;

    /// Deletes a user rate limit record by id.
    fn delete_user_rate_limit(&self, id: i64) -> impl Future<Output = Result<(), DbErr>> + Send;
}

/// Persists quota aggregate root writes.
pub trait QuotaRepository: Send + Sync {
    /// Creates or updates a user quota record.
    fn upsert_user_quota(
        &self,
        quota: UserQuotaWrite,
    ) -> impl Future<Output = Result<(), DbErr>> + Send;
}

/// Persists file aggregate root writes.
pub trait FileRepository: Send + Sync {
    /// Creates or updates a user credential file record.
    fn upsert_user_credential_file(
        &self,
        file: UserCredentialFileWrite,
    ) -> impl Future<Output = Result<(), DbErr>> + Send;

    /// Creates or updates a Claude file record.
    fn upsert_claude_file(
        &self,
        file: ClaudeFileWrite,
    ) -> impl Future<Output = Result<(), DbErr>> + Send;
}

/// Persists global settings writes.
pub trait SettingsRepository: Send + Sync {
    /// Creates or updates the global settings record.
    fn upsert_global_settings(
        &self,
        settings: GlobalSettingsWrite,
    ) -> impl Future<Output = Result<(), DbErr>> + Send;
}

/// Dispatches append-only write events without waiting for completion.
pub trait WriteSink: Send + Sync {
    /// Sends a usage write event.
    fn send_usage(&self, usage: UsageWrite);

    /// Sends an upstream request write event.
    fn send_upstream_request(&self, request: UpstreamRequestWrite);

    /// Sends a downstream request write event.
    fn send_downstream_request(&self, request: DownstreamRequestWrite);
}
