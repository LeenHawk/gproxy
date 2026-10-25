//! How a WorkBuddy credential comes into existence and how it stays alive.
//!
//! The plugin's login is a device flow without a user code (v3 `login.rs`):
//! `POST /v2/plugin/auth/state?platform=workbuddy` (or `workbuddy-ai` for the
//! international service) returns a `state` and the
//! `authUrl` to open, `GET /v2/plugin/auth/token?state=…` answers envelope
//! code 11217 until the browser is done and then the tokens, and
//! `GET /v2/plugin/login/account?state=…` answers 12151 until the account is
//! readable and then the facts the request path needs. The `state` is
//! therefore both the device code and the key both polls are keyed by.
//!
//! The refresh is `POST /v2/plugin/auth/token/refresh` with the refresh token
//! in a header rather than a body (v3 `refresh.rs`), and it may answer without
//! an expiry at all — in which case the credential says so rather than
//! pretending to a deadline it does not have.
//!
//! Everything the login learns about the account — the user id the request
//! path must announce, the tenant, the department, the domain — travels in
//! `provider_fields`, so `prepare` reads it back from the credential's
//! metadata and never goes looking.

use super::config::{ID, WorkBuddyConfig};
use super::envelope::{self, ACCOUNT_PENDING, AUTH_PENDING};
use super::{encode_component, json_headers, send, unix_now_ms};
use crate::channel::{
    ChannelError, CredentialRefresh, CredentialUpdate, CredentialView, DeviceAuthorization,
    DevicePoll, LoginContext, OAuthCredential, OAuthDeviceCode, OperationFuture, RefreshContext,
    RefreshReason,
};
use http::{HeaderName, HeaderValue, Method, StatusCode, header};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

