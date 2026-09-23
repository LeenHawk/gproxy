//! OAuth login, device authorization and credential refresh.

use super::common::{invalid_response, send_json};
use super::{Codex, CodexConfig, DEFAULT_CLIENT_ID};
use crate::OutboundClient;
use crate::channel::{
    AuthorizationCode, AuthorizationRequest, AuthorizationStart, ChannelError, CredentialRefresh,
    CredentialUpdate, DeviceAuthorization, DevicePoll, LoginContext, OAuthAuthorizationCode,
    OAuthCredential, OAuthDeviceCode, OperationFuture, RefreshContext,
};
use base64::Engine;
use http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

const OAUTH_SCOPE: &str =
    "openid profile email offline_access api.connectors.read api.connectors.invoke";

/// Decode a JWT payload without verifying it: the issuer's tokens are
/// trusted material already; the claims only supply account facts.
fn jwt_claims(token: &str) -> Option<Value> {
    let payload = token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn jwt_expiry_ms(token: &str) -> Option<i64> {
    jwt_claims(token)?
        .get("exp")?
        .as_i64()
        .and_then(|secs| secs.checked_mul(1000))
}

/// Account facts from the id token: the ChatGPT account, plan and user under
/// the `https://api.openai.com/auth` claim, the email under `profile`.
fn provider_fields(id_token: Option<&str>) -> BTreeMap<String, Value> {
    let mut fields = BTreeMap::new();
    let Some(claims) = id_token.and_then(jwt_claims) else {
        return fields;
    };
    if let Some(auth) = claims.get("https://api.openai.com/auth") {
        for (from, to) in [
            ("chatgpt_account_id", "chatgpt_account_id"),
            ("chatgpt_plan_type", "chatgpt_plan_type"),
            ("chatgpt_user_id", "chatgpt_user_id"),
        ] {
            if let Some(value) = auth.get(from).filter(|v| !v.is_null()) {
                fields.insert(to.to_owned(), value.clone());
            }
        }
        if !fields.contains_key("chatgpt_user_id")
            && let Some(user) = auth.get("user_id").filter(|v| !v.is_null())
        {
            fields.insert("chatgpt_user_id".to_owned(), user.clone());
        }
    }
    let email = claims
        .pointer("/https:~1~1api.openai.com~1profile/email")
        .or_else(|| claims.get("email"))
        .filter(|v| !v.is_null());
    if let Some(email) = email {
        fields.insert("email".to_owned(), email.clone());
    }
    fields
}

fn credential_from_tokens(
    access_token: String,
    refresh_token: Option<String>,
    id_token: Option<String>,
) -> OAuthCredential {
    OAuthCredential {
        expires_at_ms: jwt_expiry_ms(&access_token),
        provider_fields: provider_fields(id_token.as_deref()),
        provider_secrets: BTreeMap::new(),
        access_token,
        refresh_token,
        id_token,
        token_type: Some("Bearer".into()),
        scopes: OAUTH_SCOPE.split(' ').map(str::to_owned).collect(),
        refresh_expires_at_ms: None,
    }
}

fn json_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers
}

fn form_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/x-www-form-urlencoded"),
    );
    headers
}

