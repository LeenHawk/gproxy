use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;

use std::sync::atomic::{AtomicU64, AtomicUsize};

use arc_swap::ArcSwap;
use dashmap::DashMap;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::broadcast;

use gproxy_protocol::kinds::{OperationFamily, ProtocolKind};

use crate::affinity::{CacheAffinityHint, CacheAffinityPool, DEFAULT_CACHE_AFFINITY_MAX_KEYS};
use crate::channel::{
    Channel, ChannelCredential, ChannelSettings, OAuthCredentialResult, OAuthFlow,
};
use crate::dispatch::DispatchTable;
use crate::dispatch::RouteKey;
use crate::health::CredentialHealth;
use crate::request::PreparedRequest;
use crate::response::{UpstreamError, UpstreamResponse, UpstreamStreamingResponse};
use crate::retry::{RetryContext, retry_with_credentials, retry_with_credentials_stream};

type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Snapshot of credentials, revision, paired health states, and max retries for a retry cycle.
type RetryState<Cred, Health> = (Arc<Vec<Cred>>, u64, Vec<(Cred, Health)>, u32);

#[derive(Debug, Clone, Serialize)]
pub struct ProviderSnapshot {
    pub name: String,
    pub channel: String,
    pub settings: Value,
    pub credential_count: usize,
    pub credential_revision: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CredentialSnapshot {
    pub provider: String,
    pub index: usize,
    pub revision: u64,
    pub credential: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct CredentialUpdate {
    pub provider: String,
    pub index: usize,
    pub revision: u64,
    pub credential: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct OAuthFinishResult {
    pub credential: CredentialSnapshot,
    pub details: Value,
}

#[derive(Debug, Clone)]
pub enum EngineEvent {
    ProviderAdded {
        name: String,
    },
    ProviderRemoved {
        name: String,
    },
    ProviderUpdated {
        name: String,
    },
    CredentialHealthChanged {
        provider: String,
        index: usize,
        status: String,
    },
}

pub trait ProviderRegistry {
    fn get_provider(&self, name: &str) -> Result<Option<ProviderSnapshot>, UpstreamError>;
    fn list_providers(&self) -> Result<Vec<ProviderSnapshot>, UpstreamError>;
    fn get_credential(
        &self,
        provider_name: &str,
        index: usize,
    ) -> Result<Option<CredentialSnapshot>, UpstreamError>;
    fn list_credentials(
        &self,
        provider_name: Option<&str>,
    ) -> Result<Vec<CredentialSnapshot>, UpstreamError>;
}

pub trait ProviderMutator {
    fn upsert_provider_json(
        &self,
        config: crate::engine::ProviderConfig,
    ) -> Result<(), UpstreamError>;
    fn remove_provider(&self, name: &str) -> bool;
    fn upsert_credential_json(
        &self,
        provider_name: &str,
        index: Option<usize>,
        credential: Value,
    ) -> Result<Option<CredentialSnapshot>, UpstreamError>;
    fn remove_credential(
        &self,
        provider_name: &str,
        index: usize,
    ) -> Result<Option<CredentialSnapshot>, UpstreamError>;
}

pub trait EngineEventSource {
    fn subscribe(&self) -> broadcast::Receiver<EngineEvent>;
}

/// Health status snapshot for a single credential.
#[derive(Debug, Clone, Serialize)]
pub struct CredentialHealthSnapshot {
    pub provider: String,
    pub index: usize,
    /// `"healthy"` or `"unavailable"`.
    pub status: String,
    /// true if `is_available(None)` returns true.
    pub available: bool,
}

pub(crate) struct ProviderExecuteResult {
    pub response: UpstreamResponse,
    pub credential_updates: Vec<CredentialUpdate>,
    pub credential_index: usize,
    pub attempt_meta: crate::retry::UpstreamAttemptMeta,
}

pub(crate) struct ProviderExecuteStreamResult {
    pub response: UpstreamStreamingResponse,
    pub credential_updates: Vec<CredentialUpdate>,
    pub credential_index: usize,
    pub attempt_meta: crate::retry::UpstreamAttemptMeta,
}

/// Execution outcome that bundles a success value or error with an
/// optional `FailedUpstreamAttempt` snapshot. On the error path the
/// snapshot carries the real upstream URL, request headers/body, and
/// (if the upstream actually responded) the response status / headers /
/// body so the logger can persist a full upstream-request row instead
/// of a placeholder.
pub(crate) struct ExecutionOutcome<T> {
    pub inner: Result<T, UpstreamError>,
    pub failed_attempt: Option<crate::response::FailedUpstreamAttempt>,
}

impl<T> ExecutionOutcome<T> {
    fn ok(value: T) -> Self {
        Self {
            inner: Ok(value),
            failed_attempt: None,
        }
    }

    fn err(
        error: UpstreamError,
        failed_attempt: Option<crate::response::FailedUpstreamAttempt>,
    ) -> Self {
        Self {
            inner: Err(error),
            failed_attempt,
        }
    }
}

pub(crate) trait ProviderRuntime: Send + Sync {
    fn dispatch_table(&self) -> &DispatchTable;
    fn build_billing_context(
        &self,
        request: &PreparedRequest,
    ) -> Option<crate::billing::BillingContext>;
    fn estimate_billing(
        &self,
        context: &crate::billing::BillingContext,
        usage: &crate::engine::Usage,
    ) -> Option<crate::billing::BillingResult>;

    fn handle_local(
        &self,
        operation: OperationFamily,
        protocol: ProtocolKind,
        body: &[u8],
    ) -> Option<Result<Vec<u8>, UpstreamError>>;

    fn finalize_request(&self, request: PreparedRequest) -> Result<PreparedRequest, UpstreamError>;

    fn normalize_response(&self, request: &PreparedRequest, body: Vec<u8>) -> Vec<u8>;

    fn sanitize_rules(&self) -> Vec<crate::utils::sanitize::SanitizeRule>;

    fn rewrite_rules(&self) -> Vec<crate::utils::rewrite::RewriteRule>;

    fn enable_suffix(&self) -> bool;

    /// Build WS-ready (url, headers) pairs for each credential.
    fn prepare_ws_auth(
        &self,
        path: &str,
        model: Option<&str>,
    ) -> Result<Vec<(String, http::HeaderMap)>, UpstreamError>;

    fn execute<'a>(
        &'a self,
        request: PreparedRequest,
        affinity_hint: Option<CacheAffinityHint>,
        forced_credential: Option<usize>,
        client: &'a wreq::Client,
        spoof_client: Option<&'a wreq::Client>,
    ) -> BoxFuture<'a, ExecutionOutcome<ProviderExecuteResult>>;

    fn execute_stream<'a>(
        &'a self,
        request: PreparedRequest,
        affinity_hint: Option<CacheAffinityHint>,
        forced_credential: Option<usize>,
        client: &'a wreq::Client,
        spoof_client: Option<&'a wreq::Client>,
    ) -> BoxFuture<'a, ExecutionOutcome<ProviderExecuteStreamResult>>;

    fn snapshot(&self) -> Result<ProviderSnapshot, UpstreamError>;

    fn credential_snapshot(
        &self,
        index: usize,
    ) -> Result<Option<CredentialSnapshot>, UpstreamError>;

    fn credential_snapshots(&self) -> Result<Vec<CredentialSnapshot>, UpstreamError>;

    fn set_settings_json(&self, settings: Value) -> Result<(), UpstreamError>;

    fn add_credential_json(&self, credential: Value) -> Result<CredentialSnapshot, UpstreamError>;

    fn update_credential_json(
        &self,
        index: usize,
        credential: Value,
    ) -> Result<Option<CredentialSnapshot>, UpstreamError>;

    fn remove_credential_json(
        &self,
        index: usize,
    ) -> Result<Option<CredentialSnapshot>, UpstreamError>;

    fn apply_credential_update(&self, update: &CredentialUpdate) -> Result<bool, UpstreamError>;

    fn apply_credential_updates(
        &self,
        updates: &[CredentialUpdate],
    ) -> Result<Vec<bool>, UpstreamError>;

    fn prepare_quota_request(
        &self,
        credential_index: Option<usize>,
    ) -> Result<Option<http::Request<Vec<u8>>>, UpstreamError>;

    /// Refresh a credential's auth token (e.g. via OAuth refresh_token) and
    /// update the in-memory store. Returns the credential update for DB
    /// persistence, or `None` if the refresh failed or the credential has
    /// no refresh mechanism.
    fn refresh_credential_at<'a>(
        &'a self,
        credential_index: Option<usize>,
        client: &'a wreq::Client,
    ) -> BoxFuture<'a, Result<Option<CredentialUpdate>, UpstreamError>>;

