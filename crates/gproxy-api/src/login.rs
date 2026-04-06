use std::sync::Arc;

use argon2::password_hash::SaltString;
use argon2::{Algorithm, Argon2, Params, PasswordHash, PasswordHasher, PasswordVerifier, Version};
use axum::Json;
use axum::extract::State;
use serde::{Deserialize, Serialize};

use gproxy_server::AppState;
use gproxy_storage::{Scope, UserQueryRow};

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

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub user_id: i64,
    pub session_token: String,
    pub expires_in_secs: u64,
    pub is_admin: bool,
}

async fn authenticate_password_login(
    state: &AppState,
    payload: &LoginRequest,
) -> Result<UserQueryRow, HttpError> {
    let storage = state.storage();
    let users = storage
        .list_users(&gproxy_storage::UserQuery {
            name: Scope::Eq(payload.username.clone()),
            ..Default::default()
        })
        .await?;

    let user = users
        .first()
        .cloned()
        .ok_or_else(|| HttpError::unauthorized("invalid username or password"))?;

    if !verify_password(&payload.password, &user.password) {
        return Err(HttpError::unauthorized("invalid username or password"));
    }

    if !user.enabled {
        return Err(HttpError::forbidden("user is disabled"));
    }

    Ok(user)
}

pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, HttpError> {
    let user = authenticate_password_login(&state, &payload).await?;
    let ttl_secs = 24 * 60 * 60;
    let session_token = state.create_session(user.id, ttl_secs);

    Ok(Json(LoginResponse {
        user_id: user.id,
        session_token,
        expires_in_secs: ttl_secs,
        is_admin: user.is_admin,
    }))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{
        LoginRequest, hash_password, login, normalize_password_for_storage, verify_password,
    };
    use axum::{Json, extract::State};
    use gproxy_server::{AppStateBuilder, GlobalConfig};
    use gproxy_storage::{SeaOrmStorage, repository::UserRepository};

    async fn build_test_state() -> Arc<gproxy_server::AppState> {
        let storage = Arc::new(
            SeaOrmStorage::connect("sqlite::memory:", None)
                .await
                .expect("in-memory sqlite storage"),
        );
        storage.sync().await.expect("sync schema");
        storage
            .upsert_user(gproxy_storage::UserWrite {
                id: 1,
                name: "alice".to_string(),
                password: hash_password("user-password"),
                enabled: true,
                is_admin: false,
            })
            .await
            .expect("seed user");
        storage
            .upsert_user(gproxy_storage::UserWrite {
                id: 2,
                name: "admin".to_string(),
                password: hash_password("admin-password"),
                enabled: true,
                is_admin: true,
            })
            .await
            .expect("seed admin");

        let state = Arc::new(
            AppStateBuilder::new()
                .engine(gproxy_sdk::provider::engine::GproxyEngine::builder().build())
                .storage(storage)
                .config(GlobalConfig {
                    dsn: "sqlite::memory:".to_string(),
                    ..GlobalConfig::default()
                })
                .build(),
        );
        crate::bootstrap::reload_from_db(&state)
            .await
            .expect("reload state");
        state
    }

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

    #[tokio::test]
    async fn login_allows_admin_user() {
        let state = build_test_state().await;
        let response = login(
            State(state.clone()),
            Json(LoginRequest {
                username: "admin".to_string(),
                password: "admin-password".to_string(),
            }),
        )
        .await
        .expect("admin user should login via /login")
        .0;
        assert!(response.is_admin);
        let session = state
            .validate_session(&response.session_token)
            .expect("session should exist");
        assert_eq!(session.user_id, 2);
    }

    #[tokio::test]
    async fn login_response_marks_non_admin_user() {
        let state = build_test_state().await;
        let response = login(
            State(state),
            Json(LoginRequest {
                username: "alice".to_string(),
                password: "user-password".to_string(),
            }),
        )
        .await
        .expect("user login should succeed")
        .0;
        assert!(!response.is_admin);
    }

    #[tokio::test]
    async fn login_creates_session() {
        let state = build_test_state().await;
        let response = login(
            State(state.clone()),
            Json(LoginRequest {
                username: "alice".to_string(),
                password: "user-password".to_string(),
            }),
        )
        .await
        .expect("login should succeed")
        .0;
        assert!(response.session_token.starts_with("sess-"));
        let session = state
            .validate_session(&response.session_token)
            .expect("session should exist");
        assert_eq!(session.user_id, 1);
    }
}
