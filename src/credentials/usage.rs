//! On-demand per-credential upstream usage fetch (§17). Resolves the
//! credential's pooled client (the SAME proxy + TLS identity its traffic uses),
//! ensures the OAuth secret is fresh, then runs the channel's
//! `prepare_usage_request` → `parse_usage`. Admin-triggered and infrequent — not
//! on the request hot path; the orchestration mirrors [`super::refresh`].

use std::sync::Arc;

use crate::app::AppState;
use crate::channel::{Channel, ChannelError, UsageSnapshot};
use crate::http::client::UpstreamClient;
use crate::store::persistence::records::{Credential, Provider};

/// Why a usage fetch could not produce a snapshot.
#[derive(Debug, thiserror::Error)]
pub enum UsageError {
    #[error("credential not found")]
    CredentialNotFound,
    #[error("provider not found")]
    ProviderNotFound,
    #[error("unknown channel: {0}")]
    UnknownChannel(String),
    #[error("channel exposes no usage endpoint")]
    Unsupported,
    #[error(transparent)]
    Channel(#[from] ChannelError),
    #[error("decrypt secret: {0}")]
    Decrypt(String),
    #[error("upstream usage request failed: {0}")]
    Upstream(String),
    #[error("usage endpoint returned HTTP {0}")]
    Status(u16),
}

/// Fetch the live usage snapshot for one credential id.
pub async fn fetch_usage(
    state: &AppState,
    credential_id: i64,
) -> Result<UsageSnapshot, UsageError> {
    let credential = state
        .persistence
        .get_credential(credential_id)
        .await
        .map_err(|e| UsageError::Upstream(e.to_string()))?
        .ok_or(UsageError::CredentialNotFound)?;
    let provider = state
        .persistence
        .get_provider(credential.provider_id)
        .await
        .map_err(|e| UsageError::Upstream(e.to_string()))?
        .ok_or(UsageError::ProviderNotFound)?;
    let channel = state
        .channels
        .get(&provider.channel)
        .ok_or_else(|| UsageError::UnknownChannel(provider.channel.clone()))?;

    // Decrypt → ensure a fresh access token (the usage endpoints are bearer-auth,
    // so a stale token would just 401). `ensure_fresh` re-seals + persists any
    // rotation, exactly as the traffic path does.
    let opened = state
        .cipher
        .open(&credential.secret_json)
        .map_err(|e| UsageError::Decrypt(e.to_string()))?;
    let secret = state
        .refresh
        .ensure_fresh(state, &channel, &credential, &provider, opened, false)
        .await?;

    // None → the channel has no usage endpoint (api-key / vertex channels).
    let client = resolve_client(state, &channel, &credential, &provider)?;
    fetch_with(&channel, &secret, &provider.settings_json, &client).await
}

/// Transport-injectable core: build the channel's usage request, send it, and
/// parse the response. Split out from [`fetch_usage`] so the request/parse path
/// can be exercised over a stub client without the pool.
async fn fetch_with(
    channel: &Arc<dyn Channel>,
    secret: &serde_json::Value,
    settings: &serde_json::Value,
    client: &Arc<dyn UpstreamClient>,
) -> Result<UsageSnapshot, UsageError> {
    let Some(req) = channel.prepare_usage_request(secret, settings)? else {
        return Err(UsageError::Unsupported);
    };
    let resp = client
        .send(req)
        .await
        .map_err(|e| UsageError::Upstream(e.to_string()))?;
    let status = resp.status();
    let headers = resp.headers().clone();
    let body = resp.into_body();

    channel
        .parse_usage(status, &headers, &body)
        .ok_or(UsageError::Status(status.as_u16()))
}

/// Resolve the pooled client for this credential: its effective proxy + TLS
/// fingerprint (DB override) else the channel's built-in emulation, mirroring
/// [`super::refresh`] and `failover::attempt`.
pub(crate) fn resolve_client(
    state: &AppState,
    channel: &Arc<dyn Channel>,
    credential: &Credential,
    provider: &Provider,
) -> Result<Arc<dyn UpstreamClient>, UsageError> {
    state
        .upstream_client_for_credential(channel, credential, provider)
        .map_err(|e| UsageError::Upstream(format!("resolve usage client: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use bytes::Bytes;
    use http::{Response, StatusCode};
    use serde_json::json;

    use crate::http::client::ClientError;

    /// Upstream stub that returns a canned status + body for any request.
    struct CannedUpstream {
        status: StatusCode,
        body: &'static [u8],
    }
    #[async_trait]
    impl UpstreamClient for CannedUpstream {
        async fn send(&self, _req: http::Request<Bytes>) -> Result<Response<Bytes>, ClientError> {
            Ok(Response::builder()
                .status(self.status)
                .body(Bytes::from_static(self.body))
                .unwrap())
        }
    }

    fn claudecode() -> Arc<dyn Channel> {
        crate::channel::registry::ChannelRegistry::with_builtin()
            .get("claudecode")
            .expect("claudecode registered")
    }

    /// The driver runs a real channel's prepare → (stub send) → parse path.
    #[tokio::test]
    async fn fetch_with_parses_real_channel_response() {
        let client: Arc<dyn UpstreamClient> = Arc::new(CannedUpstream {
            status: StatusCode::OK,
            body: br#"{"five_hour":{"utilization":27,"resets_at":"2026-06-12T16:20:00+00:00"},
                      "seven_day":{"utilization":95,"resets_at":"2026-06-16T08:00:00+00:00"}}"#,
        });
        let secret = json!({ "access_token": "tok" });
        let snap = fetch_with(&claudecode(), &secret, &json!({}), &client)
            .await
            .expect("snapshot");
        let names: Vec<&str> = snap.windows.iter().map(|w| w.name.as_str()).collect();
        assert_eq!(names, ["five_hour", "seven_day"]);
        assert_eq!(snap.windows[1].used_percent, Some(95.0));
    }

    /// A non-2xx upstream surfaces as `Status`, not a bogus empty snapshot.
    #[tokio::test]
    async fn non_2xx_upstream_is_status_error() {
        let client: Arc<dyn UpstreamClient> = Arc::new(CannedUpstream {
            status: StatusCode::TOO_MANY_REQUESTS,
            body: b"{}",
        });
        let err = fetch_with(
            &claudecode(),
            &json!({ "access_token": "t" }),
            &json!({}),
            &client,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, UsageError::Status(429)));
    }
}
