use base64::Engine as _;
use gproxy_channel_api::{CODEX_OAUTH_CLIENT_ID, OAuthError, OAuthTokenSet};
use gproxy_store::records::{OAuthExchangeSource, OAuthGrantRecord, OAuthTokenInput};

use super::super::AppHost;
use super::util::{digest, now, pkce, random_url, store};
use super::{ACCESS_SECONDS, REFRESH_SECONDS};

pub(super) async fn exchange_code(
    host: &AppHost,
    code: &str,
    client_id: &str,
    redirect_uri: &str,
    verifier: &str,
    issuer: &str,
) -> Result<OAuthTokenSet, OAuthError> {
    let current = now();
    let record = host
        .services
        .store
        .oauth_code(&digest(code))
        .await
        .map_err(store)?
        .ok_or(OAuthError::InvalidGrant)?;
    if record.consumed_at.is_some()
        || record.expires_at <= current
        || record.grant.revoked_at.is_some()
        || record.grant.client_id != client_id
        || record.redirect_uri != redirect_uri
        || !(43..=128).contains(&verifier.len())
        || !verifier
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-._~".contains(&byte))
        || pkce(verifier) != record.code_challenge
    {
        return Err(OAuthError::InvalidGrant);
    }
    issue(
        host,
        &record.grant,
        issuer,
        OAuthExchangeSource::Code(record.id),
        current,
    )
    .await
}

pub(super) async fn refresh(
    host: &AppHost,
    refresh_token: &str,
    client_id: &str,
    issuer: &str,
) -> Result<OAuthTokenSet, OAuthError> {
    let current = now();
    let record = host
        .services
        .store
        .oauth_token(&digest(refresh_token))
        .await
        .map_err(store)?
        .ok_or(OAuthError::InvalidGrant)?;
    if record.kind != "refresh"
        || record.consumed_at.is_some()
        || record.revoked_at.is_some()
        || record.expires_at <= current
        || record.grant.revoked_at.is_some()
        || record.grant.client_id != client_id
    {
        return Err(OAuthError::InvalidGrant);
    }
    issue(
        host,
        &record.grant,
        issuer,
        OAuthExchangeSource::Refresh(record.id),
        current,
    )
    .await
}

async fn issue(
    host: &AppHost,
    grant: &OAuthGrantRecord,
    issuer: &str,
    source: OAuthExchangeSource,
    current: i64,
) -> Result<OAuthTokenSet, OAuthError> {
    let legacy = grant.client_id == CODEX_OAUTH_CLIENT_ID;
    let access = if legacy {
        jwt(host, grant, issuer, current)?
    } else {
        format!("at-gp-access-{}", random_url(32)?)
    };
    let refresh = format!("rt-gp-{}", random_url(48)?);
    let id_token = if legacy {
        jwt(host, grant, issuer, current)?
    } else {
        String::new()
    };
    let token = |value: &str, kind: &str, seconds| OAuthTokenInput {
        digest: digest(value),
        grant_id: grant.id,
        kind: kind.into(),
        created_at: current,
        expires_at: current + seconds,
    };
    if !host
        .services
        .store
        .exchange_oauth_tokens(
            source,
            &grant.client_id,
            &token(&access, "access", ACCESS_SECONDS),
            &token(&refresh, "refresh", REFRESH_SECONDS),
        )
        .await
        .map_err(store)?
    {
        return Err(OAuthError::InvalidGrant);
    }
    Ok(OAuthTokenSet {
        id_token,
        access_token: access,
        refresh_token: refresh,
        expires_in: ACCESS_SECONDS as u64,
    })
}

fn jwt(
    host: &AppHost,
    grant: &OAuthGrantRecord,
    issuer: &str,
    current: i64,
) -> Result<String, OAuthError> {
    let encode = |value: serde_json::Value| {
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(value.to_string())
    };
    let header = encode(serde_json::json!({"alg":"HS256","typ":"JWT"}));
    let payload = encode(serde_json::json!({
        "iss":issuer, "aud":grant.client_id, "sub":grant.chatgpt_user_id,
        "iat":current, "exp":current + ACCESS_SECONDS, "jti":random_url(18)?,
        "https://api.openai.com/auth":{
            "chatgpt_plan_type":"pro", "chatgpt_account_id":grant.chatgpt_account_id,
            "chatgpt_user_id":grant.chatgpt_user_id, "chatgpt_account_is_fedramp":false,
            "completed_platform_onboarding":true, "is_org_owner":false,
        }
    }));
    let input = format!("{header}.{payload}");
    let signature = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(host.services.cipher.sign_oauth(input.as_bytes()));
    Ok(format!("{input}.{signature}"))
}
