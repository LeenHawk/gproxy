//! The OAuth authorization server, mounted under every ingress mount.
//!
//! # The paths, and why they are where they are
//!
//! Relative to a mount prefix (`""`, `/acme`, `/openai-prod`):
//!
//! | Method | Path | RFC |
//! |---|---|---|
//! | `GET` | `{prefix}/v1/oauth/authorize` | 6749 §4.1.1, the consent handoff |
//! | `POST` | `{prefix}/v1/oauth/authorize` | 6749 §4.1.2, the person's decision |
//! | `POST` | `{prefix}/v1/oauth/token` | 6749 §4.1.3, §6; 8628 §3.4 |
//! | `POST` | `{prefix}/v1/oauth/device/code` | 8628 §3.1 |
//! | `POST` | `{prefix}/v1/oauth/revoke` | 7009 §2.1 |
//! | `GET` | `{prefix}/v1/.well-known/oauth-authorization-server` | 8414, issuer-relative |
//! | `GET` | `/.well-known/oauth-authorization-server{prefix}/v1` | 8414 §3.1, path insertion |
//!
//! **The issuer identifier is `{origin}{prefix}/v1`**, not `{origin}{prefix}`.
//! P7's module note sketched the mounts as `https://host`,
//! `https://host/{namespace}/v1` and `https://host/{provider}/v1`; the first
//! of those is corrected here to `https://host/v1`, so that one rule produces
//! all three and `Issuer::metadata`'s `{issuer}/oauth/…` endpoints land on the
//! paths above at every mount. A client that discovers an issuer under one
//! mount cannot then be handed endpoints belonging to another.
//!
//! # The other envelope
//!
//! Everything here answers through [`OAuthEnvelope`], never through the
//! product one. RFC 6749 §5.2 defines `error` as a *string*, and a client that
//! finds this crate's `{"error":{"code":…}}` object there reads no code at
//! all — it cannot tell "re-run the login" from "keep polling".
//!
//! # What is not here
//!
//! The *device* consent operations — rendering a pending device authorization,
//! approving one — are on the portal API, `/portal/api/oauth/device`. They are
//! not protocol endpoints: no OAuth client ever calls them, they require a
//! console session, and the RFC's own verification URI
//! ([`DEVICE_VERIFICATION_PATH`](gproxy_app::operations::issuer)) points at
//! the console's device page for exactly that reason.

use axum::response::{IntoResponse, Response};
use gproxy_app::{
    AppError, Caller, Operations,
    dto::{AuthorizeQuery, ConsentDecision, DeviceCodeRequest, RevokeRequest, TokenRequest},
    operations::IssuerOrigin,
};
use gproxy_seaorm::BatchConnectionTrait;
use http::{HeaderValue, Method, StatusCode, header, request::Parts};
use serde::Deserialize;

use crate::{HostState, Mount, error::OAuthEnvelope, session};

/// The path segment every issuer endpoint hangs off, relative to a mount.
const ISSUER_MOUNT: &str = "/v1";
const OAUTH_PREFIX: &str = "/v1/oauth/";
const WELL_KNOWN: &str = "/.well-known/oauth-authorization-server";
/// Where a browser is sent to approve an authorization: the console's consent
/// page. Relative to the instance root, like the device page, because the
/// console is one application however many mounts the data plane answers at.
const CONSENT_PATH: &str = "/console/authorize";

/// One issuer endpoint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IssuerRoute {
    /// Hand a browser to the consent page, or answer the pending request's
    /// details to a signed-in client.
    AuthorizeStart,
    /// Record the person's decision.
    AuthorizeDecide,
    Token,
    DeviceCode,
    Revoke,
    /// The discovery document. `Some` when the path itself named the mount
    /// (RFC 8414's path-insertion form), `None` when it is the request's own.
    Metadata {
        mount: Option<String>,
    },
}

/// Whether a mount-relative path belongs to the issuer, for the mount grammar.
pub fn is_issuer_path(path: &str) -> bool {
    path.starts_with(OAUTH_PREFIX) || path == format!("{ISSUER_MOUNT}{WELL_KNOWN}")
}

