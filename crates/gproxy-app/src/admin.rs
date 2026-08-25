use std::time::Duration;

use base64::Engine as _;
use gproxy_admin::dto::{ChannelDto, channel_dto};
use gproxy_admin::{AdminError, State};
use gproxy_channel_api::BoxFuture;
use gproxy_core::CacheBackend;
use gproxy_store::records::{AuditEventInput, CredentialEnvelope};
use sha2::{Digest, Sha256};

use crate::AppHandle;

impl State for AppHandle {
    fn store(&self) -> &gproxy_store::Store {
        &self.inner.host.services.store
    }

    fn seal_credential(
        &self,
        secret: &serde_json::Value,
    ) -> Result<CredentialEnvelope, AdminError> {
        self.inner
            .host
            .services
            .cipher
            .seal(secret)
            .map_err(|error| AdminError::Internal(error.to_string()))
    }

    fn seal_user_key(&self, api_key: &str) -> Result<CredentialEnvelope, AdminError> {
        self.inner
            .host
            .services
            .cipher
            .seal_user_key(&serde_json::Value::String(api_key.into()))
            .map_err(|error| AdminError::Internal(error.to_string()))
    }

    fn digest_user_key(&self, api_key: &str) -> (u32, Vec<u8>) {
        (
            crate::control::USER_KEY_DIGEST_VERSION,
            crate::control::user_key_digest(crate::control::USER_KEY_DIGEST_VERSION, api_key)
                .expect("current user-key digest version is supported"),
        )
    }

    fn reveal_user_key(
        &self,
        actor_admin_id: i64,
        id: i64,
        at: i64,
    ) -> BoxFuture<'_, Result<String, AdminError>> {
        Box::pin(async move {
            let secret = self
                .inner
                .host
                .services
                .store
                .user_key_secret(id)
                .await?
                .ok_or(AdminError::NotFound)?;
            let envelope = secret.envelope.ok_or_else(|| {
                AdminError::Conflict(
                    "key predates revealable storage and cannot be recovered".into(),
                )
            })?;
            let api_key = match self
                .inner
                .host
                .services
                .cipher
                .open_user_key(&envelope)
                .map_err(|error| AdminError::Internal(error.to_string()))?
            {
                serde_json::Value::String(value) => value,
                _ => {
                    return Err(AdminError::Internal(
                        "decrypted user key is not a string".into(),
                    ));
                }
            };
            self.inner
                .host
                .services
                .store
                .record_audit_event(&AuditEventInput {
                    actor_admin_id,
                    action: "user_key.reveal".into(),
                    target_kind: "user_key".into(),
                    target_id: Some(id),
                    at,
                    details: None,
                })
                .await?;
            Ok(api_key)
        })
    }

    fn admit_auth_attempt(
        &self,
        scope: &'static str,
        username: &str,
    ) -> BoxFuture<'_, Result<(), AdminError>> {
        let key = auth_limit_key(scope, username);
        Box::pin(async move {
            let attempts = self
                .inner
                .host
                .services
                .cache
                .incr(&key, 1, Some(Duration::from_secs(60)))
                .await
                .map_err(|error| AdminError::Internal(error.to_string()))?;
            let limit = match scope {
                "setup-source" => 4,
                scope if scope.ends_with("-source") => 60,
                _ => 8,
            };
            if attempts > limit {
                Err(AdminError::RateLimited)
            } else {
                Ok(())
            }
        })
    }

    fn clear_auth_attempts(
        &self,
        scope: &'static str,
        username: &str,
    ) -> BoxFuture<'_, Result<(), AdminError>> {
        let key = auth_limit_key(scope, username);
        Box::pin(async move {
            self.inner
                .host
                .services
                .cache
                .delete(&key)
                .await
                .map_err(|error| AdminError::Internal(error.to_string()))
        })
    }

    fn reload(&self) -> BoxFuture<'_, Result<(), AdminError>> {
        Box::pin(async move {
            AppHandle::reload(self)
                .await
                .map_err(|error| AdminError::Internal(error.to_string()))
        })
    }

    fn channel_catalogue(&self) -> Vec<ChannelDto> {
        self.inner
            .core
            .channel_descriptors()
            .map(channel_dto)
            .collect()
    }

    fn normalize_provider_settings(
        &self,
        channel: &str,
        settings: &serde_json::Value,
    ) -> Result<serde_json::Value, AdminError> {
        gproxy_channels::canonical_provider_settings(channel, settings)
            .map_err(AdminError::BadRequest)
    }
}

fn auth_limit_key(scope: &str, username: &str) -> String {
    let subject = username.trim().as_bytes();
    let digest = Sha256::digest(subject);
    let digest = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);
    format!("gproxy:admin-auth:{scope}:{digest}")
}
