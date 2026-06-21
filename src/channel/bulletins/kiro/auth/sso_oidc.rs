//! Kiro SSO-OIDC login — AWS Builder ID + IAM Identity Center (IdC).
//!
//! Both are the SAME AWS SSO-OIDC authcode + PKCE flow against
//! `https://oidc.{region}.amazonaws.com` (REST-JSON: real paths, `application/
//! json`, NO `x-amz-target`, NO SigV4 — auth is the body `clientId`/
//! `clientSecret`). They differ only in `start_url` + `region`:
//!   * **builderId** — `start_url = https://view.awsapps.com/start`, `us-east-1`.
//!   * **idc** — operator `params.start_url` + `params.region`.
//!
//! Flow (replicates kiro-cli `fig_auth::{builder_id,pkce}`, PKCE-only — the CLI's
//! default; device-code is only its no-browser fallback):
//!   1. RegisterClient  `POST /client/register`  → `{clientId, clientSecret}`.
//!   2. authorize URL   `GET  /authorize?…&scopes=<space-joined>&code_challenge_method=S256`.
//!   3. CreateToken     `POST /token` grant=`authorization_code` → tokens.
//!   4. refresh         `POST /token` grant=`refresh_token` (see [`refresh`]).
//!
//! The registered client creds + region + start_url ride the login session
//! `extra` so [`authcode_exchange`] can mint the token; the minted secret carries
//! them so [`refresh`] (and the refresh discriminator) work without re-register.

use std::sync::Arc;

use serde::Deserialize;
use serde_json::{Value, json};

use crate::channel::ChannelError;
use crate::channel::login::AuthCodeStart;
use crate::channel::oauth;
use crate::http::client::UpstreamClient;

use super::{json_post, now_ms, secret_str};

/// OAuth client name kiro registers (RE: kiro uses `Kiro-CLI`; upstream Q used
/// "Amazon Q Developer for command line").
const CLIENT_NAME: &str = "Kiro-CLI";
/// Public client (PKCE, no confidential secret beyond the registration secret).
const CLIENT_TYPE: &str = "public";
/// Builder ID start_url + region (IdC overrides both via params).
const BUILDER_ID_START_URL: &str = "https://view.awsapps.com/start";
const BUILDER_ID_REGION: &str = "us-east-1";
/// SSO-OIDC scopes. kiro's `api.oidc.scopePrefix` defaults to `codewhisperer`.
const SCOPES: [&str; 3] = [
    "codewhisperer:completions",
    "codewhisperer:analysis",
    "codewhisperer:conversations",
];
/// Default loopback redirect when the console passes none (code-only mode: the
/// operator copies the `?code=…` URL from the browser address bar; callback-URL
/// mode passes its own listener URL). Mirrors the codex convention.
const DEFAULT_REDIRECT_URI: &str = "http://127.0.0.1:1455/oauth/callback";

/// `POST /client/register` response (camelCase).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegisterClientResponse {
    client_id: Option<String>,
    client_secret: Option<String>,
}

/// `POST /token` response (camelCase). `tokenType` (always `Bearer`) is ignored.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateTokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
}

/// The regional SSO-OIDC host.
fn oidc_base(region: &str) -> String {
    format!("https://oidc.{region}.amazonaws.com")
}

/// Resolve `(start_url, region)` from operator params. `auth_method=idc` takes
/// `params.start_url` + `params.region`; builderId (the default) uses the
/// Builder ID constants.
fn target(params: &Value) -> (String, String) {
    if params.get("auth_method").and_then(Value::as_str) == Some("idc") {
        let start_url = secret_str(params, "start_url")
            .unwrap_or(BUILDER_ID_START_URL)
            .to_string();
        let region = secret_str(params, "region")
            .unwrap_or(BUILDER_ID_REGION)
            .to_string();
        (start_url, region)
    } else {
        (
            BUILDER_ID_START_URL.to_string(),
            BUILDER_ID_REGION.to_string(),
        )
    }
}

