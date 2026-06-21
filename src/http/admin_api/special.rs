//! Special admin CRUD — user-keys, users, credentials.
//!
//! These entities are "special" because their writes involve server-side
//! cryptography that the standard `edge_crud!` macro cannot abstract:
//!
//! * **user-keys**: the bare API key is GENERATED server-side (CSPRNG), its
//!   digest derived, the value sealed; the bare key is returned ONCE in the
//!   create response and never again. Caller-supplied key material is rejected
//!   (400). On update, the existing digest + ciphertext are kept immutably.
//!
//! * **users**: plaintext passwords are HASHED (argon2) on write; on update
//!   without a new password the existing hash is preserved; the hash is never
//!   serialized (reads return `has_password` instead).
//!
//! * **credentials**: the plaintext `secret_json` is SEALED (cipher) on write;
//!   on update without a new secret the stored ciphertext is preserved; the
//!   sealed value is never serialized (reads return `has_secret` instead).
//!   Reads and writes are scoped to a `provider_id` path parameter.
//!
//! Security contract: this file MUST replicate the native handlers
//! (`src/http/server/admin/crud/{user_keys,users,credentials}.rs`) exactly.
//! Any divergence in seal / hash / redact / ownership logic is a security bug.

use bytes::Bytes;
use http::Method;
use http::request::Parts;

use crate::admin::guard::guard_admin;
use crate::admin::invalidate;
use crate::api::credentials::{CredentialUpsert, CredentialView};
use crate::api::error::ApiError;
use crate::api::user_keys::{UserKeyUpsert, UserKeyView};
use crate::api::users::{UserUpsert, UserView};
use crate::app::AppState;
use crate::pipeline::auth::key_digest;
use crate::store::persistence::records::{CredentialInput, UserInput, UserKeyInput};

use super::{Resp, internal, json_body, parse_i64, segments};

// ── user-keys ─────────────────────────────────────────────────────────────────

/// Handle `GET/POST /admin/users/{user_id}/keys` and
/// `DELETE /admin/user-keys/{id}`.
///
/// Returns `Some(result)` on a path match, `None` to fall through.
pub(super) async fn dispatch_user_keys(
    state: &AppState,
    parts: &Parts,
    body: &Bytes,
) -> Option<Result<Resp, ApiError>> {
    let segs = segments(parts);
    match (&parts.method, segs.as_slice()) {
        // GET /admin/users/{user_id}/keys — redacted list
        (&Method::GET, ["admin", "users", user_id, "keys"]) => Some(
            async {
                guard_admin(state, parts).await?;
                let user_id = parse_i64(user_id)?;
                let keys = state
                    .persistence
                    .list_user_keys(user_id)
                    .await
                    .map_err(internal)?;
                Resp::json(
                    200,
                    &keys.into_iter().map(UserKeyView::from).collect::<Vec<_>>(),
                )
            }
            .await,
        ),

        // POST /admin/users/{user_id}/keys — create or update
        (&Method::POST, ["admin", "users", user_id, "keys"]) => Some(
            async {
                guard_admin(state, parts).await?;
                let user_id = parse_i64(user_id)?;
                let body: UserKeyUpsert = json_body(body)?;

                // Reject caller-supplied key material (security: keys are
                // generated server-side; external import uses a separate path).
                if body.api_key.is_some() {
                    return Err(ApiError::BadRequest(
                        "api_key is not accepted: keys are generated server-side on create \
                         (external key material is import-only)"
                            .into(),
                    ));
                }

                // Resolve (digest, ciphertext, bare) — mirrors native user_keys.rs.
                let (digest, ciphertext, bare) = match body.id {
                    // Create → mint the key here (CSPRNG).
                    None => {
                        let bare = crate::util::rand::api_key();
                        let digest = key_digest(&bare);
                        let sealed = state
                            .cipher
                            .seal(&serde_json::Value::String(bare.clone()))
                            .map_err(internal)?;
                        let ciphertext = match &sealed {
                            serde_json::Value::String(s) => s.clone(),
                            other => serde_json::to_string(other).map_err(internal)?,
                        };
                        (digest, ciphertext, Some(bare))
                    }
                    // Update → keep the existing digest + ciphertext immutably.
                    // Ownership check: the stored user_id MUST match the path user_id.
                    Some(id) => {
                        let existing = state
                            .persistence
                            .get_user_key(id)
                            .await
                            .map_err(internal)?
                            .filter(|k| k.user_id == user_id)
                            .ok_or_else(|| ApiError::NotFound("not found".into()))?;
                        (existing.api_key_digest, existing.api_key_ciphertext, None)
                    }
                };

                let input = UserKeyInput {
                    id: body.id,
                    user_id,
                    api_key_digest: digest,
                    api_key_ciphertext: ciphertext,
                    label: body.label,
                    enabled: body.enabled,
                };
                let key = state
                    .persistence
                    .upsert_user_key(input)
                    .await
                    .map_err(ApiError::from_upsert)?;
                invalidate(state).await;
                let mut view = UserKeyView::from(key);
                // Set the bare key in the response ONLY on create (plaintext-once).
                view.api_key = bare;
                Resp::json(200, &view)
            }
            .await,
        ),

        // DELETE /admin/user-keys/{id} — 204 on removal, 404 otherwise
        (&Method::DELETE, ["admin", "user-keys", id]) => Some(
            async {
                guard_admin(state, parts).await?;
                let id = parse_i64(id)?;
                let deleted = state
                    .persistence
                    .delete_user_key(id)
                    .await
                    .map_err(internal)?;
                if deleted {
                    invalidate(state).await;
                    Ok(Resp::no_content())
                } else {
                    Err(ApiError::NotFound("not found".into()))
                }
            }
            .await,
        ),

        _ => None,
    }
}