/// The endpoint `method` and a mount-relative `path` name.
pub fn route(method: &Method, path: &str) -> Option<IssuerRoute> {
    // RFC 8414 §3.1 inserts the well-known segment *before* the issuer's path,
    // so this form names its own mount and is answered whatever mount the
    // request arrived on.
    if method == Method::GET
        && let Some(mount) = path.strip_prefix(WELL_KNOWN)
    {
        return Some(IssuerRoute::Metadata {
            mount: Some(mount.to_owned()),
        });
    }
    if method == Method::GET && path == format!("{ISSUER_MOUNT}{WELL_KNOWN}") {
        return Some(IssuerRoute::Metadata { mount: None });
    }
    let endpoint = path.strip_prefix(OAUTH_PREFIX)?;
    match (method, endpoint) {
        (&Method::GET, "authorize") => Some(IssuerRoute::AuthorizeStart),
        (&Method::POST, "authorize") => Some(IssuerRoute::AuthorizeDecide),
        (&Method::POST, "token") => Some(IssuerRoute::Token),
        (&Method::POST, "device/code") => Some(IssuerRoute::DeviceCode),
        (&Method::POST, "revoke") => Some(IssuerRoute::Revoke),
        (&Method::GET, "metadata") => Some(IssuerRoute::Metadata { mount: None }),
        _ => None,
    }
}

/// Run one issuer endpoint.
pub async fn handle<C>(
    state: &HostState<C>,
    route: IssuerRoute,
    mount: &Mount,
    scheme: &str,
    parts: Parts,
    body: bytes::Bytes,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let data = state.app().data();
    let operations = Operations::new(state.app().gproxy(), &data, state.app().config());
    let issuer = operations.issuer();

    // The origin is resolved **only where it is needed** — the discovery
    // document and the device flow's verification URI. Resolving it up front
    // would make an instance with no `public_base_url` refuse a token
    // exchange from a client that sent no `Host` header, which is a failure
    // that has nothing to do with the exchange.
    match route {
        IssuerRoute::Metadata { mount: explicit } => {
            match origin(state, mount, scheme, &parts, explicit) {
                Ok(origin) => no_store(json(&issuer.metadata(&origin))),
                Err(error) => OAuthEnvelope(error).into_response(),
            }
        }
        IssuerRoute::Token => match form_or_json::<TokenRequest>(&parts, &body) {
            Ok(request) => match issuer.token(&request).await {
                // RFC 6749 §5.1: a token response is never cached.
                Ok(response) => no_store(json(&response)),
                Err(error) => no_store(OAuthEnvelope(error).into_response()),
            },
            Err(error) => OAuthEnvelope(error).into_response(),
        },
        IssuerRoute::DeviceCode => match form_or_json::<DeviceCodeRequest>(&parts, &body) {
            Ok(request) => match origin(state, mount, scheme, &parts, None) {
                Ok(origin) => match issuer.device_code(&request, &origin).await {
                    Ok(response) => no_store(json(&response)),
                    Err(error) => no_store(OAuthEnvelope(error).into_response()),
                },
                Err(error) => OAuthEnvelope(error).into_response(),
            },
            Err(error) => OAuthEnvelope(error).into_response(),
        },
        IssuerRoute::Revoke => match form_or_json::<RevokeRequest>(&parts, &body) {
            // RFC 7009 §2.2: an unknown token is a successful revocation, so
            // the answer is 200 whether a row was removed or not.
            Ok(request) => match issuer.revoke(&request).await {
                Ok(revoked) => no_store(json(&serde_json::json!({ "revoked": revoked }))),
                Err(error) => no_store(OAuthEnvelope(error).into_response()),
            },
            Err(error) => OAuthEnvelope(error).into_response(),
        },
        IssuerRoute::AuthorizeStart => authorize_start(state, &parts).await,
        IssuerRoute::AuthorizeDecide => authorize_decide(state, &parts, &body).await,
    }
}

/// `GET …/oauth/authorize`: the consent handoff.
///
/// A browser is redirected to the portal's consent page with the request
/// intact — this host serves no HTML of its own, and the page that renders a
/// consent screen is the one that already knows how to sign a person in. A
/// client that asked for JSON and is signed in gets the validated details
/// instead, which is what the page itself fetches.
async fn authorize_start<C>(state: &HostState<C>, parts: &Parts) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    if wants_html(parts) {
        let target = match parts.uri.query() {
            Some(query) => format!("{CONSENT_PATH}?{query}"),
            None => CONSENT_PATH.to_owned(),
        };
        return match HeaderValue::from_str(&target) {
            Ok(value) => {
                let mut response = StatusCode::FOUND.into_response();
                response.headers_mut().insert(header::LOCATION, value);
                response
            }
            Err(_) => OAuthEnvelope(AppError::invalid("the authorization query is not a URL"))
                .into_response(),
        };
    }
    let query = match query::<AuthorizeQuery>(parts) {
        Ok(query) => query,
        Err(error) => return OAuthEnvelope(error).into_response(),
    };
    let caller = match caller(state, parts).await {
        Ok(caller) => caller,
        Err(error) => return OAuthEnvelope(error).into_response(),
    };
    let data = state.app().data();
    let operations = Operations::new(state.app().gproxy(), &data, state.app().config());
    match operations.issuer().authorize_details(&caller, &query).await {
        Ok(details) => no_store(json(&details)),
        Err(error) => OAuthEnvelope(error).into_response(),
    }
}