    fn health_snapshots(&self) -> Vec<CredentialHealthSnapshot>;

    /// Manually mark a credential as dead (admin override).
    fn mark_credential_dead(&self, index: usize);

    /// Manually reset a credential to healthy (admin override).
    fn mark_credential_healthy(&self, index: usize);

    fn oauth_start<'a>(
        &'a self,
        client: &'a wreq::Client,
        params: &'a HashMap<String, String>,
    ) -> BoxFuture<'a, Result<Option<OAuthFlow>, UpstreamError>>;

    fn oauth_finish<'a>(
        &'a self,
        client: &'a wreq::Client,
        params: &'a HashMap<String, String>,
    ) -> BoxFuture<'a, Result<Option<(Value, Value)>, UpstreamError>>;
}

struct ProviderInstance<C: Channel> {
    name: String,
    channel: C,
    model_pricing: &'static [crate::billing::ModelPrice],
    settings: ArcSwap<C::Settings>,
    credentials: ArcSwap<Vec<C::Credential>>,
    health: Mutex<Vec<C::Health>>,
    dispatch_table: DispatchTable,
    affinity_pool: CacheAffinityPool,
    round_robin_cursor: AtomicUsize,
    credential_revision: AtomicU64,
}

impl<C: Channel> ProviderInstance<C> {
    fn new(
        name: String,
        channel: C,
        settings: C::Settings,
        credentials: Vec<(C::Credential, C::Health)>,
        dispatch_override: Option<DispatchTable>,
    ) -> Self {
        let (credential_values, health_values): (Vec<_>, Vec<_>) = credentials.into_iter().unzip();
        Self {
            name,
            model_pricing: channel.model_pricing(),
            dispatch_table: dispatch_override.unwrap_or_else(|| channel.dispatch_table()),
            channel,
            settings: ArcSwap::from_pointee(settings),
            credentials: ArcSwap::from_pointee(credential_values),
            health: Mutex::new(health_values),
            affinity_pool: CacheAffinityPool::new(DEFAULT_CACHE_AFFINITY_MAX_KEYS),
            round_robin_cursor: AtomicUsize::new(0),
            credential_revision: AtomicU64::new(0),
        }
    }

    fn align_health_len(&self, target_len: usize) -> Vec<C::Health> {
        let mut guard = self.health.lock().unwrap();
        if guard.len() < target_len {
            guard.resize_with(target_len, Default::default);
        } else if guard.len() > target_len {
            guard.truncate(target_len);
        }
        guard.clone()
    }

