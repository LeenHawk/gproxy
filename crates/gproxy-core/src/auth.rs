use std::collections::HashMap;
use std::sync::Arc;

use arc_swap::ArcSwap;
use bytes::Bytes;
use http::{HeaderMap, StatusCode};

#[derive(Debug, Clone, Default)]
pub struct AuthContext {
    pub user_id: Option<String>,
    pub key_id: Option<String>,
}

#[derive(Debug)]
pub struct AuthError {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Bytes,
}

impl AuthError {
    pub fn new(status: StatusCode, body: impl Into<Bytes>) -> Self {
        Self {
            status,
            headers: HeaderMap::new(),
            body: body.into(),
        }
    }
}

pub trait AuthProvider: Send + Sync {
    #[allow(clippy::result_large_err)]
    fn authenticate(&self, headers: &HeaderMap) -> Result<AuthContext, AuthError>;
}

#[derive(Debug, Default)]
pub struct NoopAuth;

impl AuthProvider for NoopAuth {
    fn authenticate(&self, _headers: &HeaderMap) -> Result<AuthContext, AuthError> {
        Ok(AuthContext::default())
    }
}

#[derive(Debug, Clone)]
pub struct AuthKeyEntry {
    pub key_id: i64,
    pub user_id: i64,
    pub enabled: bool,
}

#[derive(Debug, Clone)]
pub struct UserEntry {
    pub id: i64,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct AuthSnapshot {
    pub keys_by_value: HashMap<String, AuthKeyEntry>,
    pub users_by_id: HashMap<i64, UserEntry>,
}

#[derive(Debug)]
pub struct MemoryAuth {
    snapshot: ArcSwap<AuthSnapshot>,
}

impl MemoryAuth {
    pub fn new(snapshot: AuthSnapshot) -> Self {
        Self {
            snapshot: ArcSwap::from_pointee(snapshot),
        }
    }

    pub fn replace_snapshot(&self, snapshot: AuthSnapshot) {
        self.snapshot.store(Arc::new(snapshot));
    }
}

impl AuthProvider for MemoryAuth {
    fn authenticate(&self, headers: &HeaderMap) -> Result<AuthContext, AuthError> {
        let api_key = extract_api_key(headers)
            .ok_or_else(|| AuthError::new(StatusCode::UNAUTHORIZED, "missing api key"))?;

        let snapshot = self.snapshot.load();
        let entry = snapshot
            .keys_by_value
            .get(api_key.as_str())
            .ok_or_else(|| AuthError::new(StatusCode::FORBIDDEN, "invalid api key"))?;

        if !entry.enabled {
            return Err(AuthError::new(
                StatusCode::FORBIDDEN,
                "api key disabled",
            ));
        }

        Ok(AuthContext {
            user_id: Some(entry.user_id.to_string()),
            key_id: Some(entry.key_id.to_string()),
        })
    }
}

fn extract_api_key(headers: &HeaderMap) -> Option<String> {
    if let Some(value) = header_value(headers, "x-api-key") {
        return Some(value);
    }

    let auth = header_value(headers, "authorization")?;
    let auth = auth.trim();
    if let Some(token) = auth.strip_prefix("Bearer ") {
        return Some(token.trim().to_string());
    }
    if let Some(token) = auth.strip_prefix("bearer ") {
        return Some(token.trim().to_string());
    }
    None
}

fn header_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string())
}