#[derive(Deserialize)]
struct DecisionBody {
    decision: ConsentDecision,
}

/// `POST …/oauth/authorize`: the person approved or refused.
///
/// The answer carries the `Location` the client must be sent to rather than
/// being a redirect itself, because the caller is the consent page's own
/// `fetch` and a 302 there would be followed by the browser instead of by the
/// page.
async fn authorize_decide<C>(state: &HostState<C>, parts: &Parts, body: &[u8]) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let query = match query::<AuthorizeQuery>(parts) {
        Ok(query) => query,
        Err(error) => return OAuthEnvelope(error).into_response(),
    };
    let decision: DecisionBody = match serde_json::from_slice(body) {
        Ok(decision) => decision,
        Err(error) => {
            return OAuthEnvelope(AppError::invalid(format!("decision body: {error}")))
                .into_response();
        }
    };
    let caller = match caller(state, parts).await {
        Ok(caller) => caller,
        Err(error) => return OAuthEnvelope(error).into_response(),
    };
    let data = state.app().data();
    let operations = Operations::new(state.app().gproxy(), &data, state.app().config());
    match operations
        .issuer()
        .authorize(&caller, &query, decision.decision)
        .await
    {
        Ok(outcome) => no_store(json(&serde_json::json!({
            "location": outcome.location(),
            "outcome": outcome,
        }))),
        Err(error) => OAuthEnvelope(error).into_response(),
    }
}

/// Who may answer a consent: a signed-in person, or an API key with its
/// management flag on — the credentials that may manage anything
/// ([`Caller::may_manage`]).
///
/// Consent is what mints a grant, so an OAuth token is refused: it would
/// approve a fresh grant for itself, one without the baseline it was issued
/// under and outliving its revocation. An ordinary key is refused for the same
/// reason; a management key is already an administrative credential and may.
/// A cookie approval is a cookie write, so it is same-origin checked like every
/// other one.
async fn caller<C>(state: &HostState<C>, parts: &Parts) -> Result<Caller, AppError>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let caller = session::authenticate(state.app(), &parts.method, &parts.headers).await?;
    require_manager(&caller)?;
    Ok(caller)
}

/// The consent rule, shared with the portal's device approval.
pub(crate) fn require_manager(caller: &Caller) -> Result<(), AppError> {
    if caller.may_manage() {
        return Ok(());
    }
    Err(AppError::forbidden(
        "consent takes a signed-in session or a management API key",
    ))
}

/// Where this issuer answers, as this request saw it.
///
/// `public_base_url` wins when it is configured: an operator who states the
/// external origin has said what the issuer identifier is, and no header can
/// then move it. Otherwise the `Host` header names the authority and
/// [`policy::client_scheme`](crate::policy::client_scheme) names the scheme —
/// which reads `x-forwarded-proto` **only from a trusted peer**, because that
/// header decides the scheme of every endpoint in a document telling a client
/// where to send an authorization code.
fn origin<C>(
    state: &HostState<C>,
    mount: &Mount,
    scheme: &str,
    parts: &Parts,
    explicit_mount: Option<String>,
) -> Result<IssuerOrigin, AppError> {
    let base = match state.app().config().public_base_url.as_deref() {
        Some(base) if !base.trim().is_empty() => base.trim().to_owned(),
        _ => {
            let authority = parts
                .headers
                .get(header::HOST)
                .and_then(|value| value.to_str().ok())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    AppError::invalid("the request names no host and no public base URL is set")
                })?;
            format!("{scheme}://{authority}")
        }
    };
    let mount = match explicit_mount {
        Some(mount) => mount,
        None => format!("{}{ISSUER_MOUNT}", mount.prefix()),
    };
    IssuerOrigin::new(&base)?.mounted_at(&mount)
}

