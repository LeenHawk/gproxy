use std::collections::HashMap;
use std::sync::OnceLock;

use base64::Engine;
use http::header::CONTENT_TYPE;
use http::{HeaderMap, HeaderValue, StatusCode};
use rand::RngCore;
use serde_json::Value as JsonValue;
use sha2::Digest;
use time::OffsetDateTime;
use tracing::warn;

use gproxy_provider_core::{AttemptFailure, DisallowScope, UpstreamContext, UpstreamPassthroughError};

use crate::client::shared_client;
use crate::credential::BaseCredential;

use super::{
    channel_urls, credential_access_token, credential_expires_at, credential_refresh_token,
    credential_session_key, invalid_credential, ClaudeCodeUrls, CLIENT_ID, COOKIE_UA,
    OAUTH_SCOPE_SESSION, TOKEN_UA,
};
use super::oauth::{exchange_code_for_tokens, enrich_with_profile, persist_claudecode_credential, OAuthTokens};

#[derive(Clone, Debug)]
pub(super) struct CachedTokens {
    pub(super) access_token: String,
    pub(super) refresh_token: Option<String>,
    pub(super) expires_at: Option<i64>,
}

static TOKEN_CACHE: OnceLock<tokio::sync::RwLock<HashMap<i64, CachedTokens>>> = OnceLock::new();

pub(super) async fn ensure_tokens(
    pool: &gproxy_provider_core::CredentialPool<BaseCredential>,
    credential: &BaseCredential,
    ctx: &UpstreamContext,
    scope: &DisallowScope,
) -> Result<CachedTokens, AttemptFailure> {
    if let Some(cached) = token_cache().read().await.get(&credential.id).cloned()
        && !is_expired(cached.expires_at) {
            return Ok(cached);
        }

    if let Some(access_token) = credential_access_token(credential) {
        let expires_at = credential_expires_at(credential);
        if !is_expired(expires_at) {
            let tokens = CachedTokens {
                access_token,
                refresh_token: credential_refresh_token(credential),
                expires_at,
            };
            token_cache().write().await.insert(credential.id, tokens.clone());
            return Ok(tokens);
        }
    }

    let urls = channel_urls(ctx)
        .await
        .map_err(|err| AttemptFailure { passthrough: err, mark: None })?;

    if let Some(refresh_token) = credential_refresh_token(credential)
        && let Ok(tokens) =
            refresh_access_token(credential.id, refresh_token, ctx, scope, &urls).await
        {
            return Ok(tokens);
        }

    if let Some(session_key) = credential_session_key(credential) {
        let mut tokens = oauth_with_session_key(&session_key, ctx, &urls).await?;
        tokens.session_key = Some(session_key);
        let existing_id = Some(credential.id);
        persist_claudecode_credential(pool, ctx.provider_id, existing_id, tokens.clone())
            .await
            .map_err(|err| AttemptFailure {
                passthrough: err,
                mark: None,
            })?;
        let cached = CachedTokens {
            access_token: tokens.access_token,
            refresh_token: tokens.refresh_token,
            expires_at: tokens.expires_at,
        };
        token_cache().write().await.insert(credential.id, cached.clone());
        return Ok(cached);
    }

    Err(invalid_credential(scope, "missing claude oauth tokens"))
}

