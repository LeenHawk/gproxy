//! Credential health endpoints (§16.3): read the persisted per-credential and
//! per-credential-model health snapshots, and reset them.
//!
//! Read routes annotate each row with `provider_id` so the Console can group by
//! provider without a second round trip. The DELETE routes are the operator
//! escape hatch: besides dropping the snapshot they reset this instance's
//! breaker/cooldown soft state, so the next attempt is admitted again.

use std::collections::HashMap;

use bytes::Bytes;
use http::Method;

use crate::admin::guard::guard_admin;
use crate::api::error::ApiError;
use crate::app::AppState;
use crate::store::persistence::records::Credential;

use super::{Request, Resp, internal, parse_i64, segments};

#[derive(serde::Serialize)]
struct CredentialStatusView<T: serde::Serialize> {
    #[serde(flatten)]
    status: T,
    provider_id: Option<i64>,
}

fn status_views<T: serde::Serialize>(
    rows: Vec<T>,
    credentials: Vec<Credential>,
    credential_id: impl Fn(&T) -> i64,
) -> Vec<CredentialStatusView<T>> {
    let provider_ids = credentials
        .into_iter()
        .map(|credential| (credential.id, credential.provider_id))
        .collect::<HashMap<_, _>>();
    rows.into_iter()
        .map(|status| CredentialStatusView {
            provider_id: provider_ids.get(&credential_id(&status)).copied(),
            status,
        })
        .collect()
}

fn status_views_for_provider<T: serde::Serialize>(
    rows: Vec<T>,
    provider_id: Option<i64>,
) -> Vec<CredentialStatusView<T>> {
    rows.into_iter()
        .map(|status| CredentialStatusView {
            status,
            provider_id,
        })
        .collect()
}

/// Route a credential-health request to its handler.
///
/// Returns `Some(result)` when the path matches; `None` to fall through.
pub(super) async fn dispatch(
    state: &AppState,
    parts: &Request,
    _body: &Bytes,
) -> Option<Result<Resp, ApiError>> {
    let segs = segments(parts);
    let r = match (&parts.method, segs.as_slice()) {
        (&Method::GET, ["admin", "credential-statuses"]) => credential_statuses(state, parts).await,
        (&Method::GET, ["admin", "credentials", id, "status"]) => {
            credential_status(state, parts, id).await
        }
        (&Method::GET, ["admin", "credential-model-statuses"]) => {
            credential_model_statuses(state, parts).await
        }
        (&Method::GET, ["admin", "credentials", id, "model-statuses"]) => {
            credential_model_status(state, parts, id).await
        }
        (&Method::DELETE, ["admin", "credentials", id, "status"]) => {
            clear_credential_status(state, parts, id).await
        }
        (&Method::DELETE, ["admin", "credentials", id, "model-statuses"]) => {
            clear_credential_model_statuses(state, parts, id).await
        }
        _ => return None,
    };
    Some(r)
}

async fn credential_statuses(state: &AppState, parts: &Request) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let (rows, credentials) = futures_util::try_join!(
        state.persistence.list_all_credential_statuses(),
        state.persistence.list_all_credentials(),
    )
    .map_err(internal)?;
    let rows = status_views(rows, credentials, |status| status.credential_id);
    Resp::json(200, &rows)
}

async fn credential_status(state: &AppState, parts: &Request, id: &str) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let id = parse_i64(id)?;
    let (rows, credential) = futures_util::try_join!(
        state.persistence.list_credential_statuses(id),
        state.persistence.get_credential(id),
    )
    .map_err(internal)?;
    let rows = status_views_for_provider(rows, credential.map(|row| row.provider_id));
    Resp::json(200, &rows)
}

async fn credential_model_statuses(state: &AppState, parts: &Request) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let (rows, credentials) = futures_util::try_join!(
        state.persistence.list_all_credential_model_statuses(),
        state.persistence.list_all_credentials(),
    )
    .map_err(internal)?;
    let rows = status_views(rows, credentials, |status| status.credential_id);
    Resp::json(200, &rows)
}

async fn credential_model_status(
    state: &AppState,
    parts: &Request,
    id: &str,
) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let id = parse_i64(id)?;
    let (rows, credential) = futures_util::try_join!(
        state.persistence.list_credential_model_statuses(id),
        state.persistence.get_credential(id),
    )
    .map_err(internal)?;
    let rows = status_views_for_provider(rows, credential.map(|row| row.provider_id));
    Resp::json(200, &rows)
}

/// Operator reset of one credential's credential-wide health: drops the
/// persisted snapshot and this instance's breaker/cooldown. Health is
/// per-instance soft state, so other instances keep theirs until it expires.
async fn clear_credential_status(
    state: &AppState,
    parts: &Request,
    id: &str,
) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let id = parse_i64(id)?;
    state
        .persistence
        .clear_credential_statuses(id)
        .await
        .map_err(internal)?;
    state.health.clear_credential(id);
    Ok(Resp::no_content())
}

/// The same reset, scoped to every model-bound health entry of the credential.
async fn clear_credential_model_statuses(
    state: &AppState,
    parts: &Request,
    id: &str,
) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let id = parse_i64(id)?;
    state
        .persistence
        .clear_credential_model_statuses(id)
        .await
        .map_err(internal)?;
    let cleared = state.health.clear_credential_models(id);
    tracing::debug!(credential_id = id, cleared, "credential model health reset");
    Ok(Resp::no_content())
}