/// An OAuth request body, in either encoding a client might send it.
///
/// RFC 6749 §4.1.3 says `application/x-www-form-urlencoded`, and that is what
/// every conforming client sends. JSON is accepted because some CLIs send it
/// anyway and refusing them buys nothing: both decode into the same typed
/// request, so there is one validation path regardless.
fn form_or_json<T: serde::de::DeserializeOwned>(parts: &Parts, body: &[u8]) -> Result<T, AppError> {
    let json = parts
        .headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.trim_start().starts_with("application/json"));
    if json {
        serde_json::from_slice(body)
            .map_err(|error| AppError::invalid(format!("request body: {error}")))
    } else {
        serde_urlencoded::from_bytes(body)
            .map_err(|error| AppError::invalid(format!("request form: {error}")))
    }
}

fn query<T: serde::de::DeserializeOwned>(parts: &Parts) -> Result<T, AppError> {
    serde_urlencoded::from_str(parts.uri.query().unwrap_or_default())
        .map_err(|error| AppError::invalid(format!("request query: {error}")))
}

/// Whether this is a browser navigating rather than a program fetching.
fn wants_html(parts: &Parts) -> bool {
    parts
        .headers
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.contains("text/html"))
}

fn json<T: serde::Serialize>(body: &T) -> Response {
    crate::error::ok_json(body)
}

/// RFC 6749 §5.1: tokens and anything that carries one are never cached.
fn no_store(mut response: Response) -> Response {
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store, no-cache, must-revalidate"),
    );
    response
        .headers_mut()
        .insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_protocol_endpoints_are_matched_under_the_mount() {
        assert_eq!(
            route(&Method::POST, "/v1/oauth/token"),
            Some(IssuerRoute::Token)
        );
        assert_eq!(
            route(&Method::POST, "/v1/oauth/device/code"),
            Some(IssuerRoute::DeviceCode)
        );
        assert_eq!(
            route(&Method::POST, "/v1/oauth/revoke"),
            Some(IssuerRoute::Revoke)
        );
        assert_eq!(
            route(&Method::GET, "/v1/oauth/authorize"),
            Some(IssuerRoute::AuthorizeStart)
        );
        assert_eq!(
            route(&Method::POST, "/v1/oauth/authorize"),
            Some(IssuerRoute::AuthorizeDecide)
        );
        // A method the endpoint does not take is not the endpoint.
        assert_eq!(route(&Method::GET, "/v1/oauth/token"), None);
        assert_eq!(route(&Method::POST, "/v1/messages"), None);
    }

    #[test]
    fn both_discovery_forms_are_served() {
        // Issuer-relative, which is what most clients try first.
        assert_eq!(
            route(&Method::GET, "/v1/.well-known/oauth-authorization-server"),
            Some(IssuerRoute::Metadata { mount: None })
        );
        // RFC 8414 §3.1's path insertion, which names its own mount.
        assert_eq!(
            route(
                &Method::GET,
                "/.well-known/oauth-authorization-server/acme/v1"
            ),
            Some(IssuerRoute::Metadata {
                mount: Some("/acme/v1".into())
            })
        );
        assert_eq!(
            route(&Method::GET, "/.well-known/oauth-authorization-server"),
            Some(IssuerRoute::Metadata {
                mount: Some(String::new())
            })
        );
    }

    #[test]
    fn the_mount_grammar_recognises_the_issuer() {
        assert!(is_issuer_path("/v1/oauth/token"));
        assert!(is_issuer_path("/v1/.well-known/oauth-authorization-server"));
        assert!(!is_issuer_path("/v1/oauth"));
        assert!(!is_issuer_path("/oauth/token"));
        assert!(!is_issuer_path("/v1/messages"));
    }

    #[test]
    fn a_token_request_decodes_from_a_form_and_from_json() {
        let mut parts = http::Request::new(()).into_parts().0;
        parts.headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/x-www-form-urlencoded"),
        );
        let form = b"grant_type=authorization_code&client_id=cli&code=abc&code_verifier=v";
        let request: TokenRequest = form_or_json(&parts, form).unwrap();
        assert_eq!(request.grant_type, "authorization_code");
        assert_eq!(request.client_id, "cli");
        assert_eq!(request.code.as_deref(), Some("abc"));

        parts.headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        let json = br#"{"grant_type":"refresh_token","client_id":"cli","refresh_token":"r"}"#;
        let request: TokenRequest = form_or_json(&parts, json).unwrap();
        assert_eq!(request.grant_type, "refresh_token");
        assert_eq!(request.refresh_token.as_deref(), Some("r"));
    }
}
