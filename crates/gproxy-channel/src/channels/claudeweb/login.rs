//! Cookie initialization and twelve-hourly re-validation against
//! `GET /api/bootstrap` (v3 `claudeweb/login.rs` and `auth.rs::refresh`).

use gproxy_protocol::connection::Bytes;
use http::StatusCode;

use super::{
    ClaudeWeb, ClaudeWebConfig, MAX_SERVICE_BODY, auth,
    bootstrap::{self, BootstrapFailure},
    id, now_ms, prepare, read_body,
};
use crate::{
    OutboundClient,
    channel::{
        ChannelError, CredentialRefresh, CredentialUpdate, OperationFuture, ProviderView,
        RefreshContext,
    },
};

async fn fetch_bootstrap(
    client: &dyn OutboundClient,
    provider: ProviderView<'_>,
    cookie: &str,
    device_id: Option<&str>,
) -> Result<(StatusCode, Bytes), ChannelError> {
    let config = ClaudeWebConfig::from_view(provider)?;
    let base = auth::base_url(provider);
    let url = prepare::endpoint_url(
        &config.endpoints,
        prepare::BOOTSTRAP,
        &base,
        "/api/bootstrap",
        "",
        "",
    );
    let request = prepare::session_get(&url, cookie, device_id, &base)?;
    let response = client.send(request).await?;
    let body = read_body(response.body, MAX_SERVICE_BODY).await?;
    Ok((response.status, body))
}

fn validity_ms(now: i64) -> Option<i64> {
    now.checked_add(auth::VALIDATION_SECS * 1000)
}

impl CredentialRefresh for ClaudeWeb {
    /// Re-validate the cookie. A 401/403 or a logged-out bootstrap is final;
    /// other failures are transient. Persist public account and model metadata.
    fn refresh<'a>(&'a self, context: RefreshContext<'a>) -> OperationFuture<'a, CredentialUpdate> {
        Box::pin(async move {
            let secret = context.credential.secret;
            let cookie = auth::field(secret, "cookie")
                .or_else(|| auth::field(secret, "session_key"))
                .ok_or_else(|| {
                    ChannelError::RefreshRejected("credential has no session cookie".into())
                })?;
            let cookie = auth::normalize_cookie(cookie).ok_or(ChannelError::InvalidCredential)?;
            let device_id = match auth::field(secret, "device_id") {
                Some(value) => value.to_owned(),
                None => id::uuid()?,
            };
            let device_id = Some(device_id.as_str());
            let (status, body) =
                fetch_bootstrap(context.client, context.provider, &cookie, device_id).await?;
            if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
                return Err(ChannelError::RefreshRejected(format!(
                    "claude.ai session rejected with http {}",
                    status.as_u16()
                )));
            }
            if !status.is_success() {
                return Err(ChannelError::UpstreamResponse { status, body });
            }
            let bootstrap = bootstrap::parse(&body).map_err(|failure| match failure {
                BootstrapFailure::LoggedOut => {
                    ChannelError::RefreshRejected("claude.ai session is logged out".into())
                }
                BootstrapFailure::Malformed(message) => ChannelError::InvalidResponse(message),
            })?;
            let now = now_ms()?;
            let mut updated = bootstrap::secret(&cookie, device_id, &bootstrap, now);
            // Keep any extra fields a host or operator stored on the secret.
            if let (Some(previous), Some(next)) = (secret.as_object(), updated.as_object_mut()) {
                for (key, value) in previous {
                    next.entry(key.clone()).or_insert_with(|| value.clone());
                }
            }
            let mut metadata = context
                .credential
                .metadata
                .as_object()
                .cloned()
                .unwrap_or_default();
            if let Some(fields) = bootstrap::metadata(&bootstrap, now).as_object() {
                metadata.extend(fields.clone());
            }
            Ok(CredentialUpdate {
                metadata: Some(metadata.into()),
                secret: updated,
                expires_at_ms: validity_ms(now),
            })
        })
    }
}