/// Begin the SSO-OIDC authcode+PKCE login: RegisterClient a fresh public client,
/// then build the `/authorize` URL. Stashes `{client_id, client_secret, region,
/// start_url}` in `extra` for [`authcode_exchange`].
pub(super) async fn authcode_start(
    client: &Arc<dyn UpstreamClient>,
    params: &Value,
    redirect_uri: &str,
    state: &str,
    challenge: &str,
) -> Result<AuthCodeStart, ChannelError> {
    let (start_url, region) = target(params);
    let redirect_uri = if redirect_uri.trim().is_empty() {
        DEFAULT_REDIRECT_URI
    } else {
        redirect_uri
    };
    let base = oidc_base(&region);

    // RegisterClient: a public dynamic client scoped to authcode + refresh grants
    // with our redirect — its creds are reused on refresh, so they are persisted.
    let reg_body = json!({
        "clientName": CLIENT_NAME,
        "clientType": CLIENT_TYPE,
        "scopes": SCOPES,
        "grantTypes": ["authorization_code", "refresh_token"],
        "redirectUris": [redirect_uri],
        "issuerUrl": start_url,
    });
    let reg: RegisterClientResponse =
        json_post(client, &format!("{base}/client/register"), &reg_body).await?;
    let client_id = reg
        .client_id
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ChannelError::Build("RegisterClient missing clientId".into()))?;
    let client_secret = reg
        .client_secret
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ChannelError::Build("RegisterClient missing clientSecret".into()))?;

    let authorize_url = authorize_url(&base, &client_id, redirect_uri, state, challenge);

    Ok(AuthCodeStart {
        authorize_url,
        redirect_uri: redirect_uri.to_string(),
        extra: Some(json!({
            "client_id": client_id,
            "client_secret": client_secret,
            "region": region,
            "start_url": start_url,
        })),
    })
}

/// Build the SSO-OIDC `/authorize` URL. NOTE the scope param is `scopes` (plural,
/// space-joined) — the `aws-sdk-ssooidc` / kiro-cli convention, NOT RFC `scope`.
fn authorize_url(
    base: &str,
    client_id: &str,
    redirect_uri: &str,
    state: &str,
    challenge: &str,
) -> String {
    let scopes = SCOPES.join(" ");
    let query = [
        ("response_type", "code"),
        ("client_id", client_id),
        ("redirect_uri", redirect_uri),
        ("scopes", scopes.as_str()),
        ("state", state),
        ("code_challenge", challenge),
        ("code_challenge_method", "S256"),
    ];
    let mut out = String::new();
    for (k, v) in query {
        out.push(if out.is_empty() { '?' } else { '&' });
        out.push_str(&oauth::percent_encode(k));
        out.push('=');
        out.push_str(&oauth::percent_encode(v));
    }
    format!("{base}/authorize{out}")
}

/// Exchange the authcode (+PKCE verifier) for tokens via `POST /token`
/// (grant=`authorization_code`). `extra` carries the registered client creds +
/// region + start_url; the minted secret carries them for [`refresh`].
pub(super) async fn authcode_exchange(
    client: &Arc<dyn UpstreamClient>,
    code: &str,
    verifier: &str,
    redirect_uri: &str,
    extra: &Value,
) -> Result<Value, ChannelError> {
    let client_id = secret_str(extra, "client_id")
        .ok_or_else(|| ChannelError::Build("login session missing client_id".into()))?;
    let client_secret = secret_str(extra, "client_secret")
        .ok_or_else(|| ChannelError::Build("login session missing client_secret".into()))?;
    let region = secret_str(extra, "region").unwrap_or(BUILDER_ID_REGION);
    let start_url = secret_str(extra, "start_url").unwrap_or(BUILDER_ID_START_URL);

    let body = json!({
        "grantType": "authorization_code",
        "clientId": client_id,
        "clientSecret": client_secret,
        "code": code,
        "redirectUri": redirect_uri,
        "codeVerifier": verifier,
    });
    let resp: CreateTokenResponse =
        json_post(client, &format!("{}/token", oidc_base(region)), &body).await?;

    let access_token = resp
        .access_token
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ChannelError::Build("CreateToken missing accessToken".into()))?;
    let refresh_token = resp
        .refresh_token
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ChannelError::Build("CreateToken missing refreshToken".into()))?;
    let expires_at_ms = now_ms() + resp.expires_in.unwrap_or(3600) as i64 * 1000;

    Ok(json!({
        "access_token": access_token,
        "refresh_token": refresh_token,
        "expires_at_ms": expires_at_ms,
        "client_id": client_id,
        "client_secret": client_secret,
        "region": region,
        "start_url": start_url,
    }))
}