    fn store_health_if_snapshot_unchanged(
        &self,
        credentials_snapshot: &Arc<Vec<C::Credential>>,
        healths: Vec<C::Health>,
        revision: u64,
    ) {
        let current_snapshot = self.credentials.load_full();
        let current_revision = self
            .credential_revision
            .load(std::sync::atomic::Ordering::SeqCst);
        if current_revision != revision || !Arc::ptr_eq(&current_snapshot, credentials_snapshot) {
            return;
        }

        let mut guard = self.health.lock().unwrap();
        *guard = healths;
    }

    fn credential_snapshot_from_value(
        &self,
        index: usize,
        revision: u64,
        credential: &C::Credential,
    ) -> Result<CredentialSnapshot, UpstreamError> {
        Ok(CredentialSnapshot {
            provider: self.name.clone(),
            index,
            revision,
            credential: serde_json::to_value(credential)
                .map_err(|e| UpstreamError::Channel(format!("serialize credential: {e}")))?,
        })
    }

    fn prepare_retry_state(&self) -> RetryState<C::Credential, C::Health> {
        let settings = self.settings.load_full();
        let credentials_snapshot = self.credentials.load_full();
        let revision = self
            .credential_revision
            .load(std::sync::atomic::Ordering::SeqCst);
        let health_snapshot = self.align_health_len(credentials_snapshot.len());
        let creds: Vec<(C::Credential, C::Health)> = credentials_snapshot
            .iter()
            .cloned()
            .zip(health_snapshot)
            .collect();
        let max_retries = settings.max_retries_on_429();
        (credentials_snapshot, revision, creds, max_retries)
    }

    fn finalize_credentials(
        &self,
        credentials_snapshot: &Arc<Vec<C::Credential>>,
        revision: u64,
        creds: &[(C::Credential, C::Health)],
    ) -> Result<Vec<CredentialUpdate>, UpstreamError> {
        let updated_health: Vec<C::Health> =
            creds.iter().map(|(_, health)| health.clone()).collect();
        self.store_health_if_snapshot_unchanged(credentials_snapshot, updated_health, revision);

        let mut credential_updates = Vec::new();
        for (index, ((updated_credential, _), original_credential)) in
            creds.iter().zip(credentials_snapshot.iter()).enumerate()
        {
            let original_json = serde_json::to_value(original_credential)
                .map_err(|e| UpstreamError::Channel(format!("serialize credential: {e}")))?;
            let updated_json = serde_json::to_value(updated_credential)
                .map_err(|e| UpstreamError::Channel(format!("serialize credential: {e}")))?;
            if original_json != updated_json {
                credential_updates.push(CredentialUpdate {
                    provider: self.name.clone(),
                    index,
                    revision,
                    credential: updated_json,
                });
            }
        }

        // Apply refreshed credentials to in-memory ArcSwap so subsequent
        // requests use the new tokens instead of re-refreshing every time.
        if !credential_updates.is_empty() {
            let _ = self.apply_credential_updates(&credential_updates);
        }

        Ok(credential_updates)
    }
}

impl<C: Channel> ProviderRuntime for ProviderInstance<C> {
    fn dispatch_table(&self) -> &DispatchTable {
        &self.dispatch_table
    }

    fn build_billing_context(
        &self,
        request: &PreparedRequest,
    ) -> Option<crate::billing::BillingContext> {
        crate::billing::build_billing_context(C::ID, request)
    }

    fn estimate_billing(
        &self,
        context: &crate::billing::BillingContext,
        usage: &crate::engine::Usage,
    ) -> Option<crate::billing::BillingResult> {
        crate::billing::estimate_billing(self.model_pricing, context, usage)
    }

    fn handle_local(
        &self,
        operation: OperationFamily,
        protocol: ProtocolKind,
        body: &[u8],
    ) -> Option<Result<Vec<u8>, UpstreamError>> {
        self.channel.handle_local(operation, protocol, body)
    }

    fn finalize_request(&self, request: PreparedRequest) -> Result<PreparedRequest, UpstreamError> {
        let settings = self.settings.load();
        self.channel.finalize_request(&settings, request)
    }

    fn normalize_response(&self, request: &PreparedRequest, body: Vec<u8>) -> Vec<u8> {
        self.channel.normalize_response(request, body)
    }

    fn sanitize_rules(&self) -> Vec<crate::utils::sanitize::SanitizeRule> {
        self.settings.load().sanitize_rules().to_vec()
    }

    fn rewrite_rules(&self) -> Vec<crate::utils::rewrite::RewriteRule> {
        self.settings.load().rewrite_rules().to_vec()
    }

    fn enable_suffix(&self) -> bool {
        self.settings.load().enable_suffix()
    }

    fn prepare_ws_auth(
        &self,
        path: &str,
        model: Option<&str>,
    ) -> Result<Vec<(String, http::HeaderMap)>, UpstreamError> {
        let settings = self.settings.load();
        let credentials = self.credentials.load();
        if credentials.is_empty() {
            return Err(UpstreamError::Channel(
                "no credentials available for WebSocket auth".into(),
            ));
        }

        let dummy = PreparedRequest {
            method: http::Method::GET,
            route: if path == "/v1/responses" {
                RouteKey::new(
                    OperationFamily::OpenAiResponseWebSocket,
                    ProtocolKind::OpenAi,
                )
            } else {
                RouteKey::new(OperationFamily::GeminiLive, ProtocolKind::Gemini)
            },
            model: model.map(String::from),
            body: Vec::new(),
            headers: http::HeaderMap::new(),
        };

        let mut results = Vec::with_capacity(credentials.len());
        let ws_extra = self.channel.ws_extra_headers();
        for credential in credentials.iter() {
            let http_req = self
                .channel
                .prepare_request(credential, &settings, &dummy)?;
            let url = http_req.uri().to_string();
            let mut headers = http_req.headers().clone();
            headers.extend(ws_extra.clone());
            results.push((url, headers));
        }
        Ok(results)
    }