pub(super) async fn refresh_access_token(
    credential_id: i64,
    refresh_token: String,
    ctx: &UpstreamContext,
    scope: &DisallowScope,
    urls: &ClaudeCodeUrls,
) -> Result<CachedTokens, AttemptFailure> {
    let client = shared_client(ctx.proxy.as_deref())?;
    let payload = serde_json::json!({
        "grant_type": "refresh_token",
        "client_id": CLIENT_ID,
        "refresh_token": refresh_token,
    });
    let origin = urls.claude_ai_base.trim_end_matches('/');
    let token_url = format!("{}/v1/oauth/token", urls.api_base.trim_end_matches('/'));
    let response = client
        .post(token_url)
        .header(CONTENT_TYPE, HeaderValue::from_static("application/json"))
        .header(http::header::USER_AGENT, HeaderValue::from_static(TOKEN_UA))
        .header("accept", HeaderValue::from_static("application/json, text/plain, */*"))
        .header(
            "accept-language",
            HeaderValue::from_static("en-US,en;q=0.9"),
        )
        .header(
            "origin",
            HeaderValue::from_str(origin).unwrap_or_else(|_| HeaderValue::from_static("https://claude.ai")),
        )
        .header(
            "referer",
            HeaderValue::from_str(&format!("{origin}/"))
                .unwrap_or_else(|_| HeaderValue::from_static("https://claude.ai/")),
        )
        .json(&payload)
        .send()
        .await
        .map_err(|err| AttemptFailure {
            passthrough: UpstreamPassthroughError::service_unavailable(err.to_string()),
            mark: None,
        })?;
    let status = response.status();
    let headers = response.headers().clone();
    let body = response
        .bytes()
        .await
        .map_err(|err| AttemptFailure {
            passthrough: UpstreamPassthroughError::service_unavailable(err.to_string()),
            mark: None,
        })?;
    if !status.is_success() {
        let mark = if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            Some(gproxy_provider_core::DisallowMark {
                scope: scope.clone(),
                level: gproxy_provider_core::DisallowLevel::Dead,
                duration: None,
                reason: Some("refresh_token_invalid".to_string()),
            })
        } else {
            None
        };
        return Err(AttemptFailure {
            passthrough: UpstreamPassthroughError::new(status, headers, body),
            mark,
        });
    }

    let raw = serde_json::from_slice::<serde_json::Value>(&body).map_err(|err| AttemptFailure {
        passthrough: UpstreamPassthroughError::service_unavailable(err.to_string()),
        mark: None,
    })?;
    let access_token = raw
        .get("access_token")
        .and_then(|value| value.as_str())
        .ok_or_else(|| AttemptFailure {
            passthrough: UpstreamPassthroughError::service_unavailable(
                "refresh_token response missing access_token".to_string(),
            ),
            mark: None,
        })?
        .to_string();
    let refresh_token = raw
        .get("refresh_token")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());
    let expires_in = raw
        .get("expires_in")
        .and_then(|value| value.as_i64());
    let expires_at = expires_in.map(|seconds| (OffsetDateTime::now_utc().unix_timestamp() + seconds) * 1000);
    let tokens = CachedTokens {
        access_token,
        refresh_token,
        expires_at,
    };
    token_cache().write().await.insert(credential_id, tokens.clone());
    Ok(tokens)
}

fn token_cache() -> &'static tokio::sync::RwLock<HashMap<i64, CachedTokens>> {
    TOKEN_CACHE.get_or_init(|| tokio::sync::RwLock::new(HashMap::new()))
}

fn is_expired(expires_at: Option<i64>) -> bool {
    let Some(expires_at) = expires_at else {
        return false;
    };
    let now_ms = OffsetDateTime::now_utc().unix_timestamp() * 1000;
    now_ms >= expires_at.saturating_sub(60_000)
}

pub(super) async fn oauth_with_session_key(
    session_key: &str,
    ctx: &UpstreamContext,
    urls: &ClaudeCodeUrls,
) -> Result<OAuthTokens, AttemptFailure> {
    let org = get_organization_info(session_key, ctx, urls).await?;
    let (code, verifier, scope, state) =
        authorize_with_cookie(session_key, &org, ctx, urls).await?;
    let mut tokens = exchange_code_for_tokens(
        &format!("{}/oauth/code/callback", urls.console_base),
        &verifier,
        &code,
        &scope,
        Some(&state),
        ctx.proxy.as_deref(),
        &urls.api_base,
        &urls.claude_ai_base,
    )
    .await
    .map_err(|err| AttemptFailure {
        passthrough: err,
        mark: None,
    })?;
    enrich_with_profile(&mut tokens, ctx.proxy.as_deref(), &urls.api_base)
        .await
        .ok();
    Ok(tokens)
}

