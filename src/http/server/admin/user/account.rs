//! `/user/change-password` — self-service password update.
//!
//! SECURITY:
//! - `user_id` comes strictly from `SessionUser.id` (the validated session);
//!   it is never read from the request body or path.
//! - Wrong `current` password → 400 (NOT 401 — no existence oracle; generic
//!   message regardless of whether the account has a password).
//! - Weak `new` password (< 12 chars) → 400 via `validate_new`.
//! - Session is NOT invalidated on success (spec §6: self-change does not log
//!   out the user).
//! - The audit middleware records method + path only; no request body is logged,
//!   so neither the current nor the new password enters the audit trail.
//!
//! Password write strategy (Option A — reuse `upsert_user`):
//! `upsert_user` stores `UserInput.password` verbatim (the admin handler in
//! `crud/users.rs` hashes the raw password *before* constructing `UserInput`).
//! So here we: hash the new password ourselves, then pass the hash in
//! `UserInput.password`.  All other fields are copied from the `User` record
//! fetched above, so no field is accidentally cleared. No double-hashing risk.

use axum::Extension;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::Deserialize;

use crate::admin::session::SessionUser;
use crate::api::error::ApiError;
use crate::app::AppState;
use crate::store::persistence::records::UserInput;

fn internal(e: anyhow::Error) -> ApiError {
    ApiError::Internal(e.to_string())
}

#[derive(Debug, Deserialize)]
pub struct ChangePassword {
    pub current: String,
    pub new: String,
}

/// `POST /user/change-password` — verify the current password, apply policy to
/// the new one, and persist the argon2id hash.  Returns 204 on success.
pub async fn change_password(
    State(state): State<AppState>,
    Extension(u): Extension<SessionUser>,
    Json(body): Json<ChangePassword>,
) -> Result<StatusCode, ApiError> {
    // Fetch the full user record by the session-bound id (never from the
    // request).  Missing user → Unauthorized (session refers to a deleted
    // account; treat identically to an invalid session to avoid leaking state).
    let user = state
        .persistence
        .get_user(u.id)
        .await
        .map_err(internal)?
        .ok_or(ApiError::Unauthorized)?;

    // A user without a password cannot authenticate via password; return a
    // generic 400 that doesn't reveal whether other users have passwords.
    let stored_hash = user
        .password
        .as_deref()
        .ok_or_else(|| ApiError::BadRequest("no password set".into()))?;

    // Verify the supplied current password against the stored argon2id hash.
    // `verify` returns false for a malformed PHC string — no panic.
    if !crate::crypto::password::verify(&body.current, stored_hash) {
        return Err(ApiError::BadRequest("current password is incorrect".into()));
    }

    // Enforce the minimum-12-character policy on the incoming new password.
    crate::crypto::password::validate_new(&body.new).map_err(ApiError::BadRequest)?;

    // Hash the new password (argon2id).  We pass the pre-hashed value to
    // `upsert_user` because `upsert_user` stores the password field verbatim
    // (hashing is the *caller's* responsibility — see crud/users.rs:46-47).
    let new_hash = crate::crypto::password::hash(&body.new).map_err(internal)?;

    // Build the upsert input, preserving every field from the existing record.
    // `id: Some(user.id)` ensures this is an UPDATE, not an INSERT.
    let input = UserInput {
        id: Some(user.id),
        name: user.name,
        org_id: user.org_id,
        team_id: user.team_id,
        password: Some(new_hash),
        enabled: user.enabled,
        is_admin: user.is_admin,
    };
    state
        .persistence
        .upsert_user(input)
        .await
        .map_err(internal)?;

    // Session is intentionally NOT invalidated (spec §6).
    Ok(StatusCode::NO_CONTENT)
}
