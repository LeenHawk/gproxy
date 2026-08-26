mod portal;

use std::time::Duration;

use base64::Engine as _;
use gproxy_admin::dto::{ChannelDto, PortalModelDto, channel_dto};
use gproxy_admin::{AdminError, PortalIdentity, State};
use gproxy_channel_api::{AuthCodeStart, BoxFuture, DeviceInit, DevicePoll};
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

    fn login_state_get<'a>(
        &'a self,
        key: &'a str,
    ) -> BoxFuture<'a, Result<Option<Vec<u8>>, AdminError>> {
        Box::pin(async move {
            self.inner
                .host
                .services
                .cache
                .get(key)
                .await
                .map_err(cache_error)
        })
    }

    fn login_state_set<'a>(
        &'a self,
        key: &'a str,
        value: Vec<u8>,
        ttl: Duration,
    ) -> BoxFuture<'a, Result<(), AdminError>> {
        Box::pin(async move {
            self.inner
                .host
                .services
                .cache
                .set(key, value, Some(ttl))
                .await
                .map_err(cache_error)
        })
    }

    fn login_state_delete<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Result<(), AdminError>> {
        Box::pin(async move {
            self.inner
                .host
                .services
                .cache
                .delete(key)
                .await
                .map_err(cache_error)
        })
    }

    fn login_authcode_start<'a>(
        &'a self,
        channel: &'a str,
        provider_id: i64,
        params: &'a serde_json::Value,
        redirect_uri: &'a str,
        flow_state: &'a str,
        pkce_challenge: &'a str,
    ) -> BoxFuture<'a, Result<Option<AuthCodeStart>, AdminError>> {
        Box::pin(async move {
            let provider = self.login_provider(provider_id, channel)?;
            self.inner
                .core
                .login_authcode_start(
                    channel,
                    &provider,
                    params,
                    redirect_uri,
                    flow_state,
                    pkce_challenge,
                )
                .await
                .map_err(login_error)
        })
    }

    fn login_authcode_exchange<'a>(
        &'a self,
        channel: &'a str,
        provider_id: i64,
        code: &'a str,
        verifier: &'a str,
        redirect_uri: &'a str,
        extra: Option<&'a serde_json::Value>,
    ) -> BoxFuture<'a, Result<serde_json::Value, AdminError>> {
        Box::pin(async move {
            let provider = self.login_provider(provider_id, channel)?;
            self.inner
                .core
                .login_authcode_exchange(channel, &provider, code, verifier, redirect_uri, extra)
                .await
                .map_err(login_error)
        })
    }

    fn login_device_start<'a>(
        &'a self,
        channel: &'a str,
        provider_id: i64,
        params: &'a serde_json::Value,
    ) -> BoxFuture<'a, Result<DeviceInit, AdminError>> {
        Box::pin(async move {
            let provider = self.login_provider(provider_id, channel)?;
            self.inner
                .core
                .login_device_start(channel, &provider, params)
                .await
                .map_err(login_error)
        })
    }

    fn login_device_poll<'a>(
        &'a self,
        channel: &'a str,
        provider_id: i64,
        device_code: &'a str,
    ) -> BoxFuture<'a, Result<DevicePoll, AdminError>> {
        Box::pin(async move {
            let provider = self.login_provider(provider_id, channel)?;
            self.inner
                .core
                .login_device_poll(channel, &provider, device_code)
                .await
                .map_err(login_error)
        })
    }

    fn login_cookie_exchange<'a>(
        &'a self,
        channel: &'a str,
        provider_id: i64,
        cookie: &'a str,
    ) -> BoxFuture<'a, Result<serde_json::Value, AdminError>> {
        Box::pin(async move {
            let provider = self.login_provider(provider_id, channel)?;
            self.inner
                .core
                .login_cookie_exchange(channel, &provider, cookie)
                .await
                .map_err(login_error)
        })
    }

    fn channel_catalogue(&self) -> Vec<ChannelDto> {
        self.inner.core.channels().map(channel_dto).collect()
    }

    fn normalize_provider_settings(
        &self,
        channel: &str,
        settings: &serde_json::Value,
    ) -> Result<serde_json::Value, AdminError> {
        gproxy_channels::canonical_provider_settings(channel, settings)
            .map_err(AdminError::BadRequest)
    }

    fn portal_identity(&self, headers: &http::HeaderMap) -> Result<PortalIdentity, AdminError> {
        portal::identity(self, headers)
    }

    fn portal_models(&self, identity: &PortalIdentity) -> Vec<PortalModelDto> {
        portal::models(self, identity)
    }
}

impl AppHandle {
    fn login_provider(
        &self,
        provider_id: i64,
        channel: &str,
    ) -> Result<gproxy_core::ProviderRef, AdminError> {
        self.inner
            .host
            .services
            .control
            .provider(provider_id)
            .filter(|provider| provider.channel == channel)
            .ok_or_else(|| AdminError::BadRequest("login provider is unavailable".into()))
    }
}

fn cache_error(_: gproxy_core::error::StoreError) -> AdminError {
    AdminError::Internal("login state cache failed".into())
}

fn login_error(error: gproxy_channel_api::ChannelError) -> AdminError {
    match error {
        gproxy_channel_api::ChannelError::Unsupported(_) => {
            AdminError::BadRequest("channel does not support this login flow".into())
        }
        _ => AdminError::BadRequest("provider login step failed".into()),
    }
}

fn auth_limit_key(scope: &str, username: &str) -> String {
    let subject = username.trim().as_bytes();
    let digest = Sha256::digest(subject);
    let digest = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);
    format!("gproxy:admin-auth:{scope}:{digest}")
}