    fn execute<'a>(
        &'a self,
        request: PreparedRequest,
        affinity_hint: Option<CacheAffinityHint>,
        forced_credential: Option<usize>,
        client: &'a wreq::Client,
        spoof_client: Option<&'a wreq::Client>,
    ) -> BoxFuture<'a, ExecutionOutcome<ProviderExecuteResult>> {
        Box::pin(async move {
            let (credentials_snapshot, revision, mut creds, max_retries) =
                self.prepare_retry_state();
            let settings = self.settings.load_full();
            let effective_hint = if settings.enable_cache_affinity() {
                affinity_hint
            } else {
                None
            };
            let retry_result = retry_with_credentials(
                RetryContext {
                    channel: &self.channel,
                    credentials: &mut creds,
                    settings: &settings,
                    request: &request,
                    affinity_hint: effective_hint.as_ref(),
                    affinity_pool: &self.affinity_pool,
                    round_robin_cursor: &self.round_robin_cursor,
                    max_retries,
                    http_client: client,
                    spoof_client,
                    forced_credential,
                },
                |c, req| {
                    let c = c.clone();
                    async move { crate::http_client::send_request(&c, req).await }
                },
            )
            .await;

            let credential_updates =
                match self.finalize_credentials(&credentials_snapshot, revision, &creds) {
                    Ok(updates) => updates,
                    Err(e) => return ExecutionOutcome::err(e, None),
                };
            match retry_result {
                Ok(r) => ExecutionOutcome::ok(ProviderExecuteResult {
                    response: r.output,
                    credential_updates,
                    credential_index: r.credential_index,
                    attempt_meta: r.attempt_meta,
                }),
                Err(failure) => ExecutionOutcome::err(failure.error, failure.last_attempt),
            }
        })
    }

    fn execute_stream<'a>(
        &'a self,
        request: PreparedRequest,
        affinity_hint: Option<CacheAffinityHint>,
        forced_credential: Option<usize>,
        client: &'a wreq::Client,
        spoof_client: Option<&'a wreq::Client>,
    ) -> BoxFuture<'a, ExecutionOutcome<ProviderExecuteStreamResult>> {
        Box::pin(async move {
            let (credentials_snapshot, revision, mut creds, max_retries) =
                self.prepare_retry_state();
            let settings = self.settings.load_full();
            let effective_hint = if settings.enable_cache_affinity() {
                affinity_hint
            } else {
                None
            };
            let retry_result = retry_with_credentials_stream(
                RetryContext {
                    channel: &self.channel,
                    credentials: &mut creds,
                    settings: &settings,
                    request: &request,
                    affinity_hint: effective_hint.as_ref(),
                    affinity_pool: &self.affinity_pool,
                    round_robin_cursor: &self.round_robin_cursor,
                    max_retries,
                    http_client: client,
                    spoof_client,
                    forced_credential,
                },
                |c, req| {
                    let c = c.clone();
                    async move { crate::http_client::send_request_stream(&c, req).await }
                },
            )
            .await;

            let credential_updates =
                match self.finalize_credentials(&credentials_snapshot, revision, &creds) {
                    Ok(updates) => updates,
                    Err(e) => return ExecutionOutcome::err(e, None),
                };
            match retry_result {
                Ok(r) => ExecutionOutcome::ok(ProviderExecuteStreamResult {
                    response: r.output,
                    credential_updates,
                    credential_index: r.credential_index,
                    attempt_meta: r.attempt_meta,
                }),
                Err(failure) => ExecutionOutcome::err(failure.error, failure.last_attempt),
            }
        })
    }

    fn snapshot(&self) -> Result<ProviderSnapshot, UpstreamError> {
        let settings = self.settings.load();
        let credentials = self.credentials.load();
        let revision = self
            .credential_revision
            .load(std::sync::atomic::Ordering::SeqCst);
        Ok(ProviderSnapshot {
            name: self.name.clone(),
            channel: C::ID.to_string(),
            settings: serde_json::to_value(&**settings)
                .map_err(|e| UpstreamError::Channel(format!("serialize settings: {e}")))?,
            credential_count: credentials.len(),
            credential_revision: revision,
        })
    }

    fn credential_snapshot(
        &self,
        index: usize,
    ) -> Result<Option<CredentialSnapshot>, UpstreamError> {
        let credentials = self.credentials.load();
        let revision = self
            .credential_revision
            .load(std::sync::atomic::Ordering::SeqCst);
        let Some(credential) = credentials.get(index) else {
            return Ok(None);
        };
        self.credential_snapshot_from_value(index, revision, credential)
            .map(Some)
    }

    fn credential_snapshots(&self) -> Result<Vec<CredentialSnapshot>, UpstreamError> {
        let credentials = self.credentials.load();
        let revision = self
            .credential_revision
            .load(std::sync::atomic::Ordering::SeqCst);
        credentials
            .iter()
            .enumerate()
            .map(|(index, credential)| {
                self.credential_snapshot_from_value(index, revision, credential)
            })
            .collect()
    }

    fn set_settings_json(&self, settings: Value) -> Result<(), UpstreamError> {
        let parsed: C::Settings = serde_json::from_value(settings)
            .map_err(|e| UpstreamError::Channel(format!("deserialize settings: {e}")))?;
        self.settings.store(Arc::new(parsed));
        Ok(())
    }

    fn add_credential_json(&self, credential: Value) -> Result<CredentialSnapshot, UpstreamError> {
        let parsed: C::Credential = serde_json::from_value(credential)
            .map_err(|e| UpstreamError::Channel(format!("deserialize credential: {e}")))?;
        let mut current = (*self.credentials.load_full()).clone();
        current.push(parsed);
        let index = current.len() - 1;
        let revision = self
            .credential_revision
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1;
        let snapshot = self.credential_snapshot_from_value(index, revision, &current[index])?;
        self.credentials.store(Arc::new(current));
        self.health.lock().unwrap().push(C::Health::default());
        Ok(snapshot)
    }

    fn update_credential_json(
        &self,
        index: usize,
        credential: Value,
    ) -> Result<Option<CredentialSnapshot>, UpstreamError> {
        let parsed: C::Credential = serde_json::from_value(credential)
            .map_err(|e| UpstreamError::Channel(format!("deserialize credential: {e}")))?;
        let mut current = (*self.credentials.load_full()).clone();
        let Some(slot) = current.get_mut(index) else {
            return Ok(None);
        };
        *slot = parsed;
        let revision = self
            .credential_revision
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1;
        let snapshot = self.credential_snapshot_from_value(index, revision, slot)?;
        self.credentials.store(Arc::new(current));
        Ok(Some(snapshot))
    }

    fn remove_credential_json(
        &self,
        index: usize,
    ) -> Result<Option<CredentialSnapshot>, UpstreamError> {
        let mut current = (*self.credentials.load_full()).clone();
        if index >= current.len() {
            return Ok(None);
        }
        let revision = self
            .credential_revision
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1;
        let removed = current.remove(index);
        self.credentials.store(Arc::new(current));
        let mut health = self.health.lock().unwrap();
        if index < health.len() {
            health.remove(index);
        }
        self.credential_snapshot_from_value(index, revision, &removed)
            .map(Some)
    }

    fn apply_credential_update(&self, update: &CredentialUpdate) -> Result<bool, UpstreamError> {
        self.apply_credential_updates(std::slice::from_ref(update))
            .map(|results| results.into_iter().next().unwrap_or(false))
    }

    fn apply_credential_updates(
        &self,
        updates: &[CredentialUpdate],
    ) -> Result<Vec<bool>, UpstreamError> {
        if updates.is_empty() {
            return Ok(Vec::new());
        }

        let current_revision = self
            .credential_revision
            .load(std::sync::atomic::Ordering::SeqCst);
        if updates
            .iter()
            .any(|update| update.revision != current_revision)
        {
            return Ok(vec![false; updates.len()]);
        }

        let mut current = (*self.credentials.load_full()).clone();
        let mut applied = vec![false; updates.len()];

        for (position, update) in updates.iter().enumerate() {
            let Some(credential) = current.get_mut(update.index) else {
                continue;
            };

            let mut patch_target = credential.clone();
            if !patch_target.apply_update(&update.credential) {
                patch_target = serde_json::from_value(update.credential.clone())
                    .map_err(|e| UpstreamError::Channel(format!("deserialize credential: {e}")))?;
            }
            *credential = patch_target;
            applied[position] = true;
        }

        self.credentials.store(Arc::new(current));
        self.credential_revision
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(applied)
    }

    fn prepare_quota_request(
        &self,
        credential_index: Option<usize>,
    ) -> Result<Option<http::Request<Vec<u8>>>, UpstreamError> {
        let settings = self.settings.load();
        let credentials = self.credentials.load();
        let credential = match credential_index {
            Some(index) => credentials.get(index).ok_or_else(|| {
                UpstreamError::Channel(format!("credential index not found: {index}"))
            })?,
            None => match credentials.first() {
                Some(credential) => credential,
                None => return Ok(None),
            },
        };
        self.channel.prepare_quota_request(credential, &settings)
    }

    fn refresh_credential_at<'a>(
        &'a self,
        credential_index: Option<usize>,
        client: &'a wreq::Client,
    ) -> BoxFuture<'a, Result<Option<CredentialUpdate>, UpstreamError>> {
        Box::pin(async move {
            let credentials = self.credentials.load_full();
            let revision = self
                .credential_revision
                .load(std::sync::atomic::Ordering::SeqCst);
            let index = credential_index.unwrap_or(0);
            let Some(original) = credentials.get(index) else {
                return Ok(None);
            };

            let mut credential = original.clone();
            let refreshed = self
                .channel
                .refresh_credential(client, &mut credential)
                .await
                .unwrap_or(false);

            if !refreshed {
                return Ok(None);
            }

            let credential_json = serde_json::to_value(&credential)
                .map_err(|e| UpstreamError::Channel(format!("serialize credential: {e}")))?;

            let update = CredentialUpdate {
                provider: self.name.clone(),
                index,
                revision,
                credential: credential_json,
            };

            // Apply to in-memory ArcSwap
            let _ = self.apply_credential_update(&update);

            Ok(Some(update))
        })
    }

    fn health_snapshots(&self) -> Vec<CredentialHealthSnapshot> {
        let health_guard = self.health.lock().unwrap();
        health_guard
            .iter()
            .enumerate()
            .map(|(index, h)| {
                let available = h.is_available(None);
                let status = h.status(None).to_string();
                CredentialHealthSnapshot {
                    provider: self.name.clone(),
                    index,
                    status,
                    available,
                }
            })
            .collect()
    }

    fn mark_credential_dead(&self, index: usize) {
        let mut health_guard = self.health.lock().unwrap();
        if let Some(h) = health_guard.get_mut(index) {
            h.record_error(401, None, None);
        }
    }

    fn mark_credential_healthy(&self, index: usize) {
        let mut health_guard = self.health.lock().unwrap();
        if let Some(h) = health_guard.get_mut(index) {
            h.record_success(None);
        }
    }

    fn oauth_start<'a>(
        &'a self,
        client: &'a wreq::Client,
        params: &'a HashMap<String, String>,
    ) -> BoxFuture<'a, Result<Option<OAuthFlow>, UpstreamError>> {
        Box::pin(async move {
            let settings = self.settings.load();
            let params = params.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            self.channel.oauth_start(client, &settings, &params).await
        })
    }

    fn oauth_finish<'a>(
        &'a self,
        client: &'a wreq::Client,
        params: &'a HashMap<String, String>,
    ) -> BoxFuture<'a, Result<Option<(Value, Value)>, UpstreamError>> {
        Box::pin(async move {
            let settings = self.settings.load();
            let params = params.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            let result: Option<OAuthCredentialResult<C::Credential>> = self
                .channel
                .oauth_finish(client, &settings, &params)
                .await?;
            result
                .map(|result| {
                    serde_json::to_value(result.credential)
                        .map(|credential| (credential, result.details))
                        .map_err(|e| UpstreamError::Channel(format!("serialize credential: {e}")))
                })
                .transpose()
        })
    }
}

