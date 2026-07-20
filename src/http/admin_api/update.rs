//! Native self-update routes with explicit edge degradation.

use bytes::Bytes;
use http::Method;

use crate::admin::guard::guard_admin;
use crate::api::error::ApiError;
use crate::app::AppState;

use super::{Request, Resp, segments};

pub(super) async fn dispatch(
    state: &AppState,
    request: &Request,
    body: &Bytes,
) -> Option<Result<Resp, ApiError>> {
    let result = match (&request.method, segments(request).as_slice()) {
        (&Method::GET, ["admin", "update", "check"]) => check(state, request).await,
        (&Method::GET, ["admin", "update", "status"]) => status(state, request).await,
        (&Method::POST, ["admin", "update", "apply"]) => apply(state, request, body).await,
        _ => return None,
    };
    Some(result)
}

async fn check(state: &AppState, request: &Request) -> Result<Resp, ApiError> {
    guard_admin(state, request).await?;
    #[cfg(not(target_arch = "wasm32"))]
    {
        let context = native::context(state)?;
        let report = crate::selfupdate::check(&context)
            .await
            .map_err(native::update_error)?;
        Resp::json(200, &report)
    }
    #[cfg(target_arch = "wasm32")]
    edge_unavailable()
}

async fn status(state: &AppState, request: &Request) -> Result<Resp, ApiError> {
    guard_admin(state, request).await?;
    #[cfg(not(target_arch = "wasm32"))]
    {
        let value = state
            .update_status
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        Resp::json(200, &value)
    }
    #[cfg(target_arch = "wasm32")]
    edge_unavailable()
}

async fn apply(state: &AppState, request: &Request, body: &Bytes) -> Result<Resp, ApiError> {
    guard_admin(state, request).await?;
    #[cfg(not(target_arch = "wasm32"))]
    {
        native::apply(state, body).await
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = body;
        edge_unavailable()
    }
}

#[cfg(target_arch = "wasm32")]
fn edge_unavailable() -> Result<Resp, ApiError> {
    Err(ApiError::NotImplemented(
        "self-update is unavailable on edge".into(),
    ))
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::time::Duration;

    use serde::Deserialize;

    use crate::app::update_status::UpdateStatus;
    use crate::selfupdate::{
        self, ApplyOptions, Channel, DEFAULT_REPO, Restart, UpdateContext, UpdateError,
    };

    use super::*;

    const RESTART_DELAY: Duration = Duration::from_millis(750);

    #[derive(Debug, Default, Deserialize)]
    struct ApplyRequest {
        #[serde(default)]
        allow_insecure: bool,
    }

    pub(super) fn context(state: &AppState) -> Result<UpdateContext, ApiError> {
        let channel_name = state
            .cp()
            .update_channel
            .clone()
            .unwrap_or_else(|| state.config.update_channel.clone());
        let channel = if channel_name == "staging" {
            Channel::Staging
        } else {
            Channel::Releases
        };
        let client = state
            .upstream_client_for_default_proxy()
            .map_err(|error| ApiError::BadRequest(format!("update client init failed: {error}")))?;
        Ok(UpdateContext {
            repo: DEFAULT_REPO.into(),
            channel,
            data_dir: state.config.update_data_dir.clone(),
            client,
        })
    }

    pub(super) fn update_error(error: UpdateError) -> ApiError {
        let message = error.to_string();
        match error {
            UpdateError::NoArtifact(_) | UpdateError::Version(_) => ApiError::BadRequest(message),
            UpdateError::ConfirmationRequired(_) => ApiError::ConfirmationRequired(message),
            UpdateError::ManifestNotFound => ApiError::NotFound(message),
            UpdateError::Incompatible(_) | UpdateError::Downgrade(_) => ApiError::Conflict(message),
            UpdateError::Integrity(_) | UpdateError::Signature(_) => ApiError::Conflict(message),
            UpdateError::Manifest(_)
            | UpdateError::Download(_)
            | UpdateError::Io(_)
            | UpdateError::Swap(_) => ApiError::Internal(message),
        }
    }

    pub(super) async fn apply(state: &AppState, body: &Bytes) -> Result<Resp, ApiError> {
        let context = context(state)?;
        let request = parse_request(body)?;
        {
            let mut status = state
                .update_status
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            if matches!(
                *status,
                UpdateStatus::Checking
                    | UpdateStatus::Downloading
                    | UpdateStatus::Restarting { .. }
            ) {
                return Err(ApiError::Conflict(
                    "an update is already in progress".into(),
                ));
            }
            *status = UpdateStatus::Downloading;
        }

        let result = async {
            let report = selfupdate::check(&context).await?;
            if !report.available {
                return Ok(None);
            }
            selfupdate::apply_with_options(
                &context,
                ApplyOptions {
                    restart: Restart::None,
                    allow_insecure: request.allow_insecure,
                },
            )
            .await
            .map(Some)
        }
        .await;

        let (terminal, outcome) = match result {
            Ok(Some(version)) => {
                let status = UpdateStatus::Restarting {
                    version: version.clone(),
                };
                schedule_restart(version);
                (status.clone(), Resp::json(200, &status))
            }
            Ok(None) => (UpdateStatus::Idle, Resp::json(200, &UpdateStatus::Idle)),
            Err(error @ UpdateError::ConfirmationRequired(_)) => {
                (UpdateStatus::Idle, Err(update_error(error)))
            }
            Err(error) => {
                let status = UpdateStatus::Failed {
                    error: error.to_string(),
                };
                (status, Err(update_error(error)))
            }
        };
        *state
            .update_status
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = terminal;
        outcome
    }

    fn parse_request(body: &[u8]) -> Result<ApplyRequest, ApiError> {
        if body.is_empty() {
            return Ok(ApplyRequest::default());
        }
        serde_json::from_slice(body)
            .map_err(|error| ApiError::BadRequest(format!("invalid JSON body: {error}")))
    }

    fn schedule_restart(version: String) {
        std::thread::spawn(move || {
            std::thread::sleep(RESTART_DELAY);
            tracing::info!(version, "update applied; completing install hand-off");
            selfupdate::restart(Restart::ReExec);
        });
    }
}
