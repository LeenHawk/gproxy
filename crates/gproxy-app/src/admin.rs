mod connectivity;
mod helpers;
mod import;
mod model_discover;
mod model_test;
mod portal;
mod quota_probe;
mod quota_reset;

use std::time::Duration;

use gproxy_admin::dto::{ChannelDto, ExportSourceKeyDto, PortalModelDto, channel_dto};
use gproxy_admin::{AdminError, PortalIdentity, State};
use gproxy_channel_api::{AuthCodeStart, BoxFuture, DeviceInit, DevicePoll};
use gproxy_core::CacheBackend;
use gproxy_store::records::CredentialEnvelope;

use crate::AppHandle;
use helpers::{auth_limit_key, cache_error, login_error, operator_key};
#[cfg(not(target_arch = "wasm32"))]
use helpers::{tokenizer_dto, tokenizer_progress_dto};

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

    fn open_imported_credential(
        &self,
        envelope: &CredentialEnvelope,
        source: &ExportSourceKeyDto,
        source_master_key: Option<&str>,
    ) -> Result<serde_json::Value, AdminError> {
        import::open_credential(envelope, source, source_master_key)
    }

    fn reseal_imported_user_key(
        &self,
        envelope: &CredentialEnvelope,
        source: &ExportSourceKeyDto,
        key: Option<&str>,
    ) -> Result<CredentialEnvelope, AdminError> {
        import::reseal_user_key(&self.inner.host.services.cipher, envelope, source, key)
    }

    fn digest_user_key(&self, api_key: &str) -> (u32, Vec<u8>) {
        (
            crate::control::USER_KEY_DIGEST_VERSION,
            crate::control::user_key_digest(crate::control::USER_KEY_DIGEST_VERSION, api_key)
                .expect("current user-key digest version is supported"),
        )
    }

    fn reveal_user_key(&self, id: i64) -> BoxFuture<'_, Result<String, AdminError>> {
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
            Ok(api_key)
        })
    }

    fn reveal_credential_secret(
        &self,
        id: i64,
    ) -> BoxFuture<'_, Result<serde_json::Value, AdminError>> {
        Box::pin(async move {
            let stored = self
                .inner
                .host
                .services
                .store
                .credential(id)
                .await?
                .ok_or(AdminError::NotFound)?;
            let secret = self
                .inner
                .host
                .services
                .cipher
                .open(&stored.envelope)
                .map_err(|error| AdminError::Internal(error.to_string()))?;
            Ok(secret)
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

    fn connectivity_test<'a>(
        &'a self,
        request: &'a gproxy_admin::dto::ConnectivityTestRequest,
    ) -> BoxFuture<'a, Result<gproxy_admin::dto::ConnectivityTestResponse, AdminError>> {
        #[cfg(not(target_arch = "wasm32"))]
        return Box::pin(connectivity::run(
            self,
            request,
            &self.inner.host.services.transport,
        ));
        #[cfg(target_arch = "wasm32")]
        {
            let _ = request;
            Box::pin(async {
                Err(AdminError::BadRequest(
                    "connectivity testing is unavailable on edge".into(),
                ))
            })
        }
    }

    fn test_model<'a>(
        &'a self,
        actor_user_id: i64,
        request: &'a gproxy_admin::dto::ModelTestRequest,
    ) -> BoxFuture<'a, Result<gproxy_admin::dto::ModelTestResponse, AdminError>> {
        Box::pin(model_test::run(self, actor_user_id, request))
    }

    fn quota_probe<'a>(
        &'a self,
        credential_id: i64,
    ) -> BoxFuture<'a, Result<gproxy_admin::dto::QuotaProbeResponse, AdminError>> {
        Box::pin(quota_probe::run(self, credential_id))
    }

    fn quota_reset<'a>(
        &'a self,
        credential_id: i64,
    ) -> BoxFuture<'a, Result<gproxy_admin::dto::QuotaResetResponse, AdminError>> {
        Box::pin(quota_reset::run(self, credential_id))
    }

    fn discover_models<'a>(
        &'a self,
        actor_user_id: i64,
        request: &'a gproxy_admin::dto::ModelDiscoverRequest,
    ) -> BoxFuture<'a, Result<gproxy_admin::dto::ModelDiscoverResponse, AdminError>> {
        Box::pin(model_discover::run(self, actor_user_id, request))
    }

    fn fetch_tokenizer_vocab<'a>(
        &'a self,
        name: &'a str,
        repository: &'a str,
    ) -> BoxFuture<'a, Result<gproxy_admin::dto::TokenizerVocabDto, AdminError>> {
        Box::pin(async move {
            #[cfg(target_arch = "wasm32")]
            {
                let _ = (name, repository);
                Err(AdminError::Forbidden)
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                if name.is_empty() {
                    return Err(AdminError::BadRequest(
                        "tokenizer vocabulary name must not be blank".into(),
                    ));
                }
                if repository.is_empty() {
                    return Err(AdminError::BadRequest(
                        "tokenizer repository must not be blank".into(),
                    ));
                }
                let registry = &self.inner.host.services.tokenizers;
                registry.fetch(name, repository).await.map_err(|error| {
                    tracing::warn!(name, repository, %error, "manual tokenizer fetch failed");
                    AdminError::BadRequest("tokenizer vocabulary could not be fetched".into())
                })?;
                self.inner
                    .host
                    .services
                    .store
                    .tokenizer_vocabs()
                    .await?
                    .into_iter()
                    .find(|vocab| vocab.name == name)
                    .map(tokenizer_dto)
                    .ok_or(AdminError::NotFound)
            }
        })
    }

    fn tokenizer_vocab_progress(
        &self,
        name: &str,
    ) -> Option<gproxy_admin::dto::TokenizerDownloadProgressDto> {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = name;
            None
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.inner
                .host
                .services
                .tokenizers
                .download_progress(name)
                .map(tokenizer_progress_dto)
        }
    }

    fn delete_tokenizer_vocab<'a>(
        &'a self,
        name: &'a str,
    ) -> BoxFuture<'a, Result<(), AdminError>> {
        Box::pin(async move {
            #[cfg(target_arch = "wasm32")]
            {
                let _ = name;
                Err(AdminError::Forbidden)
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                let registry = &self.inner.host.services.tokenizers;
                self.inner
                    .host
                    .services
                    .store
                    .delete_tokenizer_vocab(name)
                    .await?;
                registry.evict(name);
                Ok(())
            }
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
