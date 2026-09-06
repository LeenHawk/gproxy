use web_time::{SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use gproxy_store::records::{AdminUserRecord, UserSessionInput};
use http::request::Parts;
use sha2::{Digest, Sha256};

use crate::{AdminError, State};

const COOKIE_NAME: &str = "gproxy_admin_session";
const SESSION_SECONDS: i64 = 12 * 60 * 60;

#[derive(Debug, Clone)]
pub(crate) struct AdminIdentity {
    pub id: i64,
    pub username: String,
    pub api_key: bool,
}

pub(crate) async fn create(state: &impl State, user_id: i64) -> Result<String, AdminError> {
    let mut raw = [0_u8; 32];
    getrandom::fill(&mut raw)
        .map_err(|_| AdminError::Internal("secure randomness unavailable".into()))?;
    let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw);
    let created_at = now()?;
    state
        .store()
        .create_user_session(&UserSessionInput {
            token_digest: digest(&token),
            user_id,
            created_at,
            expires_at: created_at.saturating_add(SESSION_SECONDS),
        })
        .await?;
    Ok(token)
}

pub(crate) async fn authenticate(
    state: &impl State,
    parts: &Parts,
) -> Result<AdminIdentity, AdminError> {
    if let Some(token) = bearer(parts) {
        // API keys are stored under the same digest ingress uses, which strips the
        // presentation prefix; hashing the raw token would never match an `sk-` key.
        let (_, key_digest) = state.digest_user_key(token);
        let account = state
            .store()
            .admin_for_api_key(&key_digest, now()?)
            .await?
            .ok_or(AdminError::Unauthorized)?;
        return Ok(identity(account, true));
    }
    let token = token(parts).ok_or(AdminError::Unauthorized)?;
    let account = state
        .store()
        .admin_for_session(&digest(token), now()?)
        .await?
        .ok_or(AdminError::Unauthorized)?;
    Ok(identity(account, false))
}

pub(super) async fn revoke(state: &impl State, parts: &Parts) -> Result<(), AdminError> {
    if let Some(token) = token(parts) {
        state.store().delete_user_session(&digest(token)).await?;
    }
    Ok(())
}

pub(super) fn set_cookie(token: &str) -> String {
    format!(
        "{COOKIE_NAME}={token}; HttpOnly; SameSite=Strict; Path=/admin; Max-Age={SESSION_SECONDS}"
    )
}

pub(super) fn clear_cookie() -> &'static str {
    "gproxy_admin_session=; HttpOnly; SameSite=Strict; Path=/admin; Max-Age=0"
}

pub(crate) fn now() -> Result<i64, AdminError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| AdminError::Internal("system clock is before Unix epoch".into()))?
        .as_secs()
        .try_into()
        .map_err(|_| AdminError::Internal("Unix time exceeds i64".into()))
}

pub(crate) fn cookie_token<'a>(parts: &'a Parts, name: &str) -> Option<&'a str> {
    parts
        .headers
        .get(http::header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .map(str::trim)
        .find_map(|part| part.strip_prefix(&format!("{name}=")))
        .filter(|value| !value.is_empty())
}

fn token(parts: &Parts) -> Option<&str> {
    cookie_token(parts, COOKIE_NAME)
}

pub(crate) fn digest(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

fn bearer(parts: &Parts) -> Option<&str> {
    parts
        .headers
        .get(http::header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .filter(|value| !value.is_empty())
}

fn identity(account: AdminUserRecord, api_key: bool) -> AdminIdentity {
    AdminIdentity {
        id: account.id,
        username: account.name,
        api_key,
    }
}