// ── users ─────────────────────────────────────────────────────────────────────

/// Handle `GET /admin/users`, `GET /admin/users/{id}`,
/// `POST /admin/users`, `DELETE /admin/users/{id}`.
///
/// Returns `Some(result)` on a path match, `None` to fall through.
pub(super) async fn dispatch_users(
    state: &AppState,
    parts: &Parts,
    body: &Bytes,
) -> Option<Result<Resp, ApiError>> {
    let segs = segments(parts);
    match (&parts.method, segs.as_slice()) {
        // GET /admin/users — list with redacted password hash
        (&Method::GET, ["admin", "users"]) => Some(
            async {
                guard_admin(state, parts).await?;
                let users = state.persistence.list_users().await.map_err(internal)?;
                Resp::json(
                    200,
                    &users.into_iter().map(UserView::from).collect::<Vec<_>>(),
                )
            }
            .await,
        ),

        // GET /admin/users/{id} — one redacted UserView, or 404
        (&Method::GET, ["admin", "users", id]) => Some(
            async {
                guard_admin(state, parts).await?;
                let id = parse_i64(id)?;
                match state.persistence.get_user(id).await.map_err(internal)? {
                    Some(user) => Resp::json(200, &UserView::from(user)),
                    None => Err(ApiError::NotFound("not found".into())),
                }
            }
            .await,
        ),

        // POST /admin/users — create or update; hash password
        (&Method::POST, ["admin", "users"]) => Some(
            async {
                guard_admin(state, parts).await?;
                let body: UserUpsert = json_body(body)?;

                // Resolve the password hash — mirrors native users.rs exactly.
                let password = match (&body.password, body.id) {
                    // New plaintext supplied → policy-gate, then hash.
                    (Some(pw), _) => {
                        crate::crypto::password::validate_new(pw).map_err(ApiError::BadRequest)?;
                        Some(crate::crypto::password::hash(pw).map_err(internal)?)
                    }
                    // No password on update → keep the existing hash.
                    (None, Some(id)) => state
                        .persistence
                        .get_user(id)
                        .await
                        .map_err(internal)?
                        .and_then(|u| u.password),
                    // No password on create → no login until set.
                    (None, None) => None,
                };

                let input = UserInput {
                    id: body.id,
                    name: body.name,
                    org_id: body.org_id,
                    team_id: body.team_id,
                    password,
                    enabled: body.enabled,
                    is_admin: body.is_admin,
                };
                let user = state
                    .persistence
                    .upsert_user(input)
                    .await
                    .map_err(ApiError::from_upsert)?;
                invalidate(state).await;
                Resp::json(200, &UserView::from(user))
            }
            .await,
        ),

        // DELETE /admin/users/{id} — 204 on removal, 404 otherwise
        (&Method::DELETE, ["admin", "users", id]) => Some(
            async {
                guard_admin(state, parts).await?;
                let id = parse_i64(id)?;
                let deleted = state.persistence.delete_user(id).await.map_err(internal)?;
                if deleted {
                    invalidate(state).await;
                    Ok(Resp::no_content())
                } else {
                    Err(ApiError::NotFound("not found".into()))
                }
            }
            .await,
        ),

        _ => None,
    }
}

// ── credentials ───────────────────────────────────────────────────────────────