fn form_encode(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("{k}={}", form_urlencoded_encode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

fn form_urlencoded_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            b' ' => out.push('+'),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    id_token: Option<String>,
}

/// The token endpoint's error code, at `error.code`, `error` or `code`.
fn token_error_code(body: &[u8]) -> Option<String> {
    let value: Value = serde_json::from_slice(body).ok()?;
    match value.get("error") {
        Some(Value::Object(error)) => error.get("code").and_then(Value::as_str).map(str::to_owned),
        Some(Value::String(code)) => Some(code.clone()),
        _ => value.get("code").and_then(Value::as_str).map(str::to_owned),
    }
}

async fn exchange_code(
    client: &dyn OutboundClient,
    config: &CodexConfig,
    code: &str,
    redirect_uri: &str,
    code_verifier: &str,
) -> Result<OAuthCredential, ChannelError> {
    let url = format!("{}/oauth/token", config.issuer.trim_end_matches('/'));
    let body = form_encode(&[
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", DEFAULT_CLIENT_ID),
        ("code_verifier", code_verifier),
    ]);
    let (status, _, bytes) = send_json(
        client,
        Method::POST,
        &url,
        form_headers(),
        Some(body.into_bytes()),
    )
    .await?;
    if !status.is_success() {
        return Err(ChannelError::UpstreamResponse {
            status,
            body: bytes,
        });
    }
    let tokens: TokenResponse =
        serde_json::from_slice(&bytes).map_err(|e| invalid_response(e.to_string()))?;
    let access_token = tokens
        .access_token
        .filter(|t| !t.is_empty())
        .ok_or_else(|| invalid_response("token exchange returned no access_token"))?;
    Ok(credential_from_tokens(
        access_token,
        tokens.refresh_token,
        tokens.id_token,
    ))
}

impl OAuthAuthorizationCode for Codex {
    fn authorize<'a>(
        &'a self,
        context: LoginContext<'a>,
        request: AuthorizationRequest<'a>,
    ) -> OperationFuture<'a, AuthorizationStart> {
        Box::pin(async move {
            let config = CodexConfig::from_view(context.provider)?;
            let query = form_encode(&[
                ("response_type", "code"),
                ("client_id", DEFAULT_CLIENT_ID),
                ("redirect_uri", request.redirect_uri),
                ("scope", OAUTH_SCOPE),
                ("code_challenge", request.code_challenge),
                ("code_challenge_method", "S256"),
                ("id_token_add_organizations", "true"),
                ("codex_cli_simplified_flow", "true"),
                ("state", request.state),
                ("originator", &config.originator),
            ]);
            Ok(AuthorizationStart {
                authorize_url: format!(
                    "{}/oauth/authorize?{query}",
                    config.issuer.trim_end_matches('/')
                ),
                redirect_uri: request.redirect_uri.to_owned(),
                // The exchange needs nothing this call learned.
                provider_state: BTreeMap::new(),
            })
        })
    }

    fn exchange<'a>(
        &'a self,
        context: LoginContext<'a>,
        grant: AuthorizationCode<'a>,
    ) -> OperationFuture<'a, OAuthCredential> {
        Box::pin(async move {
            let config = CodexConfig::from_view(context.provider)?;
            exchange_code(
                context.client,
                &config,
                grant.code,
                grant.redirect_uri,
                grant.code_verifier,
            )
            .await
        })
    }
}

#[derive(Deserialize)]
struct UserCodeResponse {
    device_auth_id: String,
    #[serde(alias = "usercode")]
    user_code: String,
    #[serde(default, deserialize_with = "lenient_u64")]
    interval: u64,
}

fn lenient_u64<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
    let value = Value::deserialize(deserializer)?;
    Ok(match value {
        Value::Number(n) => n.as_u64().unwrap_or(5),
        Value::String(s) => s.trim().parse().unwrap_or(5),
        _ => 5,
    })
}

#[derive(Deserialize)]
struct DeviceTokenResponse {
    authorization_code: String,
    code_verifier: String,
}