#[derive(Default)]
pub struct ProviderStoreBuilder {
    providers: HashMap<String, Arc<dyn ProviderRuntime>>,
}

impl ProviderStoreBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_provider<C: Channel>(
        self,
        name: impl Into<String>,
        channel: C,
        settings: C::Settings,
        credentials: Vec<(C::Credential, C::Health)>,
    ) -> Self {
        self.add_provider_with_dispatch(name, channel, settings, credentials, None)
    }

    pub fn add_provider_with_dispatch<C: Channel>(
        mut self,
        name: impl Into<String>,
        channel: C,
        settings: C::Settings,
        credentials: Vec<(C::Credential, C::Health)>,
        dispatch_override: Option<DispatchTable>,
    ) -> Self {
        let name = name.into();
        let provider = Arc::new(ProviderInstance::new(
            name.clone(),
            channel,
            settings,
            credentials,
            dispatch_override,
        ));
        self.providers.insert(name, provider);
        self
    }

    pub fn build(self) -> ProviderStore {
        let providers = DashMap::with_capacity(self.providers.len());
        for (name, provider) in self.providers {
            providers.insert(name, provider);
        }
        let (event_tx, _) = broadcast::channel(64);
        ProviderStore {
            providers,
            event_tx,
        }
    }
}

pub struct ProviderStore {
    providers: DashMap<String, Arc<dyn ProviderRuntime>>,
    event_tx: broadcast::Sender<EngineEvent>,
}