/// Handle `GET /admin/providers/{pid}/credentials`,
/// `GET /admin/providers/{pid}/credentials/{id}`,
/// `POST /admin/providers/{pid}/credentials`,
/// `DELETE /admin/credentials/{id}`.
///
/// Returns `Some(result)` on a path match, `None` to fall through.
pub(super) async fn dispatch_credentials(
    state: &AppState,
    parts: &Parts,
    body: &Bytes,
) -> Option<Result<Resp, ApiError>> {
    let segs = segments(parts);
    match (&parts.method, segs.as_slice()) {
        // GET /admin/providers/{provider_id}/credentials — redacted list
        (&Method::GET, ["admin", "providers", provider_id, "credentials"]) => Some(
            async {
                guard_admin(state, parts).await?;
                let provider_id = parse_i64(provider_id)?;
                let creds = state
                    .persistence
                    .list_credentials(provider_id)
                    .await
                    .map_err(internal)?;
                Resp::json(
                    200,
                    &creds
                        .into_iter()
                        .map(CredentialView::from)
                        .collect::<Vec<_>>(),
                )
            }
            .await,
        ),

        // GET /admin/providers/{provider_id}/credentials/{id} — scoped get, or 404
        (&Method::GET, ["admin", "providers", provider_id, "credentials", id]) => Some(
            async {
                guard_admin(state, parts).await?;
                let provider_id = parse_i64(provider_id)?;
                let id = parse_i64(id)?;
                match state
                    .persistence
                    .get_credential(id)
                    .await
                    .map_err(internal)?
                {
                    // Provider-scope check: the stored provider_id must match the path.
                    Some(c) if c.provider_id == provider_id => {
                        Resp::json(200, &CredentialView::from(c))
                    }
                    _ => Err(ApiError::NotFound("not found".into())),
                }
            }
            .await,
        ),

        // POST /admin/providers/{provider_id}/credentials — create or update; seal secret
        (&Method::POST, ["admin", "providers", provider_id, "credentials"]) => Some(
            async {
                guard_admin(state, parts).await?;
                let provider_id = parse_i64(provider_id)?;
                let body: CredentialUpsert = json_body(body)?;

                // Resolve the sealed secret — mirrors native credentials.rs exactly.
                let secret_json = match (&body.secret_json, body.id) {
                    // New plaintext supplied → seal it.
                    (Some(plain), _) => state.cipher.seal(plain).map_err(internal)?,
                    // No secret on update → keep the existing (already sealed) value.
                    (None, Some(id)) => {
                        let existing = state
                            .persistence
                            .get_credential(id)
                            .await
                            .map_err(internal)?
                            .filter(|c| c.provider_id == provider_id)
                            .ok_or_else(|| ApiError::NotFound("not found".into()))?;
                        existing.secret_json
                    }
                    // No secret on create → reject.
                    (None, None) => {
                        return Err(ApiError::BadRequest(
                            "secret_json required on create".into(),
                        ));
                    }
                };

                let input = CredentialInput {
                    id: body.id,
                    provider_id,
                    name: body.label,
                    kind: body.kind,
                    secret_json,
                    weight: body.weight,
                    rpm_limit: body.rpm_limit,
                    tpm_limit: body.tpm_limit,
                    proxy_url: body.proxy_url,
                    tls_fingerprint: body.tls_fingerprint,
                    enabled: body.enabled,
                };
                let cred = state
                    .persistence
                    .upsert_credential(input)
                    .await
                    .map_err(ApiError::from_upsert)?;
                invalidate(state).await;
                Resp::json(200, &CredentialView::from(cred))
            }
            .await,
        ),

        // DELETE /admin/credentials/{id} — 204 on removal, 404 otherwise
        (&Method::DELETE, ["admin", "credentials", id]) => Some(
            async {
                guard_admin(state, parts).await?;
                let id = parse_i64(id)?;
                let deleted = state
                    .persistence
                    .delete_credential(id)
                    .await
                    .map_err(internal)?;
                if deleted {
                    invalidate(state).await;
                    Ok(Resp::no_content())
                } else {
                    Err(ApiError::NotFound("not found".into()))
                }
            }
            .await,
        ),

        _ => None,
    }
}

// ── Sub-dispatcher ────────────────────────────────────────────────────────────

/// Try each special CRUD entity in order; return the first `Some`.
///
/// Collision analysis with existing dispatchers:
///
///   `dispatch_users`:
///     `["admin","users"]`         — new (list/upsert, 2 segs)
///     `["admin","users", id]`     — new (get/delete, 3 segs)
///     `["admin","users", uid, "keys"]` — new (4 segs; disjoint from 3-seg users)
///
///   `dispatch_credentials` under providers (4-seg):
///     `["admin","providers", pid, "credentials"]`       — 4 segs; nested.rs handles
///     `["admin","providers", pid, "rule-sets"]`         — different child_seg
///     `["admin","providers", pid, "models"]`            — different child_seg
///     `["admin","providers", pid, "credentials", id]`  — 5 segs; uniquely new
///     `["admin","credentials", id, "status"]`          — observability; different prefix
///     `["admin","credentials", id]` DELETE             — new (disjoint from observability GET)
///
/// The special dispatcher is called BEFORE the 6-segment identity arm in mod.rs
/// so the nested `users/{uid}/keys` arm is not confused with the 3-seg `users/{id}`.
pub(super) async fn dispatch(
    state: &AppState,
    parts: &Parts,
    body: &Bytes,
) -> Option<Result<Resp, ApiError>> {
    // user-keys first: the 4-seg arm `users/{uid}/keys` must be tested before
    // dispatch_users's 3-seg arm `users/{id}` so it is not shadowed.
    if let Some(r) = dispatch_user_keys(state, parts, body).await {
        return Some(r);
    }
    if let Some(r) = dispatch_users(state, parts, body).await {
        return Some(r);
    }
    dispatch_credentials(state, parts, body).await
}