/// What the upstream grants when it does not say (v3 `login.rs`).
const DEFAULT_EXPIRES_IN_SECS: i64 = 3_600;
/// When the last token was issued, in unix milliseconds, under the sealed
/// `provider_secrets` pocket (the plugin's `auth.lastRefreshTime`).
const LAST_REFRESH_MS: &str = "last_refresh_ms";
/// The plugin's refresh cadence (`AuthenticationManagerImpl
/// .scheduleRefreshSession`): once a day, or every five minutes once the
/// token's expiry is unknown or less than a day away.
const DAY_MS: i64 = 86_400_000;
const NEAR_EXPIRY_INTERVAL_MS: i64 = 300_000;
/// The browser step is a redirect, not a code a person types.
const NO_USER_CODE: &str = "No code required";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AuthState {
    state: String,
    auth_url: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Tokens {
    access_token: String,
    #[serde(default)]
    refresh_token: String,
    #[serde(default)]
    expires_in: Option<i64>,
    /// Already an absolute unix millisecond stamp when present.
    #[serde(default)]
    expires_at: Option<i64>,
    #[serde(default)]
    refresh_expires_in: Option<i64>,
    #[serde(default)]
    refresh_expires_at: Option<i64>,
    #[serde(default)]
    domain: Option<String>,
}

impl Tokens {
    fn expires_at_ms(&self) -> Option<i64> {
        self.expires_at.or_else(|| relative(self.expires_in))
    }

    fn refresh_expires_at_ms(&self) -> Option<i64> {
        self.refresh_expires_at
            .or_else(|| relative(self.refresh_expires_in))
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Account {
    uid: String,
    #[serde(default)]
    nickname: Option<String>,
    #[serde(default, rename = "type")]
    account_type: Option<String>,
    #[serde(default)]
    enterprise_id: Option<String>,
    #[serde(default)]
    enterprise_name: Option<String>,
    #[serde(default)]
    department_full_name: Option<String>,
}

fn relative(seconds: Option<i64>) -> Option<i64> {
    seconds.map(|seconds| unix_now_ms().saturating_add(seconds.max(0).saturating_mul(1_000)))
}

fn field(fields: &mut BTreeMap<String, Value>, name: &str, value: Option<String>) {
    if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
        fields.insert(name.into(), Value::String(value));
    }
}

/// The plugin announces its product on every auth call, before it has a token.
fn login_headers() -> http::HeaderMap {
    let mut headers = json_headers();
    headers.insert(
        HeaderName::from_static("x-product"),
        HeaderValue::from_static("SaaS"),
    );
    headers
}

/// The opt-outs the plugin writes on its pre-session calls
/// (`ExternalLinkAuthenticationProvider`): its interceptor reads them and
/// leaves them on the wire. `auth/state` and the token poll send all four;
/// the account poll, which carries a bearer, omits the first.
fn opt_out(headers: &mut http::HeaderMap, with_authorization: bool) {
    let names: &[&'static str] = if with_authorization {
        &[
            "x-no-authorization",
            "x-no-user-id",
            "x-no-enterprise-id",
            "x-no-department-info",
        ]
    } else {
        &["x-no-user-id", "x-no-enterprise-id", "x-no-department-info"]
    };
    for name in names {
        headers.insert(
            HeaderName::from_static(name),
            HeaderValue::from_static("true"),
        );
    }
}

/// `X-Domain` as the plugin's `enterpriseHeaders` sends it: the token's
/// domain, else the authority of the endpoint being called.
fn domain_header(
    headers: &mut http::HeaderMap,
    domain: Option<&str>,
    base: &str,
) -> Result<(), ChannelError> {
    let name = HeaderName::from_static("x-domain");
    let domain = domain
        .map(str::trim)
        .filter(|domain| !domain.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            base.parse::<http::Uri>()
                .ok()?
                .authority()
                .map(|authority| authority.as_str().to_owned())
        });
    if let Some(domain) = domain {
        headers.insert(
            name,
            HeaderValue::from_str(&domain).map_err(|_| ChannelError::InvalidCredential)?,
        );
    }
    Ok(())
}

impl OAuthDeviceCode for super::WorkBuddy {
    fn start<'a>(&'a self, context: LoginContext<'a>) -> OperationFuture<'a, DeviceAuthorization> {
        Box::pin(async move {
            let config = WorkBuddyConfig::from_view(context.provider)?;
            let base = config.base_url(context.provider);
            let (status, _, body) = send(
                context.client,
                Method::POST,
                &format!(
                    "{base}/v2/plugin/auth/state?platform={}",
                    config.login_platform()
                ),
                {
                    let mut headers = login_headers();
                    domain_header(&mut headers, None, &base)?;
                    opt_out(&mut headers, true);
                    headers
                },
                Some(b"{}".to_vec()),
            )
            .await?;
            if !status.is_success() {
                return Err(ChannelError::UpstreamResponse { status, body });
            }
            let state = envelope::parse::<AuthState>(&body, "auth state")?.data("auth state")?;
            Ok(DeviceAuthorization {
                device_code: state.state,
                user_code: NO_USER_CODE.into(),
                verification_uri: state.auth_url,
                verification_uri_complete: None,
                expires_at_ms: None,
                // The plugin polls once a second; the browser round trip is
                // the only thing being waited on.
                interval_secs: 1,
                provider_state: BTreeMap::new(),
            })
        })
    }

    /// One step covers both polls: the tokens, then the account facts the
    /// request path needs. Either may still be pending.
    fn poll<'a>(
        &'a self,
        context: LoginContext<'a>,
        authorization: &'a DeviceAuthorization,
    ) -> OperationFuture<'a, DevicePoll> {
        Box::pin(async move {
            let config = WorkBuddyConfig::from_view(context.provider)?;
            let base = config.base_url(context.provider);
            let state = encode_component(&authorization.device_code);
            let (status, _, body) = send(
                context.client,
                Method::GET,
                &format!("{base}/v2/plugin/auth/token?state={state}"),
                {
                    let mut headers = login_headers();
                    opt_out(&mut headers, true);
                    headers
                },
                None,
            )
            .await?;
            if !status.is_success() {
                return Err(ChannelError::UpstreamResponse { status, body });
            }
            let wrapped = envelope::parse::<Tokens>(&body, "auth token")?;
            if wrapped.code == AUTH_PENDING {
                return Ok(DevicePoll::Pending);
            }
            let tokens = wrapped.data("auth token")?;

            let mut headers = login_headers();
            headers.insert(
                header::AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {}", tokens.access_token))
                    .map_err(|_| ChannelError::InvalidCredential)?,
            );
            domain_header(&mut headers, tokens.domain.as_deref(), &base)?;
            opt_out(&mut headers, false);
            let (status, _, body) = send(
                context.client,
                Method::GET,
                &format!("{base}/v2/plugin/login/account?state={state}"),
                headers,
                None,
            )
            .await?;
            if !status.is_success() {
                return Err(ChannelError::UpstreamResponse { status, body });
            }
            let wrapped = envelope::parse::<Account>(&body, "account")?;
            if wrapped.code == ACCOUNT_PENDING {
                return Ok(DevicePoll::Pending);
            }
            let account = wrapped.data("account")?;

            let mut fields = BTreeMap::new();
            // `user_id` is not decoration: the request path refuses to run
            // without it, so it is a login fact and lives with the others.
            field(&mut fields, "user_id", Some(account.uid));
            field(&mut fields, "domain", tokens.domain.clone());
            field(&mut fields, "nickname", account.nickname);
            field(&mut fields, "account_type", account.account_type);
            field(&mut fields, "enterprise_id", account.enterprise_id);
            field(&mut fields, "enterprise_name", account.enterprise_name);
            field(
                &mut fields,
                "department_full_name",
                account.department_full_name,
            );
            Ok(DevicePoll::Ready(OAuthCredential {
                access_token: tokens.access_token.clone(),
                refresh_token: Some(tokens.refresh_token.clone())
                    .filter(|token| !token.trim().is_empty()),
                id_token: None,
                token_type: Some("Bearer".into()),
                scopes: Vec::new(),
                expires_at_ms: tokens
                    .expires_at_ms()
                    .or_else(|| relative(Some(DEFAULT_EXPIRES_IN_SECS))),
                refresh_expires_at_ms: tokens.refresh_expires_at_ms(),
                provider_fields: fields,
                provider_secrets: BTreeMap::from([(
                    LAST_REFRESH_MS.to_owned(),
                    Value::from(unix_now_ms()),
                )]),
            }))
        })
    }
}

