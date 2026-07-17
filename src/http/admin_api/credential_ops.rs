//! Edge-safe live credential operations.
//!
//! Both operations reuse the cross-target credential driver. The resolved
//! transport is `FetchClient` on wasm, so token refresh, request preparation,
//! and channel-specific response parsing are identical to native.

use bytes::Bytes;
use http::Method;
use http::request::Parts;
use serde::Deserialize;

use crate::admin::guard::guard_admin;
use crate::api::error::ApiError;
use crate::app::AppState;
use crate::credentials::usage::UsageError;

use super::{Resp, json_body, parse_i64, segments};

#[derive(Debug, Clone, Deserialize)]
struct RateLimitResetCreditBody {
    idempotency_key: String,
}

/// Route live credential reads/actions that contact the credential's upstream.
pub(super) async fn dispatch(
    state: &AppState,
    parts: &Parts,
    body: &Bytes,
) -> Option<Result<Resp, ApiError>> {
    let segs = segments(parts);
    let r = match (&parts.method, segs.as_slice()) {
        (&Method::GET, ["admin", "credentials", id, "usage"]) => {
            credential_usage(state, parts, id).await
        }
        (&Method::POST, ["admin", "credentials", id, "rate-limit-reset-credit"]) => {
            consume_rate_limit_reset_credit(state, parts, id, body).await
        }
        _ => return None,
    };
    Some(r)
}

async fn credential_usage(state: &AppState, parts: &Parts, id: &str) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let id = parse_i64(id)?;
    let usage = crate::credentials::usage::fetch_usage(state, id)
        .await
        .map_err(usage_error)?;
    Resp::json(200, &usage)
}

async fn consume_rate_limit_reset_credit(
    state: &AppState,
    parts: &Parts,
    id: &str,
    body: &Bytes,
) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let id = parse_i64(id)?;
    let body: RateLimitResetCreditBody = json_body(body)?;
    let result = crate::credentials::usage::consume_rate_limit_reset_credit(
        state,
        id,
        &body.idempotency_key,
    )
    .await
    .map_err(usage_error)?;
    Resp::json(200, &result)
}

fn usage_error(error: UsageError) -> ApiError {
    match error {
        UsageError::CredentialNotFound
        | UsageError::ProviderNotFound
        | UsageError::UnknownChannel(_) => ApiError::NotFound(error.to_string()),
        UsageError::Unsupported | UsageError::Channel(_) => ApiError::BadRequest(error.to_string()),
        UsageError::Decrypt(_) | UsageError::Upstream(_) | UsageError::Status(_) => {
            ApiError::Internal(error.to_string())
        }
    }
}
