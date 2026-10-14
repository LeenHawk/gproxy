//! Gemini resumable uploads through the gateway.
//!
//! The Files API's resumable protocol is a multi-request exchange: a *start*
//! (`x-goog-upload-command: start`) answers with an `x-goog-upload-url`, and
//! the bytes then go to that URL (`upload`, `upload, finalize`, `query`). The
//! final answer is the file resource. Passed through as it is, the URL points
//! at the upstream, the client uploads there directly, the gateway never sees
//! the file id — so it never records an owner, and the caller's own file is a
//! 404 at the gateway afterwards. The official google-genai SDKs upload this
//! way by default.
//!
//! So a successful start is rewritten: the upstream URL is kept server-side in
//! an *upload session* bound to the caller's scope, the provider and the
//! credential that started it, and the client is handed a gateway URL whose
//! `upload_id` is an unguessable session token (`gproxy-upload-…`). Core emits
//! it mount-relative (`/upload/v1beta/files?upload_id=…`); the host prefixes
//! its external origin and mount, which only it knows. Every follow-up comes
//! back through the gateway as an ordinary authenticated `CreateFile` in the
//! Gemini dialect: the session must exist and belong to the same scope and
//! provider (otherwise `ResourceNotFound`), the request is pinned to the
//! session's credential, and its path and query are swapped for the
//! upstream's. The finalize answer carries the file, which the ordinary
//! create rule registers for the caller. The upstream upload URL never
//! reaches the client.
//!
//! Sessions live in the shared cache for a week, the lifetime Google gives an
//! upload session. An upload URL on an origin other than the provider's
//! configured base cannot be reached through the channel, so such a start is
//! passed through untouched (and its file stays unregistered).

use crate::{Core, CoreError, CoreResult, RequestContext};
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey, WireRequest, WireResponse};
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Duration};

/// What every gateway upload token starts with, so a follow-up is told apart
/// from an upstream `upload_id` a client might still send.
pub const TOKEN_PREFIX: &str = "gproxy-upload-";
/// The response header carrying the upload session URL.
pub const UPLOAD_URL: &str = "x-goog-upload-url";
/// The gateway path a follow-up is sent to, relative to the mount.
const GATEWAY_PATH: &str = "/upload/v1beta/files";
/// Google keeps a resumable session for a week.
const SESSION_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct UploadSession {
    scope: String,
    provider_id: String,
    credential_id: String,
    /// The upstream upload URL's path and query.
    path: String,
    query: Option<String>,
}

fn key(token: &str) -> String {
    crate::keys::upload_session(token)
}

/// The gateway session token a Gemini upload follow-up names in its
/// `upload_id`, if it names one.
pub fn session_token(operation: OperationKey, query: Option<&str>) -> Option<String> {
    if operation.operation != Operation::CreateFile || operation.dialect != Dialect::Gemini {
        return None;
    }
    form_urlencoded::parse(query?.as_bytes())
        .find(|(name, _)| name == "upload_id")
        .map(|(_, value)| value.into_owned())
        .filter(|value| value.starts_with(TOKEN_PREFIX))
}

/// The upstream URL's path and query, when it is on the provider's own
/// origin (or the provider names none and so uses the channel's default).
fn upstream_target(upload_url: &str, base_url: Option<&str>) -> Option<(String, Option<String>)> {
    let url = url::Url::parse(upload_url).ok()?;
    if !matches!(url.scheme(), "https" | "http") {
        return None;
    }
    if let Some(base) = base_url.map(str::trim).filter(|base| !base.is_empty()) {
        let base = url::Url::parse(base).ok()?;
        if base.origin() != url.origin() {
            return None;
        }
    }
    Some((url.path().to_owned(), url.query().map(str::to_owned)))
}

impl<C> Core<C> {
    async fn upload_session(&self, token: &str) -> CoreResult<Option<UploadSession>> {
        let Some(entry) = self.cache.get(&key(token)).await? else {
            return Ok(None);
        };
        Ok(serde_json::from_slice(&entry.value).ok())
    }

    /// For a host walking several providers: the provider and credential an
    /// upload follow-up must go to, when it names a session of this scope.
    pub async fn upload_session_target(
        &self,
        scope: &str,
        operation: OperationKey,
        query: Option<&str>,
    ) -> CoreResult<Option<(String, String)>> {
        let Some(token) = session_token(operation, query) else {
            return Ok(None);
        };
        Ok(self
            .upload_session(&token)
            .await?
            .filter(|session| session.scope == scope)
            .map(|session| (session.provider_id, session.credential_id)))
    }

