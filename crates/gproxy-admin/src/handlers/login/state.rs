use std::time::Duration;

use base64::Engine as _;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{AdminError, State};

const AUTHCODE_TTL: Duration = Duration::from_secs(10 * 60);
const DEVICE_TTL: Duration = Duration::from_secs(15 * 60);

#[derive(Serialize, Deserialize)]
pub(super) struct AuthCodeSession {
    pub channel: String,
    pub provider_id: i64,
    pub verifier: String,
    pub flow_state: String,
    pub redirect_uri: String,
    pub extra: Option<Value>,
}

#[derive(Serialize, Deserialize)]
pub(super) struct DeviceSession {
    pub channel: String,
    pub provider_id: i64,
    pub label: Option<String>,
    pub device_code: String,
}

pub(super) fn pkce() -> Result<(String, String), AdminError> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|_| AdminError::Internal("secure randomness unavailable".into()))?;
    let verifier = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    let challenge = pkce_challenge(&verifier);
    Ok((verifier, challenge))
}

pub(crate) fn pkce_challenge(verifier: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

pub(super) fn session_id() -> Result<String, AdminError> {
    let mut bytes = [0_u8; 24];
    getrandom::fill(&mut bytes)
        .map_err(|_| AdminError::Internal("secure randomness unavailable".into()))?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}

pub(super) async fn store_authcode(
    state: &impl State,
    id: &str,
    session: &AuthCodeSession,
) -> Result<(), AdminError> {
    store(state, id, session, AUTHCODE_TTL).await
}

pub(super) async fn store_device(
    state: &impl State,
    id: &str,
    session: &DeviceSession,
) -> Result<(), AdminError> {
    store(state, id, session, DEVICE_TTL).await
}

pub(super) async fn authcode(state: &impl State, id: &str) -> Result<AuthCodeSession, AdminError> {
    load(state, id).await
}

pub(super) async fn device(state: &impl State, id: &str) -> Result<DeviceSession, AdminError> {
    load(state, id).await
}

pub(super) async fn delete(state: &impl State, id: &str) -> Result<(), AdminError> {
    state.login_state_delete(&key(id)).await
}

async fn store(
    state: &impl State,
    id: &str,
    session: &impl Serialize,
    ttl: Duration,
) -> Result<(), AdminError> {
    let value = serde_json::to_vec(session)
        .map_err(|_| AdminError::Internal("login state encoding failed".into()))?;
    state.login_state_set(&key(id), value, ttl).await
}

async fn load<T: DeserializeOwned>(state: &impl State, id: &str) -> Result<T, AdminError> {
    let value = state.login_state_get(&key(id)).await?.ok_or_else(expired)?;
    serde_json::from_slice(&value).map_err(|_| expired())
}

fn key(id: &str) -> String {
    format!("gproxy:login:{id}")
}

fn expired() -> AdminError {
    AdminError::BadRequest("login session is missing or expired".into())
}
