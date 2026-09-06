use std::time::Duration;

use gproxy_core::error::StoreError as CoreStoreError;
use gproxy_core::{BoxFuture, CacheBackend as _, CredentialId, CredentialRecord, CredentialStore};

use super::AppHost;

impl CredentialStore for AppHost {
    fn load<'a>(
        &'a self,
        id: CredentialId,
    ) -> BoxFuture<'a, Result<CredentialRecord, CoreStoreError>> {
        Box::pin(async move {
            if let Some(record) = self.services.control.cached_credential(id.0) {
                return Ok(record);
            }
            let stored = self
                .services
                .store
                .credential(id.0)
                .await
                .map_err(store_error)?
                .filter(|credential| credential.enabled)
                .ok_or_else(unavailable)?;
            let secret = self
                .services
                .cipher
                .open(&stored.envelope)
                .map_err(|_| encryption_error())?;
            let record = CredentialRecord {
                id,
                channel: gproxy_channels::canonical_channel_id(&stored.channel).into(),
                kind: stored.kind,
                secret,
                version: stored.version,
            };
            self.services.control.cache_credential(&record);
            Ok(record)
        })
    }

    fn persist_rotation<'a>(
        &'a self,
        id: CredentialId,
        secret: serde_json::Value,
        version: u64,
    ) -> BoxFuture<'a, Result<(), CoreStoreError>> {
        Box::pin(async move {
            // Whether or not the write wins, the next load must see the row
            // as it is now: a peer may have rotated first.
            self.services.control.forget_credential(id.0);
            let result = match self.services.cipher.seal(&secret) {
                Ok(envelope) => self
                    .services
                    .store
                    .persist_credential_rotation(id.0, &envelope, version)
                    .await
                    .map_err(store_error),
                Err(_) => Err(encryption_error()),
            };
            let released = self
                .services
                .cache
                .delete(&refresh_key(id))
                .await
                .map_err(cache_error);
            result.and(released)
        })
    }

    fn lease_refresh<'a>(
        &'a self,
        id: CredentialId,
        ttl: Duration,
    ) -> BoxFuture<'a, Result<bool, CoreStoreError>> {
        Box::pin(async move {
            self.services
                .cache
                .incr(&refresh_key(id), 1, Some(ttl))
                .await
                .map(|value| value == 1)
                .map_err(cache_error)
        })
    }
}

fn refresh_key(id: CredentialId) -> String {
    format!("gproxy:refresh:{}", id.0)
}

fn unavailable() -> CoreStoreError {
    CoreStoreError("credential is unavailable".into())
}

fn store_error(error: gproxy_store::StoreError) -> CoreStoreError {
    let message = match error {
        gproxy_store::StoreError::VersionConflict => "credential version conflict",
        gproxy_store::StoreError::Database(_) | gproxy_store::StoreError::InvalidData { .. } => {
            "credential persistence failed"
        }
    };
    CoreStoreError(message.into())
}

fn encryption_error() -> CoreStoreError {
    CoreStoreError("credential encryption failed".into())
}

fn cache_error(_: CoreStoreError) -> CoreStoreError {
    CoreStoreError("credential refresh cache failed".into())
}