impl CredentialRefresh for super::WorkBuddy {
    /// The plugin's schedule rather than the default margin: it refreshes a
    /// day after the last refresh, and every five minutes when the expiry is
    /// unknown or within a day, so a token whose lifetime the gateway never
    /// stated is still renewed before a 401 logs the account out. A
    /// credential with no recorded refresh counts as never refreshed, as the
    /// plugin's `lastRefreshTime || 0` does.
    fn proactive_refresh_reason(
        &self,
        credential: CredentialView<'_>,
        now_ms: i64,
    ) -> Option<RefreshReason> {
        let refreshable = credential
            .secret
            .get("refresh_token")
            .and_then(Value::as_str)
            .is_some_and(|token| !token.trim().is_empty());
        if !refreshable {
            return None;
        }
        let expires = credential.expires_at_ms;
        if expires.is_some_and(|expires| expires <= now_ms.saturating_add(NEAR_EXPIRY_INTERVAL_MS))
        {
            return Some(RefreshReason::ProactiveExpiry);
        }
        let interval = match expires {
            Some(expires) if expires.saturating_sub(DAY_MS) >= now_ms => DAY_MS,
            _ => NEAR_EXPIRY_INTERVAL_MS,
        };
        let last_refresh = credential
            .secret
            .pointer(&format!("/provider_secrets/{LAST_REFRESH_MS}"))
            .and_then(Value::as_i64)
            .unwrap_or(0);
        (last_refresh.saturating_add(interval) < now_ms).then_some(RefreshReason::Scheduled)
    }

