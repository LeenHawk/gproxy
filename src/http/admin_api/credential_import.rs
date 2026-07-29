//! Batch credential import — `POST /admin/providers/{pid}/credentials/import`.
//!
//! One request creates many credentials. Each item goes through the same
//! plan/seal/dedupe path as a single upsert ([`plan_credential_upsert`]), so an
//! api-key already stored for the provider reports `existing` instead of
//! creating a duplicate. Items fail independently; the response carries a
//! per-item result list in request order plus summary counts.

use bytes::Bytes;
use http::Method;

use crate::admin::credential_upsert::{CredentialUpsertPlan, plan_credential_upsert};
use crate::admin::guard::guard_admin;
use crate::admin::invalidate;
use crate::api::credentials::{
    CredentialImportItem, CredentialImportOutcome, CredentialImportRequest,
};
use crate::api::error::ApiError;
use crate::app::AppState;

use super::{Request, Resp, internal, json_body, parse_i64, segments};

/// Refuse absurdly large batches before doing any per-item work.
const MAX_ITEMS: usize = 1000;

pub(super) async fn dispatch(
    state: &AppState,
    parts: &Request,
    body: &Bytes,
) -> Option<Result<Resp, ApiError>> {
    let segs = segments(parts);
    match (&parts.method, segs.as_slice()) {
        (&Method::POST, ["admin", "providers", provider_id, "credentials", "import"]) => {
            let provider_id = *provider_id;
            Some(import(state, parts, provider_id, body).await)
        }
        _ => None,
    }
}

async fn import(
    state: &AppState,
    parts: &Request,
    provider_id: &str,
    body: &Bytes,
) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let provider_id = parse_i64(provider_id)?;
    let req: CredentialImportRequest = json_body(body)?;

    if req.items.is_empty() {
        return Err(ApiError::BadRequest("items must not be empty".into()));
    }
    if req.items.len() > MAX_ITEMS {
        return Err(ApiError::BadRequest(format!(
            "too many items: {} (max {MAX_ITEMS})",
            req.items.len()
        )));
    }
    // Fail the whole batch early when the provider does not exist (individual
    // items would otherwise all fail on the foreign key).
    state
        .persistence
        .get_provider(provider_id)
        .await
        .map_err(internal)?
        .ok_or_else(|| ApiError::NotFound("provider not found".into()))?;

    let mut outcome = CredentialImportOutcome {
        created: 0,
        existing: 0,
        failed: 0,
        results: Vec::with_capacity(req.items.len()),
    };

    for (index, item) in req.items.into_iter().enumerate() {
        let result = import_item(state, provider_id, index, item).await;
        match result.status {
            "created" => outcome.created += 1,
            "existing" => outcome.existing += 1,
            _ => outcome.failed += 1,
        }
        outcome.results.push(result);
    }

    if outcome.created > 0 {
        invalidate(state).await;
    }
    Resp::json(200, &outcome)
}

async fn import_item(
    state: &AppState,
    provider_id: i64,
    index: usize,
    item: crate::api::credentials::CredentialUpsert,
) -> CredentialImportItem {
    if item.id.is_some() {
        return error_item(index, "id is not allowed on import (create-only)".into());
    }
    let plan = match plan_credential_upsert(state, provider_id, item).await {
        Ok(plan) => plan,
        Err(e) => return error_item(index, e.message()),
    };
    match plan {
        CredentialUpsertPlan::Existing(cred) => CredentialImportItem {
            index,
            status: "existing",
            id: Some(cred.id),
            error: None,
        },
        CredentialUpsertPlan::Upsert(input) => {
            match state.persistence.upsert_credential(input).await {
                Ok(cred) => CredentialImportItem {
                    index,
                    status: "created",
                    id: Some(cred.id),
                    error: None,
                },
                Err(e) => error_item(index, ApiError::from_upsert(e).message()),
            }
        }
    }
}

fn error_item(index: usize, error: String) -> CredentialImportItem {
    CredentialImportItem {
        index,
        status: "error",
        id: None,
        error: Some(error),
    }
}
