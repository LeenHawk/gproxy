//! The xAI device login the Grok Build CLI uses, and the refresh that keeps
//! it alive (v3 `grokbuild/{login,auth}.rs`).
//!
//! Both are form-encoded OAuth at `auth.x.ai`: `POST /oauth2/device/code`
//! starts, `POST /oauth2/token` polls with the device-code grant and later
//! refreshes. The token endpoint answers the same body for success and for
//! every OAuth error, so the body is read whatever the status.
//!
//! The id token is the only place the account's subject appears, and
//! `x-grok-user-id` needs it on every request. It is decoded once, here, and
//! travels in `provider_fields` — a login fact the host persists, not
//! something `prepare` reparses.

use super::config::{DEFAULT_CLIENT_ID, GrokBuildConfig, ID, OAUTH_SCOPE};
use super::{form_encode, unix_now_ms};
use crate::channel::{
    ChannelError, CredentialRefresh, CredentialUpdate, DeviceAuthorization, DevicePoll,
    LoginContext, OAuthCredential, OAuthDeviceCode, OperationFuture, RefreshContext,
};
use crate::channels::shared::compatible::ability::send;
use crate::channels::shared::compatible::http::invalid_response;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

/// What the upstream grants when it does not say.
const DEFAULT_EXPIRES_IN_SECS: i64 = 3_600;

fn form_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/x-www-form-urlencoded"),
    );
    headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
    headers
}

#[derive(Deserialize)]
struct DeviceCodeReply {
    device_code: String,
    user_code: String,
    #[serde(default)]
    verification_uri: Option<String>,
    #[serde(default)]
    verification_uri_complete: Option<String>,
    #[serde(default)]
    interval: Option<u64>,
    #[serde(default)]
    expires_in: Option<i64>,
}

#[derive(Deserialize)]
struct TokenReply {
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    id_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
    #[serde(default)]
    error: Option<String>,
}

impl TokenReply {
    fn access(&self) -> Option<&str> {
        self.access_token
            .as_deref()
            .map(str::trim)
            .filter(|token| !token.is_empty())
    }

    fn expires_at_ms(&self) -> i64 {
        unix_now_ms().saturating_add(
            self.expires_in
                .filter(|seconds| *seconds > 0)
                .unwrap_or(DEFAULT_EXPIRES_IN_SECS)
                .saturating_mul(1_000),
        )
    }
}

/// One claim of an unverified id token. The channel reads it for the account
/// facts it needs to present, never to decide anything about trust.
pub(super) fn jwt_claim(token: &str, name: &str) -> Option<String> {
    let payload = token.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload).ok()?;
    serde_json::from_slice::<Value>(&bytes)
        .ok()?
        .get(name)?
        .as_str()
        .map(str::to_owned)
}

/// `sub` and `email` out of an id token, as public account facts.
fn account_fields(id_token: Option<&str>, fields: &mut BTreeMap<String, Value>) {
    let Some(id_token) = id_token.map(str::trim).filter(|token| !token.is_empty()) else {
        return;
    };
    if let Some(subject) = jwt_claim(id_token, "sub") {
        fields.insert("sub".into(), Value::String(subject));
    }
    if let Some(email) = jwt_claim(id_token, "email") {
        fields.insert("user_email".into(), Value::String(email));
    }
}

impl OAuthDeviceCode for super::GrokBuild {
    fn start<'a>(&'a self, context: LoginContext<'a>) -> OperationFuture<'a, DeviceAuthorization> {
        Box::pin(async move {
            let config = GrokBuildConfig::from_view(context.provider)?;
            let (status, _, body) = send(
                context.client,
                Method::POST,
                config.device_code_url(),
                form_headers(),
                Some(
                    form_encode(&[("client_id", DEFAULT_CLIENT_ID), ("scope", OAUTH_SCOPE)])
                        .into_bytes(),
                ),
            )
            .await?;
            if !status.is_success() {
                return Err(ChannelError::UpstreamResponse { status, body });
            }
            let reply: DeviceCodeReply = serde_json::from_slice(&body)
                .map_err(|error| invalid_response(format!("{ID} device code: {error}")))?;
            let verification = reply
                .verification_uri_complete
                .clone()
                .or_else(|| reply.verification_uri.clone())
                .ok_or_else(|| {
                    invalid_response(format!("{ID} device code: no verification uri"))
                })?;
            Ok(DeviceAuthorization {
                device_code: reply.device_code,
                user_code: reply.user_code,
                verification_uri: verification,
                verification_uri_complete: reply.verification_uri_complete,
                expires_at_ms: reply
                    .expires_in
                    .filter(|seconds| *seconds > 0)
                    .map(|seconds| unix_now_ms().saturating_add(seconds.saturating_mul(1_000))),
                interval_secs: reply.interval.unwrap_or(5).max(1),
                provider_state: BTreeMap::new(),
            })
        })
    }

