//! Cookie login and the twelve-hourly re-validation, both against
//! `GET /api/bootstrap` (v3 `claudeweb/login.rs` and `auth.rs::refresh`).

use gproxy_protocol::connection::Bytes;
use http::StatusCode;

use super::{
    ClaudeWeb, ClaudeWebConfig, auth,
    bootstrap::{self, BootstrapFailure},
    id, now_ms, prepare, read_body,
};
use crate::{
    OutboundClient,
    channel::{
        AcquiredCredential, ChannelError, CookieLogin, CredentialRefresh, CredentialUpdate,
        LoginContext, OperationFuture, ProviderView, RefreshContext,
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
    let body = read_body(response.body).await?;
    Ok((response.status, body))
}

fn validity_ms(now: i64) -> Option<i64> {
    now.checked_add(auth::VALIDATION_SECS * 1000)
}

impl CookieLogin for ClaudeWeb {
    /// A pasted cookie (or bare sessionKey) becomes a credential once the
    /// bootstrap reply names a logged-in account with a chat organization.
    /// A fresh device id is minted so every call looks like one browser.
    fn exchange_cookie<'a>(
        &'a self,
        context: LoginContext<'a>,
        cookie: &'a str,
    ) -> OperationFuture<'a, AcquiredCredential> {
        Box::pin(async move {
            let cookie = auth::normalize_cookie(cookie).ok_or(ChannelError::InvalidCredential)?;
            let device_id = id::uuid()?;
            let (status, body) =
                fetch_bootstrap(context.client, context.provider, &cookie, Some(&device_id))
                    .await?;
            if !status.is_success() {
                return Err(ChannelError::UpstreamResponse { status, body });
            }
            let bootstrap = bootstrap::parse(&body).map_err(|failure| match failure {
                BootstrapFailure::LoggedOut => ChannelError::InvalidCredential,
                BootstrapFailure::Malformed(message) => ChannelError::InvalidResponse(message),
            })?;
            let now = now_ms()?;
            Ok(AcquiredCredential {
                secret: bootstrap::secret(&cookie, Some(&device_id), &bootstrap, now),
                expires_at_ms: validity_ms(now),
                metadata: bootstrap::metadata(&bootstrap, now),
            })
        })
    }
}

impl CredentialRefresh for ClaudeWeb {
    /// Re-validate the cookie. A 401/403 or a logged-out bootstrap is final;
    /// other failures are transient. Model lists learned here cannot be
    /// persisted (`CredentialUpdate` carries no metadata), only the secret.
    fn refresh<'a>(&'a self, context: RefreshContext<'a>) -> OperationFuture<'a, CredentialUpdate> {
        Box::pin(async move {
            let secret = context.credential.secret;
            let cookie = auth::field(secret, "cookie")
                .or_else(|| auth::field(secret, "session_key"))
                .ok_or_else(|| {
                    ChannelError::RefreshRejected("credential has no session cookie".into())
                })?;
            let device_id = auth::field(secret, "device_id");
            let (status, body) =
                fetch_bootstrap(context.client, context.provider, cookie, device_id).await?;
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
            let mut updated = bootstrap::secret(cookie, device_id, &bootstrap, now);
            // Keep any extra fields a host or operator stored on the secret.
            if let (Some(previous), Some(next)) = (secret.as_object(), updated.as_object_mut()) {
                for (key, value) in previous {
                    next.entry(key.clone()).or_insert_with(|| value.clone());
                }
            }
            Ok(CredentialUpdate {
                secret: updated,
                expires_at_ms: validity_ms(now),
            })
        })
    }
}
