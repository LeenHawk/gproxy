//! Portal `/user/*` dispatcher (session-scoped) for the edge worker.
//!
//! This module mirrors the native handlers in `src/http/server/admin/user/`
//! exactly. Every handler obtains the user identity from `guard_session` and
//! uses `SessionUser.id` as the sole source of `user_id` — the request body,
//! query string, and path parameters are NEVER trusted for ownership decisions.
//!
//! Security invariants:
//! - Cross-user key access → 404 (no existence leak).
//! - `/user/usage` has no `user_id` field in its query struct; the parameter is
//!   structurally un-smuggleable — serde drops unknown fields silently.
//! - Effective authz scope_ids come exclusively from the `SessionUser` record.
//! - `change-password` does NOT invalidate the session (spec §6).

use bytes::Bytes;
use http::Method;
use http::request::Parts;
use serde::Deserialize;

use crate::admin::guard::guard_session;
use crate::admin::invalidate;
use crate::api::error::ApiError;
use crate::api::user_keys::UserKeyView;
use crate::app::AppState;
use crate::pipeline::auth::key_digest;
use crate::store::persistence::UsageQuery as StoreUsageQuery;
use crate::store::persistence::records::{Scope, UserInput, UserKeyInput};

use super::{Resp, internal, json_body, parse_i64, query, segments};

// ── /user/keys ────────────────────────────────────────────────────────────────

/// Body accepted by `POST /user/keys` (create). Only `label` is accepted;
/// supplying `api_key` is a 400 (keys are generated server-side).
#[derive(Deserialize)]
struct CreateKeyBody {
    pub label: Option<String>,
    /// Presence-only sentinel — rejected with 400.
    #[serde(default)]
    pub api_key: Option<String>,
}

/// Body accepted by `PATCH /user/keys/{id}`.
#[derive(Deserialize)]
struct UpdateKeyBody {
    pub label: Option<String>,
    pub enabled: bool,
}

// ── /user/usage ───────────────────────────────────────────────────────────────

/// Query parameters for `GET /user/usage`.
///
/// Deliberately omits `user_id` — it is forced from the session, so a caller
/// passing `?user_id=X` has the extra field silently dropped by serde.
#[derive(Debug, Deserialize)]
struct MyUsageQuery {
    pub at_from: Option<i64>,
    pub at_to: Option<i64>,
    pub route_name: Option<String>,
    pub model: Option<String>,
    pub before_id: Option<i64>,
    pub limit: Option<u64>,
}

/// Query parameters for `GET /user/usage-rollups`.
#[derive(Debug, Deserialize)]
struct MyRollupQuery {
    pub granularity: String,
    pub from: i64,
    pub to: i64,
}

// ── /user/quota|rate-limits|route-permissions ─────────────────────────────────

/// A rule wrapper that tags each record with its effective source layer.
///
/// Serialises as `{ "source": "<layer>", <...rule fields flattened> }`.
/// Mirrors the native `Effective<T>` in `server/admin/user/authz.rs`.
#[derive(serde::Serialize)]
struct Effective<T: serde::Serialize> {
    pub source: &'static str,
    #[serde(flatten)]
    pub rule: T,
}

// ── /user/change-password ─────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ChangePassword {
    pub current: String,
    pub new: String,
}

// ── Sub-dispatcher ────────────────────────────────────────────────────────────

/// Route a `/user/*` request (excluding `/user/me`) to its handler.
///
/// Returns `Some(result)` on a path match, `None` to fall through.
pub(super) async fn dispatch(
    state: &AppState,
    parts: &Parts,
    body: &Bytes,
) -> Option<Result<Resp, ApiError>> {
    let segs = segments(parts);
    let r = match (&parts.method, segs.as_slice()) {
        // ── keys ──────────────────────────────────────────────────────────────
        (&Method::GET, ["user", "keys"]) => list_keys(state, parts).await,
        (&Method::POST, ["user", "keys"]) => create_key(state, parts, body).await,
        (&Method::PATCH, ["user", "keys", id]) => update_key(state, parts, body, id).await,
        (&Method::DELETE, ["user", "keys", id]) => delete_key(state, parts, id).await,

        // ── usage ─────────────────────────────────────────────────────────────
        (&Method::GET, ["user", "usage"]) => user_usage(state, parts).await,
        (&Method::GET, ["user", "usage-rollups"]) => user_usage_rollups(state, parts).await,

        // ── effective authz ───────────────────────────────────────────────────
        (&Method::GET, ["user", "quota"]) => user_quota(state, parts).await,
        (&Method::GET, ["user", "rate-limits"]) => user_rate_limits(state, parts).await,
        (&Method::GET, ["user", "route-permissions"]) => user_route_permissions(state, parts).await,

        // ── account ───────────────────────────────────────────────────────────
        (&Method::POST, ["user", "change-password"]) => change_password(state, parts, body).await,

        _ => return None,
    };
    Some(r)
}

// ── /user/keys handlers ───────────────────────────────────────────────────────

