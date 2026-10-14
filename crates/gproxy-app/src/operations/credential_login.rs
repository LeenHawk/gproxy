//! Scope-aware upstream login. SDK sessions are opaque; this binding records
//! which administrator may continue one and rechecks its intended owner.
use std::time::Duration;

use gproxy_sdk::{
    Gproxy,
    dto::{
        AuthCodeComplete, AuthCodeStart, AuthCodeStarted, CookieExchange, CredentialCreated,
        CredentialOwner, DevicePollOutcome, DeviceStart, DeviceStarted,
    },
};
use gproxy_seaorm::BatchConnectionTrait;
use serde::{Deserialize, Serialize};

use crate::{AdminScope, AppData, AppError, Caller, Result, admin_scope::ScopeOwner};

#[derive(Serialize, Deserialize)]
struct Binding {
    user_id: String,
    scope: String,
    provider_id: String,
    owner: CredentialOwner,
}

pub struct CredentialLogin<'a, C> {
    gproxy: &'a Gproxy<C>,
    data: &'a AppData,
    scope: &'a AdminScope,
    caller: &'a Caller,
}

impl<'a, C: BatchConnectionTrait + Send + Sync + 'static> CredentialLogin<'a, C> {
    pub fn new(
        gproxy: &'a Gproxy<C>,
        data: &'a AppData,
        scope: &'a AdminScope,
        caller: &'a Caller,
    ) -> Self {
        Self {
            gproxy,
            data,
            scope,
            caller,
        }
    }

    fn admit(&self, provider_id: &str, owner: &CredentialOwner) -> Result<()> {
        crate::require_section(self.scope, "credentials")?;
        self.scope.admit_write(
            ScopeOwner::from_columns(
                owner.user_id.as_deref(),
                owner.team_id.as_deref(),
                owner.organization_id.as_deref(),
            ),
            self.data,
        )?;
        let snapshot = self.gproxy.core().snapshot();
        let provider = snapshot
            .providers
            .get(provider_id)
            .ok_or_else(|| AppError::not_found("provider", provider_id))?;
        if !provider.entity.enabled {
            return Err(AppError::invalid("provider is disabled"));
        }
        Ok(())
    }

    fn key(id: &str) -> String {
        format!("gproxy-app:v1:credential-login:{id}")
    }

    async fn remember(
        &self,
        id: &str,
        provider_id: String,
        owner: CredentialOwner,
        ttl: Duration,
    ) -> Result<()> {
        let binding = Binding {
            user_id: self.caller.user_id.clone(),
            scope: self.scope.selector(),
            provider_id,
            owner,
        };
        let bytes = serde_json::to_vec(&binding).map_err(|e| AppError::internal(e.to_string()))?;
        self.gproxy
            .cache()
            .put(&Self::key(id), bytes, ttl.max(Duration::from_secs(1)))
            .await?;
        Ok(())
    }

    async fn resume(&self, id: &str) -> Result<()> {
        let entry = self
            .gproxy
            .cache()
            .get(&Self::key(id))
            .await?
            .ok_or_else(|| AppError::not_found("login session", id))?;
        let binding: Binding = serde_json::from_slice(&entry.value)
            .map_err(|_| AppError::internal("invalid login binding"))?;
        if binding.user_id != self.caller.user_id || binding.scope != self.scope.selector() {
            return Err(AppError::not_found("login session", id));
        }
        self.admit(&binding.provider_id, &binding.owner)
    }

    pub async fn authcode_start(&self, request: AuthCodeStart) -> Result<AuthCodeStarted> {
        self.admit(&request.provider_id, &request.owner)?;
        let provider = request.provider_id.clone();
        let owner = request.owner.clone();
        let started = self.gproxy.login().authcode_start(request).await?;
        self.remember(
            &started.login_session_id,
            provider,
            owner,
            self.gproxy.login_ttl().authorization_code,
        )
        .await?;
        Ok(started)
    }

    pub async fn authcode_complete(&self, request: AuthCodeComplete) -> Result<CredentialCreated> {
        self.resume(&request.login_session_id).await?;
        // The SDK checks the submitted state against the saved session.
        if !authorization_has_state(&request) {
            return Err(AppError::invalid(
                "a callback URL or authorization code with state is required",
            ));
        }
        let id = request.login_session_id.clone();
        let result = self.gproxy.login().authcode_complete(request).await?;
        self.gproxy.cache().delete(&Self::key(&id)).await?;
        Ok(result)
    }

    pub async fn device_start(&self, request: DeviceStart) -> Result<DeviceStarted> {
        self.admit(&request.provider_id, &request.owner)?;
        let provider = request.provider_id.clone();
        let owner = request.owner.clone();
        let started = self.gproxy.login().device_start(request).await?;
        let mut ttl = self.gproxy.login_ttl().device_code;
        if let Some(expires) = started.expires_at_ms {
            let now = web_time::SystemTime::now()
                .duration_since(web_time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64;
            ttl = ttl.min(Duration::from_millis(
                expires.saturating_sub(now).max(0) as u64
            ));
        }
        self.remember(&started.login_session_id, provider, owner, ttl)
            .await?;
        Ok(started)
    }

    pub async fn device_poll(&self, id: &str) -> Result<DevicePollOutcome> {
        self.resume(id).await?;
        let result = self.gproxy.login().device_poll(id).await?;
        if !matches!(result, DevicePollOutcome::Pending { .. }) {
            self.gproxy.cache().delete(&Self::key(id)).await?;
        }
        Ok(result)
    }

    pub async fn cookie_exchange(&self, request: CookieExchange) -> Result<CredentialCreated> {
        self.admit(&request.provider_id, &request.owner)?;
        Ok(self.gproxy.login().cookie_exchange(request).await?)
    }
}

/// Both browser callbacks and manually pasted PKCE codes must carry state.
fn authorization_has_state(request: &AuthCodeComplete) -> bool {
    if let Some(callback) = request.callback_url.as_deref() {
        let query = callback
            .split_once('?')
            .map(|(_, q)| q.split('#').next().unwrap_or_default())
            .unwrap_or_default();
        request.code.is_none()
            && form_urlencoded::parse(query.as_bytes())
                .any(|(key, value)| key == "state" && !value.trim().is_empty())
    } else {
        request
            .code
            .as_deref()
            .is_some_and(|code| !code.trim().is_empty())
            && request
                .state
                .as_deref()
                .is_some_and(|state| !state.trim().is_empty())
    }
}