    fn refresh<'a>(&'a self, context: RefreshContext<'a>) -> OperationFuture<'a, CredentialUpdate> {
        Box::pin(async move {
            let config = WorkBuddyConfig::from_view(context.provider)?;
            let secret = context.credential.secret;
            let refresh_token = secret
                .get("refresh_token")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|token| !token.is_empty())
                .ok_or_else(|| {
                    ChannelError::RefreshRejected("credential has no refresh token".into())
                })?;
            let mut headers = login_headers();
            headers.insert(
                HeaderName::from_static("x-refresh-token"),
                HeaderValue::from_str(refresh_token)
                    .map_err(|_| ChannelError::InvalidCredential)?,
            );
            headers.insert(
                HeaderName::from_static("x-auth-refresh-source"),
                HeaderValue::from_static("plugin"),
            );
            // The refresh goes out with the session the plugin is renewing,
            // so its interceptor adds the bearer and the account facts.
            if let Ok(access) = super::auth::access_token(&context.credential) {
                headers.insert(
                    header::AUTHORIZATION,
                    HeaderValue::from_str(&format!("Bearer {access}"))
                        .map_err(|_| ChannelError::InvalidCredential)?,
                );
            }
            super::auth::account_headers(&mut headers, &context.credential)?;
            let base = config.base_url(context.provider);
            domain_header(
                &mut headers,
                super::auth::fact(&context.credential, "domain"),
                &base,
            )?;
            let (status, _, body) = send(
                context.client,
                Method::POST,
                &format!("{base}/v2/plugin/auth/token/refresh"),
                headers,
                Some(b"{}".to_vec()),
            )
            .await?;
            // The gateway answers 401 when the refresh token itself is
            // finished. Its own error codes are not classified: only the two
            // pending codes are documented, so anything else may be transient
            // and must not kill the credential.
            if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
                return Err(ChannelError::RefreshRejected(format!(
                    "http {}",
                    status.as_u16()
                )));
            }
            if !status.is_success() {
                return Err(ChannelError::UpstreamResponse { status, body });
            }
            let wrapped = envelope::parse::<Tokens>(&body, "token refresh")?;
            let code = wrapped.code;
            let message = wrapped.msg.clone();
            // The plugin logs out on an envelope code 401 answered to a call
            // carrying its token (`AuthenticationHttpInterceptor.checkToLogout`),
            // so that refusal ends the grant as surely as an HTTP 401.
            if code == 401 {
                return Err(ChannelError::RefreshRejected(format!(
                    "envelope 401: {message}"
                )));
            }
            let tokens = wrapped.data("token refresh").map_err(|error| {
                if code == AUTH_PENDING {
                    ChannelError::InvalidResponse(format!("{ID} refresh: {message} ({code})"))
                } else {
                    error
                }
            })?;
            if tokens.access_token.trim().is_empty() {
                return Err(ChannelError::InvalidResponse(format!(
                    "{ID} refresh: the envelope has no access_token"
                )));
            }
            let expires_at_ms = tokens.expires_at_ms();
            // A rotation replaces the tokens and the expiry and keeps
            // everything else, the account facts above all.
            let mut updated = secret.clone();
            let object = updated
                .as_object_mut()
                .ok_or(ChannelError::InvalidCredential)?;
            object.insert(
                "access_token".into(),
                Value::String(tokens.access_token.clone()),
            );
            if !tokens.refresh_token.trim().is_empty() {
                object.insert(
                    "refresh_token".into(),
                    Value::String(tokens.refresh_token.clone()),
                );
            }
            match expires_at_ms {
                Some(expires) => {
                    object.insert("expires_at_ms".into(), Value::from(expires));
                    object.remove("expiry_unknown");
                }
                // An expiry the gateway did not state is not invented; the
                // host then only learns the token is dead by being refused.
                None => {
                    object.remove("expires_at_ms");
                    object.insert("expiry_unknown".into(), Value::Bool(true));
                }
            }
            if let Some(expires) = tokens.refresh_expires_at_ms() {
                object.insert("refresh_expires_at_ms".into(), Value::from(expires));
            }
            let secrets = object
                .entry("provider_secrets")
                .or_insert_with(|| Value::Object(serde_json::Map::new()));
            if let Some(secrets) = secrets.as_object_mut() {
                secrets.insert(LAST_REFRESH_MS.into(), Value::from(unix_now_ms()));
            }
            if let Some(domain) = tokens.domain.as_deref().filter(|d| !d.trim().is_empty()) {
                let fields = object
                    .entry("provider_fields")
                    .or_insert_with(|| Value::Object(serde_json::Map::new()));
                if let Some(fields) = fields.as_object_mut() {
                    fields.insert("domain".into(), Value::String(domain.to_owned()));
                }
            }
            Ok(CredentialUpdate {
                metadata: None,
                secret: updated,
                expires_at_ms,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_absolute_expiry_wins_over_a_relative_one() {
        let tokens: Tokens = serde_json::from_value(
            json!({"accessToken": "a", "expiresIn": 60, "expiresAt": 1_900_000_000_000_i64}),
        )
        .unwrap();
        assert_eq!(tokens.expires_at_ms(), Some(1_900_000_000_000));

        let relative: Tokens =
            serde_json::from_value(json!({"accessToken": "a", "expiresIn": 60})).unwrap();
        assert!(relative.expires_at_ms().unwrap() > unix_now_ms());

        let silent: Tokens = serde_json::from_value(json!({"accessToken": "a"})).unwrap();
        assert_eq!(silent.expires_at_ms(), None);
    }

    #[test]
    fn the_envelope_reports_its_own_failures() {
        let wrapped =
            envelope::parse::<Tokens>(br#"{"code":40001,"msg":"nope","data":null}"#, "auth token")
                .unwrap();
        assert_eq!(wrapped.code, 40001);
        assert!(wrapped.data("auth token").is_err());
    }
}
