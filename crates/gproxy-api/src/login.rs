use std::sync::Arc;

use argon2::password_hash::SaltString;
use argon2::{Algorithm, Argon2, Params, PasswordHash, PasswordHasher, PasswordVerifier, Version};
use axum::Json;
use axum::extract::State;
use serde::{Deserialize, Serialize};

use gproxy_server::AppState;
use gproxy_storage::Scope;

use crate::error::HttpError;

/// Configured Argon2id instance with explicit OWASP-recommended parameters.
fn argon2_instance() -> Argon2<'static> {
    let params = Params::new(19 * 1024, 2, 1, None).expect("valid argon2 params");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

/// Hash a password with Argon2id and a random salt.
pub fn hash_password(password: &str) -> String {
    let salt = SaltString::generate(&mut argon2::password_hash::rand_core::OsRng);
    argon2_instance()
        .hash_password(password.as_bytes(), &salt)
        .expect("argon2 hash")
        .to_string()
}

fn is_argon2_password_hash(password_or_hash: &str) -> bool {
    PasswordHash::new(password_or_hash)
        .ok()
        .is_some_and(|hash| hash.algorithm.as_str().starts_with("argon2"))
}

/// Accept either a plaintext password or an existing Argon2 PHC hash.
pub fn normalize_password_for_storage(password_or_hash: &str) -> String {
    if is_argon2_password_hash(password_or_hash) {
        password_or_hash.to_string()
    } else {
        hash_password(password_or_hash)
    }
}

/// Verify a password against a stored Argon2 PHC hash.
pub fn verify_password(password: &str, stored_hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(stored_hash) else {
        return false;
    };
    argon2_instance()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub user_id: i64,
    pub api_key: String,
}

pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, HttpError> {
    let storage = state.storage();
    let users = storage
        .list_users(&gproxy_storage::UserQuery {
            name: Scope::Eq(payload.username.clone()),
            ..Default::default()
        })
        .await?;

    let user = users
        .first()
        .ok_or_else(|| HttpError::unauthorized("invalid username or password"))?;

    if !verify_password(&payload.password, &user.password) {
        return Err(HttpError::unauthorized("invalid username or password"));
    }

    if !user.enabled {
        return Err(HttpError::forbidden("user is disabled"));
    }

    let keys = storage
        .list_user_keys(&gproxy_storage::UserKeyQuery {
            user_id: Scope::Eq(user.id),
            enabled: Scope::Eq(true),
            ..Default::default()
        })
        .await?;

    let key = keys
        .first()
        .ok_or_else(|| HttpError::internal("user has no API key"))?;

    Ok(Json(LoginResponse {
        user_id: user.id,
        api_key: key.api_key.clone(),
    }))
}

#[cfg(test)]
mod tests {
    use super::{hash_password, normalize_password_for_storage, verify_password};

    #[test]
    fn normalize_password_hashes_plaintext() {
        let stored = normalize_password_for_storage("secret-password");
        assert_ne!(stored, "secret-password");
        assert!(verify_password("secret-password", &stored));
    }

    #[test]
    fn normalize_password_preserves_argon2_hashes() {
        let hash = hash_password("secret-password");
        assert_eq!(normalize_password_for_storage(&hash), hash);
    }
}