impl ProviderStore {
    pub fn builder() -> ProviderStoreBuilder {
        ProviderStoreBuilder::new()
    }

    pub fn add_provider<C: Channel>(
        &self,
        name: impl Into<String>,
        channel: C,
        settings: C::Settings,
        credentials: Vec<(C::Credential, C::Health)>,
    ) {
        self.add_provider_with_dispatch(name, channel, settings, credentials, None);
    }

    pub fn add_provider_with_dispatch<C: Channel>(
        &self,
        name: impl Into<String>,
        channel: C,
        settings: C::Settings,
        credentials: Vec<(C::Credential, C::Health)>,
        dispatch_override: Option<DispatchTable>,
    ) {
        let name = name.into();
        let provider = Arc::new(ProviderInstance::new(
            name.clone(),
            channel,
            settings,
            credentials,
            dispatch_override,
        ));
        self.providers.insert(name.clone(), provider);
        self.emit_event(EngineEvent::ProviderAdded { name });
    }

    /// Add or replace a provider from serialized JSON config.
    pub fn add_provider_json(
        &self,
        config: crate::engine::ProviderConfig,
    ) -> Result<(), UpstreamError> {
        macro_rules! add {
            ($self:expr, $ch:expr, $cfg:expr) => {{
                let crate::engine::ProviderConfig {
                    name,
                    settings_json,
                    credentials,
                    dispatch,
                    ..
                } = $cfg;
                let dispatch = match dispatch {
                    Some(document) => Some(
                        crate::dispatch::DispatchTable::from_document(document).map_err(|e| {
                            UpstreamError::Channel(format!("invalid dispatch for '{}': {e}", name))
                        })?,
                    ),
                    None => None,
                };
                let settings = serde_json::from_value(settings_json).map_err(|e| {
                    UpstreamError::Channel(format!("invalid settings for '{}': {e}", name))
                })?;
                let creds: Vec<_> = credentials
                    .into_iter()
                    .filter_map(|c| {
                        serde_json::from_value(c)
                            .ok()
                            .map(|c| (c, crate::health::ModelCooldownHealth::default()))
                    })
                    .collect();
                $self.add_provider_with_dispatch(&name, $ch, settings, creds, dispatch);
                Ok(())
            }};
        }

        use crate::channels::*;

        match config.channel.as_str() {
            "openai" => add!(self, openai::OpenAiChannel, config),
            "anthropic" => add!(self, anthropic::AnthropicChannel, config),
            "claudecode" => add!(self, claudecode::ClaudeCodeChannel, config),
            "codex" => add!(self, codex::CodexChannel, config),
            "vertex" => add!(self, vertex::VertexChannel, config),
            "vertexexpress" => add!(self, vertexexpress::VertexExpressChannel, config),
            "aistudio" => add!(self, aistudio::AiStudioChannel, config),
            "geminicli" => add!(self, geminicli::GeminiCliChannel, config),
            "antigravity" => add!(self, antigravity::AntigravityChannel, config),
            "nvidia" => add!(self, nvidia::NvidiaChannel, config),
            "deepseek" => add!(self, deepseek::DeepSeekChannel, config),
            "groq" => add!(self, groq::GroqChannel, config),
            "openrouter" => add!(self, openrouter::OpenRouterChannel, config),
            "custom" => add!(self, custom::CustomChannel, config),
            _ => Err(UpstreamError::Channel(format!(
                "unknown channel: {}",
                config.channel
            ))),
        }
    }

