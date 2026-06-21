//! Admin auth DTOs (login request / session identity).

/// `POST /admin/login` body.
#[derive(serde::Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// The authenticated identity surfaced by `/admin/me` and `/admin/login`.
#[derive(serde::Serialize)]
pub struct MeResponse {
    pub id: i64,
    pub name: String,
    pub is_admin: bool,
}

/// `POST /admin/login` 200 body.
#[derive(serde::Serialize)]
pub struct LoginResponse {
    pub user: MeResponse,
}
