use base64::Engine as _;
use gproxy_admin::AdminError;
use sha2::{Digest, Sha256};

use crate::AppHandle;

pub(super) async fn reveal_credential_secret(
    app: &AppHandle,
    id: i64,
) -> Result<serde_json::Value, AdminError> {
    let services = &app.inner.host.services;
    let stored = services
        .store
        .credential(id)
        .await?
        .ok_or(AdminError::NotFound)?;
    services
        .cipher
        .open(&stored.envelope)
        .map_err(|error| AdminError::Internal(error.to_string()))
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn tokenizer_dto(
    vocab: gproxy_store::records::TokenizerVocabRecord,
) -> gproxy_admin::dto::TokenizerVocabDto {
    gproxy_admin::dto::TokenizerVocabDto {
        name: vocab.name,
        repository: vocab.repository,
        size_bytes: vocab.size_bytes,
        updated_at: vocab.updated_at,
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn tokenizer_progress_dto(
    progress: gproxy_tokenize::TokenizerDownloadProgress,
) -> gproxy_admin::dto::TokenizerDownloadProgressDto {
    gproxy_admin::dto::TokenizerDownloadProgressDto {
        downloaded_bytes: progress.downloaded_bytes,
        total_bytes: progress.total_bytes,
    }
}

impl AppHandle {
    pub(super) fn login_provider(
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

pub(super) fn cache_error(_: gproxy_core::error::StoreError) -> AdminError {
    AdminError::Internal("login state cache failed".into())
}

pub(super) fn login_error(error: gproxy_channel_api::ChannelError) -> AdminError {
    match error {
        gproxy_channel_api::ChannelError::Unsupported(_) => {
            AdminError::BadRequest("channel does not support this login flow".into())
        }
        _ => AdminError::BadRequest("provider login step failed".into()),
    }
}

pub(super) fn auth_limit_key(scope: &str, username: &str) -> String {
    let subject = username.trim().as_bytes();
    let digest = Sha256::digest(subject);
    let digest = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);
    format!("gproxy:admin-auth:{scope}:{digest}")
}

pub(super) async fn operator_key(
    app: &AppHandle,
    actor_user_id: i64,
    snapshot: &gproxy_store::records::ControlSnapshot,
) -> Result<(String, String), AdminError> {
    let key = snapshot
        .user_keys
        .iter()
        .filter(|key| key.user_id == actor_user_id && key.enabled)
        .min_by_key(|key| key.id)
        .ok_or_else(|| {
            AdminError::BadRequest("this administrator has no enabled API key to use".into())
        })?;
    let stored = app
        .inner
        .host
        .services
        .store
        .user_key_secret(key.id)
        .await?
        .and_then(|secret| secret.envelope)
        .ok_or_else(|| AdminError::Conflict("that key predates revealable storage".into()))?;
    let secret = app
        .inner
        .host
        .services
        .cipher
        .open_user_key(&stored)
        .map_err(|error| AdminError::Internal(error.to_string()))?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| AdminError::Internal("stored key is not a string".into()))?;
    Ok((
        key.prefix.clone().unwrap_or_else(|| format!("#{}", key.id)),
        secret,
    ))
}