impl OAuthDeviceCode for Codex {
    fn start<'a>(&'a self, context: LoginContext<'a>) -> OperationFuture<'a, DeviceAuthorization> {
        Box::pin(async move {
            let config = CodexConfig::from_view(context.provider)?;
            let issuer = config.issuer.trim_end_matches('/');
            let (status, _, bytes) = send_json(
                context.client,
                Method::POST,
                &format!("{issuer}/api/accounts/deviceauth/usercode"),
                json_headers(),
                Some(
                    json!({"client_id": DEFAULT_CLIENT_ID})
                        .to_string()
                        .into_bytes(),
                ),
            )
            .await?;
            if !status.is_success() {
                return Err(ChannelError::UpstreamResponse {
                    status,
                    body: bytes,
                });
            }
            let response: UserCodeResponse =
                serde_json::from_slice(&bytes).map_err(|e| invalid_response(e.to_string()))?;
            Ok(DeviceAuthorization {
                device_code: response.device_auth_id,
                user_code: response.user_code,
                verification_uri: format!("{issuer}/codex/device"),
                verification_uri_complete: None,
                // The issuer allows fifteen minutes; the host knows the clock.
                expires_at_ms: None,
                interval_secs: response.interval.max(1),
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
            let config = CodexConfig::from_view(context.provider)?;
            let issuer = config.issuer.trim_end_matches('/');
            let (status, _, bytes) = send_json(
                context.client,
                Method::POST,
                &format!("{issuer}/api/accounts/deviceauth/token"),
                json_headers(),
                Some(
                    json!({
                        "device_auth_id": authorization.device_code,
                        "user_code": authorization.user_code,
                    })
                    .to_string()
                    .into_bytes(),
                ),
            )
            .await?;
            match status {
                StatusCode::FORBIDDEN | StatusCode::NOT_FOUND => return Ok(DevicePoll::Pending),
                StatusCode::TOO_MANY_REQUESTS => {
                    return Ok(DevicePoll::SlowDown {
                        interval_secs: authorization.interval_secs.saturating_mul(2).max(5),
                    });
                }
                StatusCode::GONE => return Ok(DevicePoll::Expired),
                status if !status.is_success() => {
                    return Err(ChannelError::UpstreamResponse {
                        status,
                        body: bytes,
                    });
                }
                _ => {}
            }
            let response: DeviceTokenResponse =
                serde_json::from_slice(&bytes).map_err(|e| invalid_response(e.to_string()))?;
            let credential = exchange_code(
                context.client,
                &config,
                &response.authorization_code,
                &format!("{issuer}/deviceauth/callback"),
                &response.code_verifier,
            )
            .await?;
            Ok(DevicePoll::Ready(credential))
        })
    }
}

impl CredentialRefresh for Codex {
    fn refresh<'a>(&'a self, context: RefreshContext<'a>) -> OperationFuture<'a, CredentialUpdate> {
        Box::pin(async move {
            let config = CodexConfig::from_view(context.provider)?;
            let secret = context.credential.secret;
            let refresh_token = secret
                .get("refresh_token")
                .and_then(Value::as_str)
                .filter(|t| !t.is_empty())
                .ok_or_else(|| {
                    ChannelError::RefreshRejected("credential has no refresh token".into())
                })?;
            let url = format!("{}/oauth/token", config.issuer.trim_end_matches('/'));
            let body = json!({
                "client_id": DEFAULT_CLIENT_ID,
                "grant_type": "refresh_token",
                "refresh_token": refresh_token,
            });
            let (status, _, bytes) = send_json(
                context.client,
                Method::POST,
                &url,
                json_headers(),
                Some(body.to_string().into_bytes()),
            )
            .await?;
            if !status.is_success() {
                let code = token_error_code(&bytes).map(|c| c.to_ascii_lowercase());
                let definitive = status == StatusCode::UNAUTHORIZED
                    || matches!(
                        code.as_deref(),
                        Some(
                            "refresh_token_expired"
                                | "refresh_token_reused"
                                | "refresh_token_invalidated"
                        )
                    )
                    || (status == StatusCode::BAD_REQUEST
                        && code.as_deref() == Some("invalid_grant"));
                if definitive {
                    return Err(ChannelError::RefreshRejected(
                        code.unwrap_or_else(|| format!("http {}", status.as_u16())),
                    ));
                }
                return Err(ChannelError::UpstreamResponse {
                    status,
                    body: bytes,
                });
            }
            let tokens: TokenResponse =
                serde_json::from_slice(&bytes).map_err(|e| invalid_response(e.to_string()))?;
            let access_token = tokens
                .access_token
                .filter(|t| !t.is_empty())
                .ok_or_else(|| invalid_response("refresh returned no access_token"))?;
            let refresh_token = tokens
                .refresh_token
                .filter(|t| !t.is_empty())
                .or_else(|| Some(refresh_token.to_owned()));
            let id_token = tokens.id_token.filter(|t| !t.is_empty()).or_else(|| {
                secret
                    .get("id_token")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            });
            let mut credential = credential_from_tokens(access_token, refresh_token, id_token);
            // Keep account facts a login recorded that the new id token lacks.
            if let Some(Value::Object(previous)) = secret.get("provider_fields") {
                for (key, value) in previous {
                    credential
                        .provider_fields
                        .entry(key.clone())
                        .or_insert_with(|| value.clone());
                }
            }
            let expires_at_ms = credential.expires_at_ms;
            let secret =
                serde_json::to_value(&credential).map_err(|e| invalid_response(e.to_string()))?;
            Ok(CredentialUpdate {
                secret,
                expires_at_ms,
            })
        })
    }
}
