use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{HeaderMap, Uri};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use gproxy_server::AppState;
use gproxy_server::principal::MemoryUserKey;

use crate::error::HttpError;

#[derive(Debug, Clone)]
pub struct AuthenticatedUser(pub MemoryUserKey);

/// Extract API key from request headers.
/// Checks: Authorization: Bearer <key>, x-api-key, x-goog-api-key.
pub fn extract_api_key(headers: &HeaderMap) -> Result<String, HttpError> {
    // Authorization: Bearer <key>
    if let Some(value) = headers.get("authorization")
        && let Ok(s) = value.to_str()
        && let Some(token) = s
            .strip_prefix("Bearer ")
            .or_else(|| s.strip_prefix("bearer "))
    {
        let trimmed = token.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }
    // x-api-key
    if let Some(value) = headers.get("x-api-key")
        && let Ok(s) = value.to_str()
    {
        let trimmed = s.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }
    // x-goog-api-key
    if let Some(value) = headers.get("x-goog-api-key")
        && let Ok(s) = value.to_str()
    {
        let trimmed = s.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }
    Err(HttpError::unauthorized("missing API key"))
}

/// Extract API key from the `?key=<value>` query parameter (Gemini native auth).
///
/// Only returns a non-empty key. Returns `None` if the URI has no `key` param
/// or the value is empty — callers fall back to header-based extraction.
pub fn extract_api_key_from_uri(uri: &Uri) -> Option<String> {
    let query = uri.query()?;
    let (_, value) = url::form_urlencoded::parse(query.as_bytes()).find(|(k, _)| k == "key")?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Authenticate a user API key and return the key record.
pub fn authenticate_user(
    headers: &HeaderMap,
    state: &AppState,
) -> Result<MemoryUserKey, HttpError> {
    let api_key = extract_api_key(headers)?;
    state
        .authenticate_api_key(&api_key)
        .ok_or_else(|| HttpError::unauthorized("invalid or disabled API key"))
}

/// Extract a bearer token from headers first, falling back to `?key=<value>`
/// in the URI so Gemini-native clients (which put the key in the URL) work.
fn extract_api_key_header_or_query(headers: &HeaderMap, uri: &Uri) -> Result<String, HttpError> {
    match extract_api_key(headers) {
        Ok(key) => Ok(key),
        Err(header_err) => extract_api_key_from_uri(uri).ok_or(header_err),
    }
}

/// Authenticate as admin using either an admin session token or an API key
/// owned by an admin user.
pub fn authorize_admin(headers: &HeaderMap, state: &AppState) -> Result<(), HttpError> {
    let token = extract_api_key(headers)?;
    authorize_admin_token(&token, state)
}

/// Authenticate as admin using a pre-extracted bearer token. Used by the
/// admin middleware so it can fall back to the `?key=` query parameter.
pub fn authorize_admin_token(token: &str, state: &AppState) -> Result<(), HttpError> {
    if token.starts_with("sess-") {
        let session_user = authenticate_session(token, state)?;
        if session_user.is_admin {
            return Ok(());
        }
        return Err(HttpError::forbidden("admin access required"));
    }

    let user_key = state
        .authenticate_api_key(token)
        .ok_or_else(|| HttpError::unauthorized("invalid or disabled API key"))?;
    let user = state
        .find_user(user_key.user_id)
        .ok_or_else(|| HttpError::unauthorized("user not found"))?;
    if user.is_admin {
        Ok(())
    } else {
        Err(HttpError::forbidden("admin access required"))
    }
}

pub async fn require_user_middleware(
    State(state): State<Arc<AppState>>,
    mut request: Request,
    next: Next,
) -> Response {
    let auth_result =
        extract_api_key_header_or_query(request.headers(), request.uri()).and_then(|api_key| {
            state
                .authenticate_api_key(&api_key)
                .ok_or_else(|| HttpError::unauthorized("invalid or disabled API key"))
        });
    match auth_result {
        Ok(user_key) => {
            request.extensions_mut().insert(AuthenticatedUser(user_key));
            next.run(request).await
        }
        Err(err) => {
            tracing::warn!(
                method = %request.method(),
                path = request.uri().path(),
                status = err.status.as_u16(),
                error = %err.message,
                "provider request rejected by auth middleware"
            );
            err.into_response()
        }
    }
}

pub async fn require_admin_middleware(
    State(state): State<Arc<AppState>>,
    request: Request,
    next: Next,
) -> Response {
    let auth_result = extract_api_key_header_or_query(request.headers(), request.uri())
        .and_then(|token| authorize_admin_token(&token, &state));
    match auth_result {
        Ok(()) => next.run(request).await,
        Err(err) => {
            tracing::warn!(
                method = %request.method(),
                path = request.uri().path(),
                status = err.status.as_u16(),
                error = %err.message,
                "admin request rejected by auth middleware"
            );
            err.into_response()
        }
    }
}

/// Authenticated session user (from session token, not API key).
#[derive(Debug, Clone)]
pub struct SessionUser {
    pub user_id: i64,
    pub is_admin: bool,
}

fn authenticate_session(token: &str, state: &AppState) -> Result<SessionUser, HttpError> {
    let session = state
        .validate_session(token)
        .ok_or_else(|| HttpError::unauthorized("session expired or invalid"))?;
    let user = state
        .find_user(session.user_id)
        .ok_or_else(|| HttpError::unauthorized("user not found"))?;
    if !user.enabled {
        return Err(HttpError::forbidden("user is disabled"));
    }
    Ok(SessionUser {
        user_id: user.id,
        is_admin: user.is_admin,
    })
}

/// Middleware for /user/* routes: requires a session token (from /login).
///
/// Session tokens are short-lived (24h) and memory-only.
/// This separates user management auth from provider proxy auth,
/// so a leaked inference API key cannot be used to generate new keys
/// or enumerate existing ones.
pub async fn require_user_session_middleware(
    State(state): State<Arc<AppState>>,
    mut request: Request,
    next: Next,
) -> Response {
    let token = match extract_api_key(request.headers()) {
        Ok(t) => t,
        Err(err) => return err.into_response(),
    };

    // Accept session tokens (sess-*) for /user/* routes
    if token.starts_with("sess-") {
        match authenticate_session(&token, &state) {
            Ok(session_user) => {
                request.extensions_mut().insert(session_user);
                return next.run(request).await;
            }
            Err(err) => return err.into_response(),
        }
    }

    HttpError::unauthorized("session token required (use /login to obtain one)").into_response()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::Router;
    use axum::body::Body;
    use axum::http::{Request as HttpRequest, StatusCode};
    use axum::middleware::from_fn_with_state;
    use axum::routing::get;
    use tower::ServiceExt;

    use super::{
        require_admin_middleware, require_user_middleware, require_user_session_middleware,
    };
    use gproxy_server::{AppState, AppStateBuilder, GlobalConfig};
    use gproxy_storage::{SeaOrmStorage, repository::UserRepository};

    async fn build_test_state() -> Arc<AppState> {
        let storage = Arc::new(
            SeaOrmStorage::connect("sqlite::memory:", None)
                .await
                .expect("in-memory sqlite storage"),
        );
        storage.sync().await.expect("sync schema");
        storage
            .upsert_user(gproxy_storage::UserWrite {
                id: 1,
                name: "admin".to_string(),
                password: crate::login::hash_password("admin-password"),
                enabled: true,
                is_admin: true,
            })
            .await
            .expect("seed admin");
        storage
            .upsert_user_key(gproxy_storage::UserKeyWrite {
                id: 10,
                user_id: 1,
                api_key: "sk-admin".to_string(),
                label: Some("admin".to_string()),
                enabled: true,
            })
            .await
            .expect("seed admin key");
        storage
            .upsert_user(gproxy_storage::UserWrite {
                id: 2,
                name: "alice".to_string(),
                password: crate::login::hash_password("user-password"),
                enabled: true,
                is_admin: false,
            })
            .await
            .expect("seed user");
        storage
            .upsert_user_key(gproxy_storage::UserKeyWrite {
                id: 20,
                user_id: 2,
                api_key: "sk-user".to_string(),
                label: Some("user".to_string()),
                enabled: true,
            })
            .await
            .expect("seed user key");

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

    fn admin_router(state: Arc<AppState>) -> Router {
        Router::new()
            .route("/check", get(|| async { StatusCode::NO_CONTENT }))
            .layer(from_fn_with_state(state.clone(), require_admin_middleware))
            .with_state(state)
    }

    fn user_router(state: Arc<AppState>) -> Router {
        Router::new()
            .route("/check", get(|| async { StatusCode::NO_CONTENT }))
            .layer(from_fn_with_state(
                state.clone(),
                require_user_session_middleware,
            ))
            .with_state(state)
    }

    fn user_api_router(state: Arc<AppState>) -> Router {
        Router::new()
            .route(
                "/aistudio/v1beta/models/x:generateContent",
                get(|| async { StatusCode::NO_CONTENT }),
            )
            .layer(from_fn_with_state(state.clone(), require_user_middleware))
            .with_state(state)
    }

    #[tokio::test]
    async fn provider_route_accepts_and_rejects_query_api_key() {
        let cases = [
            (
                "/aistudio/v1beta/models/x:generateContent?key=sk-user",
                StatusCode::NO_CONTENT,
            ),
            (
                "/aistudio/v1beta/models/x:generateContent",
                StatusCode::UNAUTHORIZED,
            ),
            (
                "/aistudio/v1beta/models/x:generateContent?key=sk-bogus",
                StatusCode::UNAUTHORIZED,
            ),
            (
                "/aistudio/v1beta/models/x:generateContent?key=",
                StatusCode::UNAUTHORIZED,
            ),
        ];
        for (uri, expected) in cases {
            let state = build_test_state().await;
            let response = user_api_router(state)
                .oneshot(
                    HttpRequest::builder()
                        .uri(uri)
                        .body(Body::empty())
                        .expect("build request"),
                )
                .await
                .expect("router response");
            assert_eq!(response.status(), expected, "uri = {uri}");
        }
    }

    #[tokio::test]
    async fn admin_route_accepts_admin_owned_api_key() {
        let state = build_test_state().await;
        let response = admin_router(state)
            .oneshot(
                HttpRequest::builder()
                    .uri("/check")
                    .header("authorization", "Bearer sk-admin")
                    .body(Body::empty())
                    .expect("build request"),
            )
            .await
            .expect("router response");
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn admin_route_accepts_admin_session() {
        let state = build_test_state().await;
        let token = state.create_session(1, 60);
        let response = admin_router(state)
            .oneshot(
                HttpRequest::builder()
                    .uri("/check")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .expect("build request"),
            )
            .await
            .expect("router response");
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn user_route_accepts_admin_session() {
        let state = build_test_state().await;
        let token = state.create_session(1, 60);
        let response = user_router(state)
            .oneshot(
                HttpRequest::builder()
                    .uri("/check")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .expect("build request"),
            )
            .await
            .expect("router response");
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn admin_route_rejects_non_admin_api_key() {
        let state = build_test_state().await;
        let response = admin_router(state)
            .oneshot(
                HttpRequest::builder()
                    .uri("/check")
                    .header("authorization", "Bearer sk-user")
                    .body(Body::empty())
                    .expect("build request"),
            )
            .await
            .expect("router response");
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn admin_route_rejects_session_after_admin_role_removed() {
        let state = build_test_state().await;
        let token = state.create_session(1, 60);
        state.upsert_user_in_memory(gproxy_server::MemoryUser {
            id: 1,
            name: "admin".to_string(),
            enabled: true,
            is_admin: false,
            password_hash: crate::login::hash_password("admin-password"),
        });
        let response = admin_router(state)
            .oneshot(
                HttpRequest::builder()
                    .uri("/check")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .expect("build request"),
            )
            .await
            .expect("router response");
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn user_route_rejects_disabled_session_user() {
        let state = build_test_state().await;
        let token = state.create_session(2, 60);
        state.upsert_user_in_memory(gproxy_server::MemoryUser {
            id: 2,
            name: "alice".to_string(),
            enabled: false,
            is_admin: false,
            password_hash: crate::login::hash_password("user-password"),
        });
        let response = user_router(state)
            .oneshot(
                HttpRequest::builder()
                    .uri("/check")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .expect("build request"),
            )
            .await
            .expect("router response");
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
}
