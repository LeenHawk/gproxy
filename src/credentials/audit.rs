//! Shared wire auditing for purpose-grouped upstream call sequences.

use std::sync::{Arc, Mutex};

use bytes::Bytes;
use serde_json::Value;

use crate::http::client::{ClientError, UpstreamClient};
use crate::http::redaction::{body_string, headers_json, redact_query};
use crate::store::persistence::PersistenceBackend;
use crate::store::persistence::records::{Credential, UpstreamRequestInput};

#[derive(Clone)]
struct ObservedCall {
    at: i64,
    url: String,
    method: String,
    status: i64,
    latency_ms: i64,
    headers_json: Option<Value>,
    body: Option<String>,
    response_body: Option<String>,
}

struct ObservingClient {
    inner: Arc<dyn UpstreamClient>,
    observed: Arc<Mutex<Vec<ObservedCall>>>,
    redact: bool,
    capture_body: bool,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl UpstreamClient for ObservingClient {
    async fn send(&self, req: http::Request<Bytes>) -> Result<http::Response<Bytes>, ClientError> {
        let mut call = ObservedCall {
            at: crate::util::time::unix_now(),
            url: audit_url(req.uri(), self.redact),
            method: req.method().to_string(),
            status: 0,
            latency_ms: 0,
            headers_json: Some(headers_json(req.headers(), self.redact)),
            body: self
                .capture_body
                .then(|| body_string(req.body(), self.redact)),
            response_body: None,
        };
        #[cfg(not(target_arch = "wasm32"))]
        let started = std::time::Instant::now();
        let response = self.inner.send(req).await;
        #[cfg(not(target_arch = "wasm32"))]
        {
            call.latency_ms = started.elapsed().as_millis().min(i64::MAX as u128) as i64;
        }
        if let Ok(response) = &response {
            call.status = i64::from(response.status().as_u16());
            call.response_body = self
                .capture_body
                .then(|| body_string(response.body(), self.redact));
        }
        lock(&self.observed).push(call);
        response
    }
}

/// A purpose-grouped sequence with one persisted row per observed HTTP call.
pub(crate) struct UpstreamAuditSequence<'a> {
    enabled: bool,
    persistence: &'a dyn PersistenceBackend,
    request_id: String,
    provider_id: Option<i64>,
    credential_id: Option<i64>,
    observed: Arc<Mutex<Vec<ObservedCall>>>,
    redact: bool,
    capture_body: bool,
}

impl<'a> UpstreamAuditSequence<'a> {
    pub(super) fn new(
        purpose: &str,
        enabled: bool,
        persistence: &'a dyn PersistenceBackend,
        credential: &Credential,
        enable_upstream_log_body: bool,
        disable_log_redaction: bool,
    ) -> Self {
        let at = crate::util::time::unix_now();
        let redact = if enabled {
            crate::http::redaction::warn_if_redaction_disabled(disable_log_redaction)
        } else {
            !disable_log_redaction
        };
        Self {
            enabled,
            persistence,
            request_id: format!(
                "{purpose}:{}:{at}:{}",
                credential.id,
                crate::util::rand::uuid_v4()
            ),
            provider_id: Some(credential.provider_id),
            credential_id: Some(credential.id),
            observed: Arc::new(Mutex::new(Vec::new())),
            redact,
            capture_body: enable_upstream_log_body,
        }
    }

    /// Create an audit sequence before a login has produced a credential.
    pub(crate) fn for_login(
        enabled: bool,
        persistence: &'a dyn PersistenceBackend,
        channel: &str,
        provider_id: Option<i64>,
        request_id: Option<&str>,
        enable_upstream_log_body: bool,
        disable_log_redaction: bool,
    ) -> Self {
        let at = crate::util::time::unix_now();
        let redact = if enabled {
            crate::http::redaction::warn_if_redaction_disabled(disable_log_redaction)
        } else {
            !disable_log_redaction
        };
        Self {
            enabled,
            persistence,
            request_id: request_id.map(str::to_owned).unwrap_or_else(|| {
                format!("login:{channel}:{at}:{}", crate::util::rand::uuid_v4())
            }),
            provider_id,
            credential_id: None,
            observed: Arc::new(Mutex::new(Vec::new())),
            redact,
            capture_body: enable_upstream_log_body,
        }
    }

    pub(crate) fn wrap_client(&self, client: Arc<dyn UpstreamClient>) -> Arc<dyn UpstreamClient> {
        if !self.enabled {
            return client;
        }
        Arc::new(ObservingClient {
            inner: client,
            observed: Arc::clone(&self.observed),
            redact: self.redact,
            capture_body: self.capture_body,
        })
    }

    pub(crate) fn request_id(&self) -> &str {
        &self.request_id
    }

    pub(crate) async fn persist(&self, sequence_error: Option<&str>) {
        if !self.enabled {
            return;
        }
        let calls = lock(&self.observed).clone();
        let last = calls.len().checked_sub(1);
        for (index, call) in calls.into_iter().enumerate() {
            let fallback_error = if Some(index) == last && self.capture_body {
                sequence_error.map(|error| error_body(error, self.redact))
            } else {
                None
            };
            let response_body = call.response_body.or(fallback_error);
            let input = UpstreamRequestInput {
                request_id: self.request_id.clone(),
                at: call.at,
                provider_id: self.provider_id,
                credential_id: self.credential_id,
                url: call.url,
                method: call.method,
                status: call.status,
                latency_ms: call.latency_ms,
                headers_json: call.headers_json,
                body: call.body,
                // The overall error belongs only to the last call and only
                // when no actual HTTP response payload was captured.
                response_body,
            };
            if let Err(error) = self.persistence.append_upstream_request(input).await {
                tracing::warn!(
                    credential_id = ?self.credential_id,
                    provider_id = ?self.provider_id,
                    request_id = self.request_id,
                    call_index = index,
                    error = %error,
                    "upstream audit write failed"
                );
            }
        }
    }
}

fn lock(observed: &Mutex<Vec<ObservedCall>>) -> std::sync::MutexGuard<'_, Vec<ObservedCall>> {
    observed
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn error_body(error: &str, redact: bool) -> String {
    let error = if redact {
        crate::http::telemetry::redact_url_query(error)
    } else {
        std::borrow::Cow::Borrowed(error)
    };
    body_string(error.as_bytes(), redact)
}

fn audit_url(uri: &http::Uri, redact: bool) -> String {
    let url = uri.to_string();
    match uri.query() {
        Some(query) => {
            let prefix = &url[..url.len() - query.len()];
            format!("{prefix}{}", redact_query(query, redact))
        }
        None => url,
    }
}

#[cfg(all(test, not(target_arch = "wasm32"), feature = "persist-db"))]
#[path = "audit_tests.rs"]
mod tests;
