//! Edge-safe live credential operations.
//!
//! Every route here names one concrete stored credential. These account-level
//! operations intentionally never appear on the public aggregate gateway.

use bytes::Bytes;
use http::Method;
use serde::Deserialize;

use crate::admin::guard::guard_admin;
use crate::api::error::ApiError;
use crate::app::AppState;
use crate::channel::{CredentialControlOperation, CredentialControlResponse};
use crate::credentials::control::ControlError;
use crate::credentials::usage::UsageError;

use super::{Request, Resp, json_body, parse_i64, segments};

#[derive(Debug, Clone, Deserialize)]
struct RateLimitResetCreditBody {
    idempotency_key: String,
}

pub(super) async fn dispatch(
    state: &AppState,
    parts: &Request,
    body: &Bytes,
) -> Option<Result<Resp, ApiError>> {
    let segs = segments(parts);
    let result = match (&parts.method, segs.as_slice()) {
        (&Method::GET, ["admin", "credentials", id, "usage"]) => {
            credential_usage(state, parts, id).await
        }
        (&Method::GET, ["admin", "credentials", id, "rate-limit-reset-credits"]) => {
            credential_json(
                state,
                parts,
                id,
                CredentialControlOperation::ListRateLimitResetCredits,
            )
            .await
        }
        (
            &Method::POST,
            [
                "admin",
                "credentials",
                id,
                "rate-limit-reset-credits",
                "consume",
            ],
        )
        | (&Method::POST, ["admin", "credentials", id, "rate-limit-reset-credit"]) => {
            consume_rate_limit_reset_credit(state, parts, id, body).await
        }
        (&Method::GET, ["admin", "credentials", id, "account"]) => {
            credential_json(state, parts, id, CredentialControlOperation::Account).await
        }
        (&Method::GET, ["admin", "credentials", id, "profile"]) => {
            credential_json(state, parts, id, CredentialControlOperation::Profile).await
        }
        (&Method::GET, ["admin", "credentials", id, "settings"]) => {
            credential_json(state, parts, id, CredentialControlOperation::Settings).await
        }
        (&Method::GET, ["admin", "credentials", id, "tasks"]) => {
            credential_json(
                state,
                parts,
                id,
                CredentialControlOperation::ListTasks {
                    query: parts.uri.query().map(str::to_owned),
                },
            )
            .await
        }
        (&Method::POST, ["admin", "credentials", id, "tasks"]) => {
            create_task(state, parts, id, body).await
        }
        (&Method::GET, ["admin", "credentials", id, "tasks", task_id]) => {
            credential_json(
                state,
                parts,
                id,
                CredentialControlOperation::GetTask {
                    task_id: (*task_id).to_owned(),
                },
            )
            .await
        }
        (
            &Method::GET,
            [
                "admin",
                "credentials",
                id,
                "tasks",
                task_id,
                "turns",
                turn_id,
                "siblings",
            ],
        ) => {
            credential_json(
                state,
                parts,
                id,
                CredentialControlOperation::ListSiblingTurns {
                    task_id: (*task_id).to_owned(),
                    turn_id: (*turn_id).to_owned(),
                },
            )
            .await
        }
        _ => return None,
    };
    Some(result)
}

async fn credential_usage(state: &AppState, parts: &Request, id: &str) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let id = parse_i64(id)?;
    let usage = crate::credentials::usage::fetch_usage(state, id)
        .await
        .map_err(usage_error)?;
    Resp::json(200, &usage)
}

async fn credential_json(
    state: &AppState,
    parts: &Request,
    id: &str,
    operation: CredentialControlOperation,
) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let id = parse_i64(id)?;
    match crate::credentials::control::execute(state, id, operation)
        .await
        .map_err(control_error)?
    {
        CredentialControlResponse::Json(value) => Resp::json(200, &value),
        _ => Err(ApiError::Internal(
            "unexpected credential control response".into(),
        )),
    }
}

async fn create_task(
    state: &AppState,
    parts: &Request,
    id: &str,
    body: &Bytes,
) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let id = parse_i64(id)?;
    let body: serde_json::Value = json_body(body)?;
    match crate::credentials::control::execute(
        state,
        id,
        CredentialControlOperation::CreateTask { body },
    )
    .await
    .map_err(control_error)?
    {
        CredentialControlResponse::Json(value) => Resp::json(200, &value),
        _ => Err(ApiError::Internal(
            "unexpected credential control response".into(),
        )),
    }
}

async fn consume_rate_limit_reset_credit(
    state: &AppState,
    parts: &Request,
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

fn control_error(error: ControlError) -> ApiError {
    match error {
        ControlError::CredentialNotFound
        | ControlError::ProviderNotFound
        | ControlError::UnknownChannel(_) => ApiError::NotFound(error.to_string()),
        ControlError::Unsupported(_) | ControlError::Channel(_) => {
            ApiError::BadRequest(error.to_string())
        }
        ControlError::Decrypt(_) | ControlError::Upstream(_) | ControlError::Status(_) => {
            ApiError::Internal(error.to_string())
        }
    }
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