    fn poll<'a>(
        &'a self,
        context: LoginContext<'a>,
        authorization: &'a DeviceAuthorization,
    ) -> OperationFuture<'a, DevicePoll> {
        Box::pin(async move {
            let config = GrokBuildConfig::from_view(context.provider)?;
            let (status, _, body) = send(
                context.client,
                Method::POST,
                config.token_url(),
                form_headers(),
                Some(
                    form_encode(&[
                        ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                        ("client_id", DEFAULT_CLIENT_ID),
                        ("device_code", &authorization.device_code),
                    ])
                    .into_bytes(),
                ),
            )
            .await?;
            // A pending authorization is an error status with an OAuth code
            // in the body, so the body is read before the status is judged.
            let Ok(reply) = serde_json::from_slice::<TokenReply>(&body) else {
                return Err(ChannelError::UpstreamResponse { status, body });
            };
            if let Some(access) = reply.access() {
                let mut fields = BTreeMap::new();
                account_fields(reply.id_token.as_deref(), &mut fields);
                return Ok(DevicePoll::Ready(OAuthCredential {
                    access_token: access.to_owned(),
                    refresh_token: reply
                        .refresh_token
                        .clone()
                        .filter(|token| !token.trim().is_empty()),
                    id_token: reply.id_token.clone(),
                    token_type: Some("Bearer".into()),
                    scopes: OAUTH_SCOPE.split(' ').map(str::to_owned).collect(),
                    expires_at_ms: Some(reply.expires_at_ms()),
                    refresh_expires_at_ms: None,
                    provider_fields: fields,
                    provider_secrets: BTreeMap::new(),
                }));
            }
            match reply.error.as_deref() {
                Some("authorization_pending") => Ok(DevicePoll::Pending),
                Some("slow_down") => Ok(DevicePoll::SlowDown {
                    interval_secs: authorization.interval_secs.saturating_mul(2).max(5),
                }),
                Some("access_denied") => Ok(DevicePoll::Denied),
                Some("expired_token") => Ok(DevicePoll::Expired),
                _ => Err(ChannelError::UpstreamResponse { status, body }),
            }
        })
    }
}

impl CredentialRefresh for super::GrokBuild {
    fn refresh<'a>(&'a self, context: RefreshContext<'a>) -> OperationFuture<'a, CredentialUpdate> {
        Box::pin(async move {
            let config = GrokBuildConfig::from_view(context.provider)?;
            let secret = context.credential.secret;
            let refresh_token = secret
                .get("refresh_token")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|token| !token.is_empty())
                .ok_or_else(|| {
                    ChannelError::RefreshRejected("credential has no refresh token".into())
                })?;
            // A credential minted against a different issuer keeps renewing
            // against that one.
            let url = super::auth::fact(&context.credential, "token_endpoint")
                .unwrap_or(config.token_url())
                .to_owned();
            let (status, _, body) = send(
                context.client,
                Method::POST,
                &url,
                form_headers(),
                Some(
                    form_encode(&[
                        ("grant_type", "refresh_token"),
                        ("client_id", DEFAULT_CLIENT_ID),
                        ("refresh_token", refresh_token),
                    ])
                    .into_bytes(),
                ),
            )
            .await?;
            let reply = serde_json::from_slice::<TokenReply>(&body).ok();
            if !status.is_success() || reply.as_ref().is_none_or(|reply| reply.access().is_none()) {
                let code = reply.as_ref().and_then(|reply| reply.error.clone());
                // `invalid_grant` is the issuer saying this refresh token is
                // finished; anything else may be transient.
                if status == StatusCode::UNAUTHORIZED
                    || matches!(code.as_deref(), Some("invalid_grant" | "invalid_client"))
                {
                    return Err(ChannelError::RefreshRejected(
                        code.unwrap_or_else(|| format!("http {}", status.as_u16())),
                    ));
                }
                return Err(ChannelError::UpstreamResponse { status, body });
            }
            let reply = reply.expect("a token reply was just confirmed");
            let access = reply.access().expect("an access token was just confirmed");
            let expires_at_ms = reply.expires_at_ms();
            // A rotation replaces the tokens and the expiry and keeps
            // everything the reply did not name.
            let mut updated = secret.clone();
            let object = updated
                .as_object_mut()
                .ok_or(ChannelError::InvalidCredential)?;
            object.insert("access_token".into(), Value::String(access.to_owned()));
            if let Some(token) = reply
                .refresh_token
                .as_deref()
                .map(str::trim)
                .filter(|token| !token.is_empty())
            {
                object.insert("refresh_token".into(), Value::String(token.to_owned()));
            }
            object.insert("expires_at_ms".into(), Value::from(expires_at_ms));
            if let Some(id_token) = reply
                .id_token
                .as_deref()
                .map(str::trim)
                .filter(|token| !token.is_empty())
            {
                object.insert("id_token".into(), Value::String(id_token.to_owned()));
                let mut fields = BTreeMap::new();
                account_fields(Some(id_token), &mut fields);
                let existing = object
                    .entry("provider_fields")
                    .or_insert_with(|| Value::Object(serde_json::Map::new()));
                if let Some(existing) = existing.as_object_mut() {
                    for (name, value) in fields {
                        existing.insert(name, value);
                    }
                }
            }
            Ok(CredentialUpdate {
                secret: updated,
                expires_at_ms: Some(expires_at_ms),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `{"sub":"user-1","email":"a@b.c"}`, unsigned and unpadded.
    fn id_token() -> String {
        let payload = URL_SAFE_NO_PAD.encode(br#"{"sub":"user-1","email":"a@b.c"}"#);
        format!("header.{payload}.signature")
    }

    #[test]
    fn the_subject_and_the_email_come_out_of_the_id_token() {
        let mut fields = BTreeMap::new();
        account_fields(Some(&id_token()), &mut fields);
        assert_eq!(fields["sub"], "user-1");
        assert_eq!(fields["user_email"], "a@b.c");

        let mut none = BTreeMap::new();
        account_fields(Some("not.a.jwt"), &mut none);
        assert!(none.is_empty(), "an unreadable token adds nothing");
    }

    #[test]
    fn the_device_grant_is_form_encoded_with_its_colons_escaped() {
        let body = form_encode(&[
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("client_id", "abc"),
        ]);
        assert_eq!(
            body,
            "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code&client_id=abc"
        );
    }
}
