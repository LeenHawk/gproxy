//! How a Kiro credential comes into existence and how it stays alive.
//!
//! Two products issue one, and they are different upstreams (v3
//! `kiro/{login,auth}.rs`):
//!
//! * The **Kiro desktop app** runs its own device flow at
//!   `prod.us-east-1.auth.desktop.kiro.dev`: `POST /oauth/device/authorization`
//!   starts it with `{clientId: "Kiro-CLI", loginProvider}`, `POST
//!   /oauth/device/poll` returns `{status}` until `authorized`, and `POST
//!   /refreshToken` renews with `{refreshToken}`. No client secret exists.
//! * **IAM Identity Center** issues one through AWS OIDC at
//!   `oidc.{region}.amazonaws.com`: the browser step is `/authorize` with
//!   PKCE, the exchange and the refresh are `/token` with a registered
//!   client id and secret.
//!
//! v3 registered that OIDC client itself, during `authcode_start`, and carried
//! the resulting id and secret to the exchange in the login session's `extra`
//! field. v4's `OAuthAuthorizationCode` has no state channel between
//! `authorize` and `exchange`, and a client secret must not travel through
//! `provider_fields` because the host publishes those as credential metadata.
//! So the pair is provider configuration here: an operator registers the
//! client once and writes `sso_client_id`/`sso_client_secret`. A credential
//! imported from v3 keeps refreshing either way, because the flat secret is
//! read as a fallback.
//!
//! Facts the login learns — the profile ARN the runtime plane needs, the
//! region and portal an IdC credential belongs to — travel in
//! `provider_fields`, and `prepare` reads them back out of the metadata.

use super::config::{DEFAULT_REDIRECT_URI, ID, KiroConfig, OAUTH_SCOPE, validate_region};
use super::{fact, form_encode, json_headers, send, unix_now_ms};
use crate::channel::{
    AuthorizationCode, AuthorizationRequest, AuthorizationStart, ChannelError, CredentialRefresh,
    CredentialUpdate, CredentialView, DeviceAuthorization, DevicePoll, LoginContext,
    OAuthAuthorizationCode, OAuthCredential, OAuthDeviceCode, OperationFuture, RefreshContext,
};
use http::{Method, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// The client name both the device authorization and the OIDC registration
/// use (v3 `login.rs`).
const CLIENT_NAME: &str = "Kiro-CLI";
/// What the upstream grants when it does not say.
const DEFAULT_EXPIRES_IN_SECS: i64 = 3_600;

fn invalid_response(message: impl Into<String>) -> ChannelError {
    ChannelError::InvalidResponse(format!("{ID}: {}", message.into()))
}

fn oidc_base(region: &str) -> Result<String, ChannelError> {
    validate_region(region)?;
    Ok(format!("https://oidc.{region}.amazonaws.com"))
}

fn expires_at_ms(seconds: Option<i64>) -> i64 {
    unix_now_ms().saturating_add(
        seconds
            .filter(|seconds| *seconds > 0)
            .unwrap_or(DEFAULT_EXPIRES_IN_SECS)
            .saturating_mul(1_000),
    )
}

fn required(value: Option<String>, name: &str) -> Result<String, ChannelError> {
    value
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| invalid_response(format!("the reply has no {name}")))
}

fn field(fields: &mut BTreeMap<String, Value>, name: &str, value: Option<String>) {
    if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
        fields.insert(name.into(), Value::String(value));
    }
}