/// `GET /user/keys` — list all keys belonging to the session user.
/// `api_key` is never included in list responses.
async fn list_keys(state: &AppState, parts: &Parts) -> Result<Resp, ApiError> {
    let u = guard_session(state, parts).await?;
    let keys = state
        .persistence
        .list_user_keys(u.id)
        .await
        .map_err(internal)?;
    Resp::json(
        200,
        &keys.into_iter().map(UserKeyView::from).collect::<Vec<_>>(),
    )
}

/// `POST /user/keys` — create a key for the session user; bare key returned once.
/// Caller-supplied `api_key` is rejected (400). `user_id` from session only.
async fn create_key(state: &AppState, parts: &Parts, body: &Bytes) -> Result<Resp, ApiError> {
    let u = guard_session(state, parts).await?;
    let b: CreateKeyBody = json_body(body)?;

    if b.api_key.is_some() {
        return Err(ApiError::BadRequest(
            "api_key is not accepted: keys are generated server-side on create".into(),
        ));
    }

    // Mint the key server-side (CSPRNG).
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

    let input = UserKeyInput {
        id: None,
        user_id: u.id, // always from session — never from request
        api_key_digest: digest,
        api_key_ciphertext: ciphertext,
        label: b.label,
        enabled: true,
    };
    let key = state
        .persistence
        .upsert_user_key(input)
        .await
        .map_err(internal)?;
    invalidate(state).await;

    let mut view = UserKeyView::from(key);
    view.api_key = Some(bare); // plaintext-once
    Resp::json(200, &view)
}

/// `PATCH /user/keys/{id}` — update label/enabled; ownership checked (cross-user → 404).
/// Key material (digest + ciphertext) is immutable — rotate by create + delete.
async fn update_key(
    state: &AppState,
    parts: &Parts,
    body: &Bytes,
    id: &str,
) -> Result<Resp, ApiError> {
    let u = guard_session(state, parts).await?;
    let id = parse_i64(id)?;
    let b: UpdateKeyBody = json_body(body)?;

    let existing = state
        .persistence
        .get_user_key(id)
        .await
        .map_err(internal)?
        .filter(|k| k.user_id == u.id) // ownership check — cross-user → None → 404
        .ok_or_else(|| ApiError::NotFound("not found".into()))?;

    let input = UserKeyInput {
        id: Some(id),
        user_id: u.id, // always from session
        api_key_digest: existing.api_key_digest,
        api_key_ciphertext: existing.api_key_ciphertext,
        label: b.label,
        enabled: b.enabled,
    };
    let key = state
        .persistence
        .upsert_user_key(input)
        .await
        .map_err(internal)?;
    invalidate(state).await;

    // api_key stays None on updates (not returned again).
    Resp::json(200, &UserKeyView::from(key))
}

/// `DELETE /user/keys/{id}` — delete a key the session user owns.
///
/// Ownership is checked before deletion; cross-user access returns 404.
async fn delete_key(state: &AppState, parts: &Parts, id: &str) -> Result<Resp, ApiError> {
    let u = guard_session(state, parts).await?;
    let id = parse_i64(id)?;

    // Ownership check — cross-user access must not reveal existence.
    let _ = state
        .persistence
        .get_user_key(id)
        .await
        .map_err(internal)?
        .filter(|k| k.user_id == u.id)
        .ok_or_else(|| ApiError::NotFound("not found".into()))?;

    state
        .persistence
        .delete_user_key(id)
        .await
        .map_err(internal)?;
    invalidate(state).await;

    Ok(Resp::no_content())
}

// ── /user/usage handlers ──────────────────────────────────────────────────────

const DEFAULT_LIMIT: u64 = 100;
const MAX_LIMIT: u64 = 1000;

/// `GET /user/usage` — session-scoped; `user_id` forced from session (param absent
/// from `MyUsageQuery` → structurally un-smuggleable via `?user_id=`).
async fn user_usage(state: &AppState, parts: &Parts) -> Result<Resp, ApiError> {
    let u = guard_session(state, parts).await?;
    let q: MyUsageQuery = query(parts)?;
    let limit = q.limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);
    let store_q = StoreUsageQuery {
        user_id: Some(u.id), // forced from session — never from request
        at_from: q.at_from,
        at_to: q.at_to,
        route_name: q.route_name,
        model: q.model,
        before_id: q.before_id,
        limit,
        ..Default::default() // provider_id stays None
    };
    let rows = state
        .persistence
        .query_usages(&store_q)
        .await
        .map_err(internal)?;
    Resp::json(200, &rows)
}

/// `GET /user/usage-rollups?granularity=hour|day|week|month&from=&to=`
/// Returns rollup buckets scoped to the authenticated user only.
async fn user_usage_rollups(state: &AppState, parts: &Parts) -> Result<Resp, ApiError> {
    let u = guard_session(state, parts).await?;
    let q: MyRollupQuery = query(parts)?;
    if !matches!(q.granularity.as_str(), "hour" | "day" | "week" | "month") {
        return Err(ApiError::BadRequest(
            "granularity must be one of hour|day|week|month".into(),
        ));
    }
    let rows = state
        .persistence
        .list_usage_rollups(&q.granularity, q.from, q.to, Some(u.id))
        .await
        .map_err(internal)?;
    Resp::json(200, &rows)
}

