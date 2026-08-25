use web_time::{SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use gproxy_store::records::{AdminAccountRecord, AdminSessionInput};
use http::request::Parts;
use sha2::{Digest, Sha256};

use crate::{AdminError, State};

const COOKIE_NAME: &str = "gproxy_admin_session";
const SESSION_SECONDS: i64 = 12 * 60 * 60;

#[derive(Debug, Clone)]
pub(crate) struct AdminIdentity {
    pub id: i64,
    pub username: String,
}

pub(super) async fn create(state: &impl State, admin_id: i64) -> Result<String, AdminError> {
    let mut raw = [0_u8; 32];
    getrandom::fill(&mut raw)
        .map_err(|_| AdminError::Internal("secure randomness unavailable".into()))?;
    let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw);
    let created_at = now()?;
    state
        .store()
        .create_admin_session(&AdminSessionInput {
            token_digest: digest(&token),
            admin_id,
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
    let token = token(parts).ok_or(AdminError::Unauthorized)?;
    let account = state
        .store()
        .admin_for_session(&digest(token), now()?)
        .await?
        .ok_or(AdminError::Unauthorized)?;
    Ok(identity(account))
}

pub(super) async fn revoke(state: &impl State, parts: &Parts) -> Result<(), AdminError> {
    if let Some(token) = token(parts) {
        state.store().delete_admin_session(&digest(token)).await?;
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

fn token(parts: &Parts) -> Option<&str> {
    parts
        .headers
        .get(http::header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .map(str::trim)
        .find_map(|part| part.strip_prefix(&format!("{COOKIE_NAME}=")))
        .filter(|value| !value.is_empty())
}

fn digest(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

fn identity(account: AdminAccountRecord) -> AdminIdentity {
    AdminIdentity {
        id: account.id,
        username: account.username,
    }
}