fn credential(
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    provider_fields: BTreeMap<String, Value>,
) -> OAuthCredential {
    OAuthCredential {
        access_token,
        refresh_token,
        id_token: None,
        token_type: Some("Bearer".into()),
        scopes: OAUTH_SCOPE.split(' ').map(str::to_owned).collect(),
        expires_at_ms: Some(expires_at_ms(expires_in)),
        refresh_expires_at_ms: None,
        provider_fields,
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeviceCodeReply {
    device_code: String,
    user_code: String,
    #[serde(default)]
    verification_uri: Option<String>,
    #[serde(default)]
    verification_uri_complete: Option<String>,
    /// The desktop app states its polling interval in milliseconds.
    #[serde(default)]
    interval_in_milliseconds: Option<u64>,
    #[serde(default)]
    expires_in: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DevicePollReply {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    profile_arn: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
}

/// AWS OIDC and the desktop auth host answer a token with the same fields,
/// in AWS's camelCase.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TokenReply {
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    profile_arn: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
}

impl OAuthDeviceCode for super::Kiro {
    fn start<'a>(&'a self, context: LoginContext<'a>) -> OperationFuture<'a, DeviceAuthorization> {
        Box::pin(async move {
            let config = KiroConfig::from_view(context.provider)?;
            let body = json!({"clientId": CLIENT_NAME, "loginProvider": config.login_provider()});
            let (status, _, bytes) = send(
                context.client,
                Method::POST,
                &format!("{}/oauth/device/authorization", config.auth_base_url()),
                json_headers(),
                Some(body.to_string().into_bytes()),
            )
            .await?;
            if !status.is_success() {
                return Err(ChannelError::UpstreamResponse {
                    status,
                    body: bytes,
                });
            }
            let reply: DeviceCodeReply = serde_json::from_slice(&bytes)
                .map_err(|error| invalid_response(format!("device code: {error}")))?;
            let verification = reply
                .verification_uri_complete
                .clone()
                .or_else(|| reply.verification_uri.clone())
                .ok_or_else(|| invalid_response("device code: no verification uri"))?;
            Ok(DeviceAuthorization {
                device_code: reply.device_code,
                user_code: reply.user_code,
                verification_uri: verification,
                verification_uri_complete: reply.verification_uri_complete,
                expires_at_ms: reply
                    .expires_in
                    .filter(|seconds| *seconds > 0)
                    .map(|seconds| unix_now_ms().saturating_add(seconds.saturating_mul(1_000))),
                interval_secs: reply
                    .interval_in_milliseconds
                    .map(|milliseconds| (milliseconds / 1_000).max(1))
                    .unwrap_or(5),
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
            let config = KiroConfig::from_view(context.provider)?;
            let body = json!({"deviceCode": authorization.device_code, "clientId": CLIENT_NAME});
            let (status, _, bytes) = send(
                context.client,
                Method::POST,
                &format!("{}/oauth/device/poll", config.auth_base_url()),
                json_headers(),
                Some(body.to_string().into_bytes()),
            )
            .await?;
            let Ok(reply) = serde_json::from_slice::<DevicePollReply>(&bytes) else {
                return Err(ChannelError::UpstreamResponse {
                    status,
                    body: bytes,
                });
            };
            match reply.status.as_deref() {
                Some("authorization_pending") => Ok(DevicePoll::Pending),
                Some("slow_down") => Ok(DevicePoll::SlowDown {
                    interval_secs: authorization.interval_secs.saturating_mul(2).max(5),
                }),
                Some("expired_token") => Ok(DevicePoll::Expired),
                Some("authorized") => {
                    let access = required(reply.access_token, "accessToken")?;
                    let refresh = required(reply.refresh_token, "refreshToken")?;
                    let mut fields = BTreeMap::new();
                    field(&mut fields, "profile_arn", reply.profile_arn);
                    Ok(DevicePoll::Ready(credential(
                        access,
                        Some(refresh),
                        reply.expires_in,
                        fields,
                    )))
                }
                // `access_denied` and anything the desktop app has not
                // documented both mean this attempt is over.
                Some(_) => Ok(DevicePoll::Denied),
                None => Err(ChannelError::UpstreamResponse {
                    status,
                    body: bytes,
                }),
            }
        })
    }
}

impl OAuthAuthorizationCode for super::Kiro {
    /// The AWS OIDC browser step. The registered client is configuration, not
    /// something this call can mint — see the module note.
    fn authorize<'a>(
        &'a self,
        context: LoginContext<'a>,
        request: AuthorizationRequest<'a>,
    ) -> OperationFuture<'a, AuthorizationStart> {
        Box::pin(async move {
            let config = KiroConfig::from_view(context.provider)?;
            let client_id = config
                .sso_client_id
                .as_deref()
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .ok_or_else(|| {
                    ChannelError::InvalidConfig(
                        "an authorization-code login needs a registered `sso_client_id`".into(),
                    )
                })?;
            let redirect_uri = match request.redirect_uri.trim() {
                "" => DEFAULT_REDIRECT_URI,
                uri => uri,
            };
            let query = form_encode(&[
                ("response_type", "code"),
                ("client_id", client_id),
                ("redirect_uri", redirect_uri),
                ("scopes", OAUTH_SCOPE),
                ("state", request.state),
                ("code_challenge", request.code_challenge),
                ("code_challenge_method", "S256"),
            ]);
            Ok(AuthorizationStart {
                authorize_url: format!("{}/authorize?{query}", oidc_base(config.region()?)?),
                redirect_uri: redirect_uri.to_owned(),
            })
        })
    }

    fn exchange<'a>(
        &'a self,
        context: LoginContext<'a>,
        grant: AuthorizationCode<'a>,
    ) -> OperationFuture<'a, OAuthCredential> {
        Box::pin(async move {
            let config = KiroConfig::from_view(context.provider)?;
            let (client_id, client_secret) = sso_client(&config, None)?;
            let region = config.region()?.to_owned();
            let body = json!({
                "grantType": "authorization_code",
                "clientId": client_id,
                "clientSecret": client_secret,
                "code": grant.code,
                "redirectUri": grant.redirect_uri,
                "codeVerifier": grant.code_verifier,
            });
            let (status, _, bytes) = send(
                context.client,
                Method::POST,
                &format!("{}/token", oidc_base(&region)?),
                json_headers(),
                Some(body.to_string().into_bytes()),
            )
            .await?;
            if !status.is_success() {
                return Err(ChannelError::UpstreamResponse {
                    status,
                    body: bytes,
                });
            }
            let reply: TokenReply = serde_json::from_slice(&bytes)
                .map_err(|error| invalid_response(format!("token: {error}")))?;
            let access = required(reply.access_token, "accessToken")?;
            let refresh = required(reply.refresh_token, "refreshToken")?;
            let mut fields = BTreeMap::new();
            // The region and the portal say which OIDC host renews this
            // credential; the client secret stays in configuration.
            field(&mut fields, "region", Some(region));
            field(
                &mut fields,
                "start_url",
                Some(config.start_url().to_owned()),
            );
            field(&mut fields, "profile_arn", reply.profile_arn);
            Ok(credential(access, Some(refresh), reply.expires_in, fields))
        })
    }
}

