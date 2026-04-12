use std::future::Future;

use sea_orm::DbErr;

use crate::repository::{
    CredentialRepository, FileRepository, ModelRepository, PermissionRepository,
    ProviderRepository, QuotaRepository, SettingsRepository, UserRepository, WriteSink,
};
use crate::seaorm::SeaOrmStorage;
use crate::write::*;

fn send_write_event(storage: SeaOrmStorage, event: StorageWriteEvent) {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        std::mem::drop(handle.spawn(async move {
            let _ = storage.apply_write_event(event).await;
        }));
        return;
    }

    let _ = std::thread::Builder::new()
        .name("gproxy-storage-write-sink".to_string())
        .spawn(move || {
            if let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                let _ = runtime.block_on(storage.apply_write_event(event));
            }
        });
}

impl ProviderRepository for SeaOrmStorage {
    fn upsert_provider(
        &self,
        provider: ProviderWrite,
    ) -> impl Future<Output = Result<(), DbErr>> + Send {
        self.apply_write_event(StorageWriteEvent::UpsertProvider(provider))
    }

    fn delete_provider(&self, id: i64) -> impl Future<Output = Result<(), DbErr>> + Send {
        self.apply_write_event(StorageWriteEvent::DeleteProvider { id })
    }
}

impl CredentialRepository for SeaOrmStorage {
    fn upsert_credential(
        &self,
        credential: CredentialWrite,
    ) -> impl Future<Output = Result<(), DbErr>> + Send {
        self.apply_write_event(StorageWriteEvent::UpsertCredential(credential))
    }

    fn delete_credential(&self, id: i64) -> impl Future<Output = Result<(), DbErr>> + Send {
        self.apply_write_event(StorageWriteEvent::DeleteCredential { id })
    }

    fn upsert_credential_status(
        &self,
        status: CredentialStatusWrite,
    ) -> impl Future<Output = Result<(), DbErr>> + Send {
        self.apply_write_event(StorageWriteEvent::UpsertCredentialStatus(status))
    }

    fn delete_credential_status(&self, id: i64) -> impl Future<Output = Result<(), DbErr>> + Send {
        self.apply_write_event(StorageWriteEvent::DeleteCredentialStatus { id })
    }
}

impl UserRepository for SeaOrmStorage {
    fn upsert_user(&self, user: UserWrite) -> impl Future<Output = Result<(), DbErr>> + Send {
        self.apply_write_event(StorageWriteEvent::UpsertUser(user))
    }

    fn delete_user(&self, id: i64) -> impl Future<Output = Result<(), DbErr>> + Send {
        self.apply_write_event(StorageWriteEvent::DeleteUser { id })
    }

    fn upsert_user_key(
        &self,
        user_key: UserKeyWrite,
    ) -> impl Future<Output = Result<(), DbErr>> + Send {
        self.apply_write_event(StorageWriteEvent::UpsertUserKey(user_key))
    }

    fn delete_user_key(&self, id: i64) -> impl Future<Output = Result<(), DbErr>> + Send {
        self.apply_write_event(StorageWriteEvent::DeleteUserKey { id })
    }
}

impl ModelRepository for SeaOrmStorage {
    fn upsert_model(&self, model: ModelWrite) -> impl Future<Output = Result<(), DbErr>> + Send {
        self.apply_write_event(StorageWriteEvent::UpsertModel(model))
    }

    fn delete_model(&self, id: i64) -> impl Future<Output = Result<(), DbErr>> + Send {
        self.apply_write_event(StorageWriteEvent::DeleteModel { id })
    }
}

impl PermissionRepository for SeaOrmStorage {
    fn upsert_user_permission(
        &self,
        permission: UserModelPermissionWrite,
    ) -> impl Future<Output = Result<(), DbErr>> + Send {
        self.apply_write_event(StorageWriteEvent::UpsertUserModelPermission(permission))
    }

    fn delete_user_permission(&self, id: i64) -> impl Future<Output = Result<(), DbErr>> + Send {
        self.apply_write_event(StorageWriteEvent::DeleteUserModelPermission { id })
    }

    fn upsert_user_file_permission(
        &self,
        permission: UserFilePermissionWrite,
    ) -> impl Future<Output = Result<(), DbErr>> + Send {
        self.apply_write_event(StorageWriteEvent::UpsertUserFilePermission(permission))
    }

    fn delete_user_file_permission(
        &self,
        id: i64,
    ) -> impl Future<Output = Result<(), DbErr>> + Send {
        self.apply_write_event(StorageWriteEvent::DeleteUserFilePermission { id })
    }

    fn upsert_user_rate_limit(
        &self,
        rate_limit: UserRateLimitWrite,
    ) -> impl Future<Output = Result<(), DbErr>> + Send {
        self.apply_write_event(StorageWriteEvent::UpsertUserRateLimit(rate_limit))
    }

    fn delete_user_rate_limit(&self, id: i64) -> impl Future<Output = Result<(), DbErr>> + Send {
        self.apply_write_event(StorageWriteEvent::DeleteUserRateLimit { id })
    }
}

impl QuotaRepository for SeaOrmStorage {
    fn upsert_user_quota(
        &self,
        quota: UserQuotaWrite,
    ) -> impl Future<Output = Result<(), DbErr>> + Send {
        self.apply_write_event(StorageWriteEvent::UpsertUserQuota(quota))
    }
}

impl FileRepository for SeaOrmStorage {
    fn upsert_user_credential_file(
        &self,
        file: UserCredentialFileWrite,
    ) -> impl Future<Output = Result<(), DbErr>> + Send {
        self.apply_write_event(StorageWriteEvent::UpsertUserCredentialFile(file))
    }

    fn upsert_claude_file(
        &self,
        file: ClaudeFileWrite,
    ) -> impl Future<Output = Result<(), DbErr>> + Send {
        self.apply_write_event(StorageWriteEvent::UpsertClaudeFile(file))
    }
}

impl SettingsRepository for SeaOrmStorage {
    fn upsert_global_settings(
        &self,
        settings: GlobalSettingsWrite,
    ) -> impl Future<Output = Result<(), DbErr>> + Send {
        self.apply_write_event(StorageWriteEvent::UpsertGlobalSettings(settings))
    }
}

impl WriteSink for SeaOrmStorage {
    fn send_usage(&self, usage: UsageWrite) {
        send_write_event(self.clone(), StorageWriteEvent::UpsertUsage(usage));
    }

    fn send_upstream_request(&self, request: UpstreamRequestWrite) {
        send_write_event(
            self.clone(),
            StorageWriteEvent::UpsertUpstreamRequest(request),
        );
    }

    fn send_downstream_request(&self, request: DownstreamRequestWrite) {
        send_write_event(
            self.clone(),
            StorageWriteEvent::UpsertDownstreamRequest(request),
        );
    }
}
