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

/// The claude.ai error message, whether a string or a structured value.
fn error_message(body: &[u8]) -> Option<String> {
    let value: serde_json::Value = serde_json::from_slice(body).ok()?;
    let message = value.pointer("/error/message")?;
    Some(match message.as_str() {
        Some(text) => text.to_owned(),
        None => message.to_string(),
    })
}

/// Only a 401, an OAuth-forbidden 403 or a disabled organization retires the
/// cookie (clewdr `check_claude`). Any other 403 may be a Cloudflare or region
/// block in front of a still-valid session.
fn definitive_rejection(status: StatusCode, body: &[u8]) -> Option<String> {
    let message = error_message(body);
    let rejected = match status {
        StatusCode::UNAUTHORIZED => true,
        StatusCode::FORBIDDEN => message.as_deref().is_some_and(|message| {
            message
                .to_ascii_lowercase()
                .contains("oauth authentication is currently not allowed for this organization")
        }),
        StatusCode::BAD_REQUEST => {
            message.as_deref() == Some("This organization has been disabled.")
        }
        _ => false,
    };
    rejected.then(|| match message {
        Some(message) => format!(
            "claude.ai session rejected with http {}: {message}",
            status.as_u16()
        ),
        None => format!("claude.ai session rejected with http {}", status.as_u16()),
    })
}

fn validity_ms(now: i64) -> Option<i64> {
    now.checked_add(auth::VALIDATION_SECS * 1000)
}

impl CredentialRefresh for ClaudeWeb {
    /// Re-validate the cookie. A definitive rejection, a logged-out bootstrap
    /// or a banned organization is final; other failures are transient.
    /// Persist public account and model metadata.
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
            if let Some(reason) = definitive_rejection(status, &body) {
                return Err(ChannelError::RefreshRejected(reason));
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
            if bootstrap::banned(&bootstrap, now) {
                return Err(ChannelError::RefreshRejected(
                    "claude.ai account is banned".into(),
                ));
            }
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