/// The registered IdC client: provider configuration first, then the flat
/// secret a v3 credential carries.
fn sso_client(
    config: &KiroConfig,
    credential: Option<&CredentialView<'_>>,
) -> Result<(String, String), ChannelError> {
    let from_secret = |name: &str| {
        credential.and_then(|credential| {
            credential
                .secret
                .get(name)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        })
    };
    let id = config
        .sso_client_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .or_else(|| from_secret("client_id"));
    let secret = config
        .sso_client_secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .or_else(|| from_secret("client_secret"));
    id.zip(secret).ok_or_else(|| {
        ChannelError::InvalidConfig(
            "an Identity Center credential needs `sso_client_id` and `sso_client_secret`".into(),
        )
    })
}

/// An Identity Center credential is one that names a registered client, in
/// its own secret or in provider configuration together with a region.
fn is_identity_center(config: &KiroConfig, credential: &CredentialView<'_>) -> bool {
    fact(credential, "client_id").is_some() || fact(credential, "region").is_some() || {
        config.sso_client_id.is_some() && config.sso_client_secret.is_some()
    }
}

/// AWS names a definitive refusal in `__type` or `error`; anything else may
/// be transient and must not kill the credential.
fn definitive(status: StatusCode, body: &[u8]) -> Option<String> {
    if status == StatusCode::UNAUTHORIZED {
        return Some(format!("http {}", status.as_u16()));
    }
    if !status.is_client_error() {
        return None;
    }
    let value: Value = serde_json::from_slice(body).ok()?;
    let code = ["__type", "error", "message"]
        .into_iter()
        .find_map(|name| value.get(name).and_then(Value::as_str))?;
    let lowered = code.to_ascii_lowercase();
    (lowered.contains("invalid_grant")
        || lowered.contains("invalidgrant")
        || lowered.contains("invalid_client")
        || lowered.contains("unauthorized")
        || lowered.contains("accessdenied"))
    .then(|| code.to_owned())
}