    /// The enforcement for a follow-up: the session must be this scope's on
    /// this provider; the request is pinned to its credential and pointed at
    /// the upstream session. Any other request passes through unchanged.
    pub(crate) async fn bind_upload_session(
        &self,
        context: Arc<RequestContext>,
        request: &mut WireRequest<HttpBody>,
    ) -> CoreResult<Arc<RequestContext>> {
        let Some(token) = session_token(context.operation, request.query.as_deref()) else {
            return Ok(context);
        };
        let not_found = || CoreError::ResourceNotFound {
            kind: "upload session",
            id: token.clone(),
        };
        let session = self
            .upload_session(&token)
            .await?
            .filter(|session| {
                session.scope == context.scope
                    && session.provider_id == context.target.provider.entity.id
            })
            .ok_or_else(not_found)?;
        let credential = context
            .target
            .credentials
            .iter()
            .find(|credential| credential.id == session.credential_id)
            .cloned()
            .ok_or_else(not_found)?;
        request.path = session.path;
        request.query = session.query;
        Ok(super::pin(context, credential))
    }

    /// After a successful Gemini resumable start: keep the upstream upload URL
    /// in a session bound to the caller and hand back the gateway's own.
    /// `true` when the response was a start (there is no file to register).
    pub(crate) async fn open_upload_session(
        &self,
        request: &RequestContext,
        credential_id: &str,
        response: &mut WireResponse<HttpBody>,
    ) -> CoreResult<bool> {
        if request.operation.operation != Operation::CreateFile
            || request.operation.dialect != Dialect::Gemini
        {
            return Ok(false);
        }
        let Some(upload_url) = response
            .headers
            .get(UPLOAD_URL)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
        else {
            return Ok(false);
        };
        let provider = &request.target.provider.entity;
        let Some((path, query)) = upstream_target(&upload_url, provider.base_url.as_deref()) else {
            tracing::warn!(
                provider = %provider.id,
                "resumable upload URL is not on the provider's origin; passed through, and the file will not be registered"
            );
            return Ok(true);
        };
        let token = format!(
            "{TOKEN_PREFIX}{}{}",
            crate::ids::random_id(),
            crate::ids::random_id()
        );
        let session = UploadSession {
            scope: request.scope.clone(),
            provider_id: provider.id.clone(),
            credential_id: credential_id.to_owned(),
            path,
            query,
        };
        let value = serde_json::to_vec(&session)
            .map_err(|e| CoreError::InvalidTarget(format!("upload session: {e}")))?;
        self.cache.put(&key(&token), value, SESSION_TTL).await?;
        let gateway = format!("{GATEWAY_PATH}?upload_id={token}&upload_protocol=resumable");
        response.headers.insert(
            http::HeaderName::from_static(UPLOAD_URL),
            http::HeaderValue::from_str(&gateway)
                .map_err(|e| CoreError::InvalidTarget(e.to_string()))?,
        );
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gemini_create() -> OperationKey {
        OperationKey {
            operation: Operation::CreateFile,
            dialect: Dialect::Gemini,
        }
    }

    #[test]
    fn only_gateway_tokens_on_gemini_uploads_are_sessions() {
        let query = Some("upload_id=gproxy-upload-abc&upload_protocol=resumable");
        assert_eq!(
            session_token(gemini_create(), query).as_deref(),
            Some("gproxy-upload-abc")
        );
        assert_eq!(
            session_token(gemini_create(), Some("upload_id=upstream-xyz")),
            None
        );
        assert_eq!(session_token(gemini_create(), None), None);
        let openai = OperationKey {
            operation: Operation::CreateFile,
            dialect: Dialect::OpenAi,
        };
        assert_eq!(session_token(openai, query), None);
    }

    #[test]
    fn the_upstream_url_must_be_on_the_providers_origin() {
        let url = "https://generativelanguage.googleapis.com/upload/v1beta/files?upload_id=u1&upload_protocol=resumable";
        assert_eq!(
            upstream_target(url, Some("https://generativelanguage.googleapis.com")),
            Some((
                "/upload/v1beta/files".into(),
                Some("upload_id=u1&upload_protocol=resumable".into())
            ))
        );
        assert!(upstream_target(url, None).is_some());
        assert_eq!(upstream_target(url, Some("https://proxy.example")), None);
        assert_eq!(upstream_target("not a url", None), None);
    }
}