// ── /user/quota|rate-limits|route-permissions handlers ───────────────────────

/// `GET /user/quota` — effective quota at user → team → org (scope_ids from session).
async fn user_quota(state: &AppState, parts: &Parts) -> Result<Resp, ApiError> {
    let u = guard_session(state, parts).await?;
    let mut out: Vec<Effective<crate::store::persistence::records::Quota>> = Vec::new();

    if let Some(q) = state
        .persistence
        .get_quota(Scope::User, u.id)
        .await
        .map_err(internal)?
    {
        out.push(Effective {
            source: "user",
            rule: q,
        });
    }
    if let Some(tid) = u.team_id
        && let Some(q) = state
            .persistence
            .get_quota(Scope::Team, tid)
            .await
            .map_err(internal)?
    {
        out.push(Effective {
            source: "team",
            rule: q,
        });
    }
    if let Some(q) = state
        .persistence
        .get_quota(Scope::Org, u.org_id)
        .await
        .map_err(internal)?
    {
        out.push(Effective {
            source: "org",
            rule: q,
        });
    }
    Resp::json(200, &out)
}

/// `GET /user/rate-limits` — effective rate limits at user → team → org layers.
async fn user_rate_limits(state: &AppState, parts: &Parts) -> Result<Resp, ApiError> {
    let u = guard_session(state, parts).await?;
    let mut out: Vec<Effective<crate::store::persistence::records::RateLimit>> = Vec::new();

    for r in state
        .persistence
        .list_rate_limits(Scope::User, u.id)
        .await
        .map_err(internal)?
    {
        out.push(Effective {
            source: "user",
            rule: r,
        });
    }
    if let Some(tid) = u.team_id {
        for r in state
            .persistence
            .list_rate_limits(Scope::Team, tid)
            .await
            .map_err(internal)?
        {
            out.push(Effective {
                source: "team",
                rule: r,
            });
        }
    }
    for r in state
        .persistence
        .list_rate_limits(Scope::Org, u.org_id)
        .await
        .map_err(internal)?
    {
        out.push(Effective {
            source: "org",
            rule: r,
        });
    }
    Resp::json(200, &out)
}

/// `GET /user/route-permissions` — effective route permissions at user → team → org layers.
async fn user_route_permissions(state: &AppState, parts: &Parts) -> Result<Resp, ApiError> {
    let u = guard_session(state, parts).await?;
    let mut out: Vec<Effective<crate::store::persistence::records::RoutePermission>> = Vec::new();

    for p in state
        .persistence
        .list_route_permissions(Scope::User, u.id)
        .await
        .map_err(internal)?
    {
        out.push(Effective {
            source: "user",
            rule: p,
        });
    }
    if let Some(tid) = u.team_id {
        for p in state
            .persistence
            .list_route_permissions(Scope::Team, tid)
            .await
            .map_err(internal)?
        {
            out.push(Effective {
                source: "team",
                rule: p,
            });
        }
    }
    for p in state
        .persistence
        .list_route_permissions(Scope::Org, u.org_id)
        .await
        .map_err(internal)?
    {
        out.push(Effective {
            source: "org",
            rule: p,
        });
    }
    Resp::json(200, &out)
}

// ── /user/change-password handler ────────────────────────────────────────────

/// `POST /user/change-password` — verify+validate+hash; returns 204. Session kept.
async fn change_password(state: &AppState, parts: &Parts, body: &Bytes) -> Result<Resp, ApiError> {
    let u = guard_session(state, parts).await?;
    let b: ChangePassword = json_body(body)?;

    // Fetch user by session-bound id — deleted account treated as invalid session.
    let user = state
        .persistence
        .get_user(u.id)
        .await
        .map_err(internal)?
        .ok_or(ApiError::Unauthorized)?;

    // A user without a password cannot authenticate via password; generic 400.
    let stored_hash = user
        .password
        .as_deref()
        .ok_or_else(|| ApiError::BadRequest("no password set".into()))?;

    // Verify the supplied current password.
    if !crate::crypto::password::verify(&b.current, stored_hash) {
        return Err(ApiError::BadRequest("current password is incorrect".into()));
    }

    // Enforce the minimum-12-character policy.
    crate::crypto::password::validate_new(&b.new).map_err(ApiError::BadRequest)?;

    // Hash the new password (argon2id) and pass it verbatim to upsert_user.
    // upsert_user stores the password field as-is (hashing is the caller's
    // responsibility — same contract as admin crud/users.rs).
    let new_hash = crate::crypto::password::hash(&b.new).map_err(internal)?;

    // Build the upsert input, preserving every other field from the existing record.
    // id: Some(user.id) ensures UPDATE, not INSERT.
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

    // Session intentionally NOT invalidated (spec §6).
    Ok(Resp::no_content())
}