impl CredentialRefresh for super::Kiro {
    fn refresh<'a>(&'a self, context: RefreshContext<'a>) -> OperationFuture<'a, CredentialUpdate> {
        Box::pin(async move {
            let config = KiroConfig::from_view(context.provider)?;
            let secret = context.credential.secret;
            let refresh_token = secret
                .get("refresh_token")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|token| !token.is_empty())
                .ok_or_else(|| {
                    ChannelError::RefreshRejected("credential has no refresh token".into())
                })?;
            let (url, body) = if is_identity_center(&config, &context.credential) {
                let (client_id, client_secret) = sso_client(&config, Some(&context.credential))?;
                let region = fact(&context.credential, "region")
                    .map(str::to_owned)
                    .unwrap_or_else(|| config.region.clone());
                (
                    format!("{}/token", oidc_base(region.trim())?),
                    json!({
                        "clientId": client_id,
                        "clientSecret": client_secret,
                        "refreshToken": refresh_token,
                        "grantType": "refresh_token",
                    }),
                )
            } else {
                (
                    format!("{}/refreshToken", config.auth_base_url()),
                    json!({ "refreshToken": refresh_token }),
                )
            };
            let (status, _, bytes) = send(
                context.client,
                Method::POST,
                &url,
                json_headers(),
                Some(body.to_string().into_bytes()),
            )
            .await?;
            if !status.is_success() {
                if let Some(code) = definitive(status, &bytes) {
                    return Err(ChannelError::RefreshRejected(code));
                }
                return Err(ChannelError::UpstreamResponse {
                    status,
                    body: bytes,
                });
            }
            let reply: TokenReply = serde_json::from_slice(&bytes)
                .map_err(|error| invalid_response(format!("refresh: {error}")))?;
            let access = required(reply.access_token, "accessToken")?;
            let expires_at_ms = expires_at_ms(reply.expires_in);
            // A rotation replaces the tokens and the expiry and keeps
            // everything the reply did not name, the profile ARN above all.
            let mut updated = secret.clone();
            let object = updated
                .as_object_mut()
                .ok_or(ChannelError::InvalidCredential)?;
            object.insert("access_token".into(), Value::String(access));
            if let Some(token) = reply.refresh_token.filter(|token| !token.trim().is_empty()) {
                object.insert("refresh_token".into(), Value::String(token));
            }
            object.insert("expires_at_ms".into(), Value::from(expires_at_ms));
            if let Some(arn) = reply.profile_arn.filter(|arn| !arn.trim().is_empty()) {
                let fields = object
                    .entry("provider_fields")
                    .or_insert_with(|| Value::Object(serde_json::Map::new()));
                if let Some(fields) = fields.as_object_mut() {
                    fields.insert("profile_arn".into(), Value::String(arn));
                }
            }
            Ok(CredentialUpdate {
                secret: updated,
                expires_at_ms: Some(expires_at_ms),
            })
        })
    }
}

/// The authorize URL is assembled here rather than by a URL crate, so the
/// escaping is pinned by a test.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_region_may_only_look_like_a_region() {
        assert!(oidc_base("us-east-1").is_ok());
        assert!(oidc_base("us-east-1/../evil").is_err());
        assert!(oidc_base("").is_err());
    }

    #[test]
    fn only_a_named_refusal_is_definitive() {
        assert!(definitive(StatusCode::UNAUTHORIZED, b"").is_some());
        assert_eq!(
            definitive(
                StatusCode::BAD_REQUEST,
                br#"{"__type":"InvalidGrantException"}"#
            )
            .as_deref(),
            Some("InvalidGrantException")
        );
        assert!(
            definitive(
                StatusCode::BAD_REQUEST,
                br#"{"__type":"ThrottlingException"}"#
            )
            .is_none()
        );
        assert!(definitive(StatusCode::BAD_GATEWAY, b"down").is_none());
    }

    #[test]
    fn the_scope_survives_being_a_query_value() {
        let query = form_encode(&[("scopes", OAUTH_SCOPE)]);
        assert_eq!(
            query,
            "scopes=codewhisperer%3Acompletions%20codewhisperer%3Aanalysis%20codewhisperer%3Aconversations"
        );
        assert_eq!(super::super::encode_component("a b"), "a%20b");
    }
}