    pub fn remove_provider(&self, name: &str) -> bool {
        let removed = self.providers.remove(name).is_some();
        if removed {
            self.emit_event(EngineEvent::ProviderRemoved {
                name: name.to_string(),
            });
        }
        removed
    }

    pub fn list_providers(&self) -> Result<Vec<ProviderSnapshot>, UpstreamError> {
        self.providers
            .iter()
            .map(|entry| entry.value().snapshot())
            .collect()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<EngineEvent> {
        self.event_tx.subscribe()
    }

    /// Get health status for all credentials across all providers.
    pub fn list_health(&self, provider_name: Option<&str>) -> Vec<CredentialHealthSnapshot> {
        let mut out = Vec::new();
        for entry in self.providers.iter() {
            if provider_name.is_some_and(|filter| filter != entry.key().as_str()) {
                continue;
            }
            out.extend(entry.value().health_snapshots());
        }
        out
    }

    /// Manually mark a credential as dead.
    pub fn mark_credential_dead(&self, provider_name: &str, index: usize) -> bool {
        if let Some(provider) = self.get_runtime(provider_name) {
            provider.mark_credential_dead(index);
            self.emit_health_change(provider_name, index, &provider);
            true
        } else {
            false
        }
    }

    /// Manually reset a credential to healthy.
    pub fn mark_credential_healthy(&self, provider_name: &str, index: usize) -> bool {
        if let Some(provider) = self.get_runtime(provider_name) {
            provider.mark_credential_healthy(index);
            self.emit_health_change(provider_name, index, &provider);
            true
        } else {
            false
        }
    }

    pub fn get_provider(&self, name: &str) -> Result<Option<ProviderSnapshot>, UpstreamError> {
        let Some(provider) = self.get_runtime(name) else {
            return Ok(None);
        };
        provider.snapshot().map(Some)
    }

    pub fn list_credentials(
        &self,
        provider_name: Option<&str>,
    ) -> Result<Vec<CredentialSnapshot>, UpstreamError> {
        let mut out = Vec::new();
        for entry in self.providers.iter() {
            if provider_name.is_some_and(|filter| filter != entry.key().as_str()) {
                continue;
            }
            out.extend(entry.value().credential_snapshots()?);
        }
        Ok(out)
    }

    pub fn get_credential(
        &self,
        provider_name: &str,
        index: usize,
    ) -> Result<Option<CredentialSnapshot>, UpstreamError> {
        let Some(provider) = self.get_runtime(provider_name) else {
            return Ok(None);
        };
        provider.credential_snapshot(index)
    }

    pub fn update_provider_settings(
        &self,
        provider_name: &str,
        settings: Value,
    ) -> Result<bool, UpstreamError> {
        let Some(provider) = self.get_runtime(provider_name) else {
            return Ok(false);
        };
        provider.set_settings_json(settings)?;
        self.emit_provider_updated(provider_name);
        Ok(true)
    }

    pub fn add_credential(
        &self,
        provider_name: &str,
        credential: Value,
    ) -> Result<Option<CredentialSnapshot>, UpstreamError> {
        let Some(provider) = self.get_runtime(provider_name) else {
            return Ok(None);
        };
        let result = provider.add_credential_json(credential).map(Some);
        if result.as_ref().is_ok_and(Option::is_some) {
            self.emit_provider_updated(provider_name);
        }
        tracing::info!(provider = provider_name, "credential added");
        result
    }

    pub fn update_credential(
        &self,
        provider_name: &str,
        index: usize,
        credential: Value,
    ) -> Result<Option<CredentialSnapshot>, UpstreamError> {
        let Some(provider) = self.get_runtime(provider_name) else {
            return Ok(None);
        };
        let result = provider.update_credential_json(index, credential);
        if result.as_ref().is_ok_and(Option::is_some) {
            self.emit_provider_updated(provider_name);
        }
        tracing::info!(provider = provider_name, index, "credential updated");
        result
    }

    pub fn remove_credential(
        &self,
        provider_name: &str,
        index: usize,
    ) -> Result<Option<CredentialSnapshot>, UpstreamError> {
        let Some(provider) = self.get_runtime(provider_name) else {
            return Ok(None);
        };
        let result = provider.remove_credential_json(index);
        if result.as_ref().is_ok_and(Option::is_some) {
            self.emit_provider_updated(provider_name);
        }
        tracing::info!(provider = provider_name, index, "credential removed");
        result
    }

    pub fn apply_credential_update(
        &self,
        update: &CredentialUpdate,
    ) -> Result<bool, UpstreamError> {
        let Some(provider) = self.get_runtime(&update.provider) else {
            return Ok(false);
        };
        let applied = provider.apply_credential_update(update)?;
        if applied {
            self.emit_provider_updated(&update.provider);
        }
        Ok(applied)
    }

    pub fn apply_credential_updates(
        &self,
        updates: &[CredentialUpdate],
    ) -> Result<Vec<bool>, UpstreamError> {
        let mut grouped: HashMap<(String, u64), Vec<(usize, CredentialUpdate)>> = HashMap::new();
        for (index, update) in updates.iter().cloned().enumerate() {
            grouped
                .entry((update.provider.clone(), update.revision))
                .or_default()
                .push((index, update));
        }

        let mut results = vec![false; updates.len()];
        for ((provider_name, _revision), entries) in grouped {
            let Some(provider) = self.get_runtime(&provider_name) else {
                continue;
            };
            let batch: Vec<CredentialUpdate> =
                entries.iter().map(|(_, update)| update.clone()).collect();
            let batch_results = provider.apply_credential_updates(&batch)?;
            let mut provider_updated = false;
            for ((original_index, _), applied) in entries.into_iter().zip(batch_results.into_iter())
            {
                results[original_index] = applied;
                provider_updated |= applied;
            }
            if provider_updated {
                self.emit_provider_updated(&provider_name);
            }
        }
        Ok(results)
    }

    pub async fn oauth_start(
        &self,
        provider_name: &str,
        client: &wreq::Client,
        params: HashMap<String, String>,
    ) -> Result<Option<OAuthFlow>, UpstreamError> {
        tracing::info!(provider = provider_name, "oauth flow started");
        let Some(provider) = self.get_runtime(provider_name) else {
            return Ok(None);
        };
        provider.oauth_start(client, &params).await
    }

    pub async fn oauth_finish(
        &self,
        provider_name: &str,
        client: &wreq::Client,
        params: HashMap<String, String>,
    ) -> Result<Option<OAuthFinishResult>, UpstreamError> {
        let Some(provider) = self.get_runtime(provider_name) else {
            return Ok(None);
        };
        let Some((credential_json, details)) = provider.oauth_finish(client, &params).await? else {
            return Ok(None);
        };
        let Some(credential) = self.add_credential(provider_name, credential_json)? else {
            return Ok(None);
        };
        tracing::info!(provider = provider_name, "oauth flow completed");
        Ok(Some(OAuthFinishResult {
            credential,
            details,
        }))
    }

    /// Get the dispatch table for a named provider.
    pub fn get_dispatch_table(&self, name: &str) -> Option<DispatchTable> {
        self.providers
            .get(name)
            .map(|entry| entry.value().dispatch_table().clone())
    }

    pub fn estimate_billing(
        &self,
        provider_name: &str,
        context: &crate::billing::BillingContext,
        usage: &crate::engine::Usage,
    ) -> Option<crate::billing::BillingResult> {
        self.get_runtime(provider_name)
            .and_then(|provider| provider.estimate_billing(context, usage))
    }

    pub(crate) fn get_runtime(&self, name: &str) -> Option<Arc<dyn ProviderRuntime>> {
        self.providers
            .get(name)
            .map(|entry| Arc::clone(entry.value()))
    }

    fn emit_event(&self, event: EngineEvent) {
        let _ = self.event_tx.send(event);
    }

    fn emit_provider_updated(&self, name: &str) {
        self.emit_event(EngineEvent::ProviderUpdated {
            name: name.to_string(),
        });
    }

    fn emit_health_change(
        &self,
        provider_name: &str,
        index: usize,
        provider: &Arc<dyn ProviderRuntime>,
    ) {
        let Some(snapshot) = provider
            .health_snapshots()
            .into_iter()
            .find(|snapshot| snapshot.index == index)
        else {
            return;
        };

        self.emit_event(EngineEvent::CredentialHealthChanged {
            provider: provider_name.to_string(),
            index,
            status: snapshot.status,
        });
    }
}

impl ProviderRegistry for ProviderStore {
    fn get_provider(&self, name: &str) -> Result<Option<ProviderSnapshot>, UpstreamError> {
        ProviderStore::get_provider(self, name)
    }

