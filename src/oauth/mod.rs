use std::collections::HashMap;
use std::sync::OnceLock;

use axum::http::StatusCode;
use base64::Engine;
use rand::RngCore;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use url::Url;

pub const DEFAULT_TTL_SECS: i64 = 15 * 60;

#[derive(Debug, Deserialize)]
pub struct CallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
    pub callback_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PendingEntry<T> {
    pub code_verifier: String,
    pub redirect_uri: String,
    pub created_at: i64,
    pub data: T,
}

impl<T> PendingEntry<T> {
    pub fn new(redirect_uri: String, code_verifier: String, created_at: i64, data: T) -> Self {
        Self {
            code_verifier,
            redirect_uri,
            created_at,
            data,
        }
    }
}

pub struct PendingStore<T> {
    inner: OnceLock<Mutex<HashMap<String, PendingEntry<T>>>>,
}

impl<T> PendingStore<T> {
    pub const fn new() -> Self {
        Self {
            inner: OnceLock::new(),
        }
    }

    fn map(&self) -> &Mutex<HashMap<String, PendingEntry<T>>> {
        self.inner.get_or_init(|| Mutex::new(HashMap::new()))
    }

    pub async fn insert(&self, state: String, entry: PendingEntry<T>, now: i64, ttl_secs: i64) {
        let mut pending = self.map().lock().await;
        pending.retain(|_, item| now.saturating_sub(item.created_at) <= ttl_secs);
        pending.insert(state, entry);
    }

    pub async fn take(&self, state: &str, now: i64, ttl_secs: i64) -> Option<PendingEntry<T>> {
        let mut pending = self.map().lock().await;
        pending.retain(|_, item| now.saturating_sub(item.created_at) <= ttl_secs);
        pending.remove(state)
    }
}

impl<T> Default for PendingStore<T> {
    fn default() -> Self {
        Self::new()
    }
}

pub fn generate_state() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

pub fn generate_code_verifier() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

pub fn pkce_challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}

pub fn extract_callback_params(query: &CallbackQuery) -> Result<(String, String), StatusCode> {
    let mut code = query.code.clone();
    let mut state = query.state.clone();

    if (code.is_none() || state.is_none()) && query.callback_url.is_some() {
        let (parsed_code, parsed_state) =
            parse_callback_url(query.callback_url.as_deref().unwrap_or_default())?;
        if code.is_none() {
            code = Some(parsed_code);
        }
        if state.is_none() {
            state = Some(parsed_state);
        }
    }

    let Some(code) = code else {
        return Err(StatusCode::BAD_REQUEST);
    };
    let Some(state) = state else {
        return Err(StatusCode::BAD_REQUEST);
    };
    Ok((code, state))
}

pub fn parse_callback_url(input: &str) -> Result<(String, String), StatusCode> {
    let url = Url::parse(input).map_err(|_| StatusCode::BAD_REQUEST)?;
    let mut code = None;
    let mut state = None;
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "code" => code = Some(value.to_string()),
            "state" => state = Some(value.to_string()),
            _ => {}
        }
    }
    let Some(code) = code else {
        return Err(StatusCode::BAD_REQUEST);
    };
    let Some(state) = state else {
        return Err(StatusCode::BAD_REQUEST);
    };
    Ok((code, state))
}