pub(super) async fn cache_tokens(credential_id: i64, tokens: CachedTokens) {
    token_cache().write().await.insert(credential_id, tokens);
}

struct OrgInfo {
    uuid: String,
    capabilities: Vec<String>,
}

async fn get_organization_info(
    session_key: &str,
    ctx: &UpstreamContext,
    urls: &ClaudeCodeUrls,
) -> Result<OrgInfo, AttemptFailure> {
    let client = shared_client(ctx.proxy.as_deref())?;
    let headers = build_cookie_headers(session_key, urls)?;
    let url = format!("{}/api/organizations", urls.claude_ai_base);
    let response = client
        .get(url)
        .headers(headers)
        .send()
        .await
        .map_err(|err| AttemptFailure {
            passthrough: UpstreamPassthroughError::service_unavailable(err.to_string()),
            mark: None,
        })?;
    let status = response.status();
    let body = response
        .bytes()
        .await
        .map_err(|err| AttemptFailure {
            passthrough: UpstreamPassthroughError::service_unavailable(err.to_string()),
            mark: None,
        })?;
    if !status.is_success() {
        warn!(
            event = "claudecode.sessionkey_org",
            status = %status.as_u16(),
            body = %String::from_utf8_lossy(&body)
        );
        return Err(AttemptFailure {
            passthrough: UpstreamPassthroughError::service_unavailable(format!(
                "sessionKey org lookup failed: {status}"
            )),
            mark: None,
        });
    }
    let payload = serde_json::from_slice::<JsonValue>(&body).map_err(|err| AttemptFailure {
        passthrough: UpstreamPassthroughError::service_unavailable(err.to_string()),
        mark: None,
    })?;
    let list = payload.as_array().ok_or_else(|| AttemptFailure {
        passthrough: UpstreamPassthroughError::service_unavailable(
            "invalid org list response".to_string(),
        ),
        mark: None,
    })?;
    let mut best: Option<OrgInfo> = None;
    for org in list {
        let capabilities = org
            .get("capabilities")
            .and_then(|value| value.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(|v| v.to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if !capabilities.iter().any(|value| value == "chat") {
            continue;
        }
        let uuid = org
            .get("uuid")
            .or_else(|| org.get("id"))
            .and_then(|value| value.as_str())
            .map(|value| value.to_string());
        let Some(uuid) = uuid else {
            continue;
        };
        let candidate = OrgInfo { uuid, capabilities };
        let replace = match &best {
            Some(existing) => candidate.capabilities.len() > existing.capabilities.len(),
            None => true,
        };
        if replace {
            best = Some(candidate);
        }
    }
    best.ok_or_else(|| AttemptFailure {
        passthrough: UpstreamPassthroughError::service_unavailable(
            "no organization with chat capability".to_string(),
        ),
        mark: None,
    })
}

async fn authorize_with_cookie(
    session_key: &str,
    org: &OrgInfo,
    ctx: &UpstreamContext,
    urls: &ClaudeCodeUrls,
) -> Result<(String, String, String, String), AttemptFailure> {
    let code_verifier = generate_code_verifier();
    let code_challenge = generate_code_challenge(&code_verifier);
    let state = generate_state();
    let scope = OAUTH_SCOPE_SESSION;
    let payload = serde_json::json!({
        "response_type": "code",
        "client_id": CLIENT_ID,
        "organization_uuid": org.uuid,
        "redirect_uri": format!("{}/oauth/code/callback", urls.console_base),
        "scope": scope,
        "state": state,
        "code_challenge": code_challenge,
        "code_challenge_method": "S256",
    });
    let url = format!("{}/v1/oauth/{}/authorize", urls.api_base, org.uuid);
    let mut headers = build_cookie_headers(session_key, urls)?;
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    let client = shared_client(ctx.proxy.as_deref())?;
    let response = client
        .post(&url)
        .headers(headers)
        .json(&payload)
        .send()
        .await
        .map_err(|err| AttemptFailure {
            passthrough: UpstreamPassthroughError::service_unavailable(err.to_string()),
            mark: None,
        })?;
    let status = response.status();
    let body = response
        .bytes()
        .await
        .map_err(|err| AttemptFailure {
            passthrough: UpstreamPassthroughError::service_unavailable(err.to_string()),
            mark: None,
        })?;
    if !status.is_success() {
        warn!(
            event = "claudecode.sessionkey_authorize",
            status = %status.as_u16(),
            url = %url,
            redirect_uri = %format!("{}/oauth/code/callback", urls.console_base),
            scope = %scope,
            body = %String::from_utf8_lossy(&body)
        );
        return Err(AttemptFailure {
            passthrough: UpstreamPassthroughError::service_unavailable(format!(
                "sessionKey authorize failed: {status}"
            )),
            mark: None,
        });
    }
    let payload = serde_json::from_slice::<JsonValue>(&body).map_err(|err| AttemptFailure {
        passthrough: UpstreamPassthroughError::service_unavailable(err.to_string()),
        mark: None,
    })?;
    let redirect_uri = payload
        .get("redirect_uri")
        .and_then(|value| value.as_str())
        .ok_or_else(|| AttemptFailure {
            passthrough: UpstreamPassthroughError::service_unavailable(
                "missing redirect_uri".to_string(),
            ),
            mark: None,
        })?;
    let code = extract_query_value(redirect_uri, "code").ok_or_else(|| AttemptFailure {
        passthrough: UpstreamPassthroughError::service_unavailable(
            "missing code in redirect_uri".to_string(),
        ),
        mark: None,
    })?;
    Ok((code, code_verifier, scope.to_string(), state))
}

#[allow(clippy::result_large_err)]
fn build_cookie_headers(
    session_key: &str,
    urls: &ClaudeCodeUrls,
) -> Result<HeaderMap, AttemptFailure> {
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::ACCEPT,
        HeaderValue::from_static("application/json"),
    );
    headers.insert(
        http::header::ACCEPT_LANGUAGE,
        HeaderValue::from_static("en-US,en;q=0.9"),
    );
    headers.insert(
        http::header::CACHE_CONTROL,
        HeaderValue::from_static("no-cache"),
    );
    headers.insert(
        http::header::COOKIE,
        HeaderValue::from_str(&format!("sessionKey={session_key}")).map_err(|err| AttemptFailure {
            passthrough: UpstreamPassthroughError::service_unavailable(err.to_string()),
            mark: None,
        })?,
    );
    let origin = urls.claude_ai_base.trim_end_matches('/');
    let referer = format!("{origin}/new");
    headers.insert(
        http::header::ORIGIN,
        HeaderValue::from_str(origin).map_err(|err| AttemptFailure {
            passthrough: UpstreamPassthroughError::service_unavailable(err.to_string()),
            mark: None,
        })?,
    );
    headers.insert(
        http::header::REFERER,
        HeaderValue::from_str(&referer).map_err(|err| AttemptFailure {
            passthrough: UpstreamPassthroughError::service_unavailable(err.to_string()),
            mark: None,
        })?,
    );
    headers.insert(http::header::USER_AGENT, HeaderValue::from_static(COOKIE_UA));
    Ok(headers)
}

fn generate_state() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn generate_code_verifier() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn generate_code_challenge(verifier: &str) -> String {
    let mut hasher = sha2::Sha256::new();
    hasher.update(verifier.as_bytes());
    let digest = hasher.finalize();
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}

fn extract_query_value(url: &str, key: &str) -> Option<String> {
    let query = url.split('?').nth(1)?;
    for pair in query.split('&') {
        let mut iter = pair.splitn(2, '=');
        let name = iter.next()?;
        let value = iter.next().unwrap_or("");
        if name == key {
            return urlencoding::decode(value).ok().map(|v| v.to_string());
        }
    }
    None
}