    fn list_providers(&self) -> Result<Vec<ProviderSnapshot>, UpstreamError> {
        ProviderStore::list_providers(self)
    }

    fn get_credential(
        &self,
        provider_name: &str,
        index: usize,
    ) -> Result<Option<CredentialSnapshot>, UpstreamError> {
        ProviderStore::get_credential(self, provider_name, index)
    }

    fn list_credentials(
        &self,
        provider_name: Option<&str>,
    ) -> Result<Vec<CredentialSnapshot>, UpstreamError> {
        ProviderStore::list_credentials(self, provider_name)
    }
}

impl ProviderMutator for ProviderStore {
    fn upsert_provider_json(
        &self,
        config: crate::engine::ProviderConfig,
    ) -> Result<(), UpstreamError> {
        ProviderStore::add_provider_json(self, config)
    }

    fn remove_provider(&self, name: &str) -> bool {
        ProviderStore::remove_provider(self, name)
    }

    fn upsert_credential_json(
        &self,
        provider_name: &str,
        index: Option<usize>,
        credential: Value,
    ) -> Result<Option<CredentialSnapshot>, UpstreamError> {
        match index {
            Some(index) => ProviderStore::update_credential(self, provider_name, index, credential),
            None => ProviderStore::add_credential(self, provider_name, credential),
        }
    }

    fn remove_credential(
        &self,
        provider_name: &str,
        index: usize,
    ) -> Result<Option<CredentialSnapshot>, UpstreamError> {
        ProviderStore::remove_credential(self, provider_name, index)
    }
}

impl EngineEventSource for ProviderStore {
    fn subscribe(&self) -> broadcast::Receiver<EngineEvent> {
        ProviderStore::subscribe(self)
    }
}

#[cfg(test)]
mod tests {
    use super::ProviderStore;
    use crate::channels::codex::{CodexChannel, CodexSettings};
    use crate::health::ModelCooldownHealth;

    #[test]
    fn prepare_quota_request_uses_selected_credential_index() {
        let store = ProviderStore::builder()
            .add_provider(
                "demo",
                CodexChannel,
                CodexSettings::default(),
                vec![
                    (
                        crate::channels::codex::CodexCredential {
                            access_token: "token-1".to_string(),
                            ..Default::default()
                        },
                        ModelCooldownHealth::default(),
                    ),
                    (
                        crate::channels::codex::CodexCredential {
                            access_token: "token-2".to_string(),
                            ..Default::default()
                        },
                        ModelCooldownHealth::default(),
                    ),
                ],
            )
            .build();

        let runtime = store.get_runtime("demo").expect("runtime");
        let request = runtime
            .prepare_quota_request(Some(1))
            .expect("quota request")
            .expect("http request");

        let auth = request
            .headers()
            .get("Authorization")
            .and_then(|value| value.to_str().ok())
            .expect("authorization header");
        assert_eq!(auth, "Bearer token-2");
    }
}