/// Refresh via `POST /token` grant=`refresh_token` at `oidc.{region}.amazonaws.com`
/// using the stored registered-client creds. Serves builderId AND idc — the
/// `region` stored in the secret already targets the right host.
pub(super) async fn refresh(
    client: &Arc<dyn UpstreamClient>,
    secret: &Value,
) -> Result<Value, ChannelError> {
    let refresh_token = secret_str(secret, "refresh_token")
        .ok_or_else(|| ChannelError::InvalidCredential("missing refresh_token".into()))?;
    let region = secret_str(secret, "region").unwrap_or(BUILDER_ID_REGION);
    let client_id = secret_str(secret, "client_id").unwrap_or_default();
    let client_secret = secret_str(secret, "client_secret").unwrap_or_default();

    let body = json!({
        "clientId": client_id,
        "clientSecret": client_secret,
        "refreshToken": refresh_token,
        "grantType": "refresh_token",
    });
    let resp: CreateTokenResponse =
        json_post(client, &format!("{}/token", oidc_base(region)), &body).await?;

    let new_access = resp
        .access_token
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ChannelError::Build("refresh response missing accessToken".into()))?;
    let expires_at_ms = now_ms() + resp.expires_in.unwrap_or(3600) as i64 * 1000;

    let mut out = secret.clone();
    let obj = out
        .as_object_mut()
        .ok_or_else(|| ChannelError::Build("secret is not an object".into()))?;
    obj.insert("access_token".into(), Value::String(new_access));
    if let Some(rt) = resp.refresh_token.filter(|s| !s.is_empty()) {
        obj.insert("refresh_token".into(), Value::String(rt));
    }
    obj.insert("expires_at_ms".into(), Value::Number(expires_at_ms.into()));
    Ok(out)
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn target_defaults_builder_id_and_overrides_for_idc() {
        // No auth_method / builderId → the Builder ID constants.
        assert_eq!(
            target(&json!({})),
            (
                "https://view.awsapps.com/start".to_string(),
                "us-east-1".to_string()
            )
        );
        // idc → operator start_url + region.
        assert_eq!(
            target(&json!({
                "auth_method": "idc",
                "start_url": "https://acme.awsapps.com/start",
                "region": "eu-west-1",
            })),
            (
                "https://acme.awsapps.com/start".to_string(),
                "eu-west-1".to_string()
            )
        );
    }

    #[test]
    fn authorize_url_uses_plural_scopes_and_s256() {
        let url = authorize_url(
            "https://oidc.us-east-1.amazonaws.com",
            "cid-1",
            "http://127.0.0.1:1455/oauth/callback",
            "st-9",
            "chal-x",
        );
        assert!(url.starts_with("https://oidc.us-east-1.amazonaws.com/authorize?"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("client_id=cid-1"));
        // plural `scopes`, space-joined → %20.
        assert!(url.contains("scopes=codewhisperer%3Acompletions%20codewhisperer%3Aanalysis"));
        assert!(url.contains("code_challenge=chal-x"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("state=st-9"));
        // redirect_uri percent-encoded (`:` `/` encoded).
        assert!(url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A1455%2Foauth%2Fcallback"));
    }
}
