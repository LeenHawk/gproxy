//! An administrator's own console bundle: upload, inspect, remove.
//!
//! `POST` takes the zip as the raw request body (`application/zip`), bounded
//! by the host's body limit. The built-in console remains reachable behind
//! `/console/?console=builtin` whatever is uploaded here.
use axum::{
    Extension, Router,
    body::Bytes,
    extract::State,
    response::{IntoResponse, Response},
    routing::get,
};
use gproxy_app::{AdminScope, require_section};
use gproxy_seaorm::BatchConnectionTrait;

use crate::{HostState, error::ErrorResponse};

pub(super) fn routes<C>() -> Router<HostState<C>>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    Router::new().route(
        "/console-bundle",
        get(status::<C>).post(install::<C>).delete(remove::<C>),
    )
}

async fn status<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
) -> Response {
    if let Err(error) = require_section(&scope, "configuration.settings") {
        return ErrorResponse(error).into_response();
    }
    crate::error::ok_json(&state.console().custom_status().await)
}

async fn install<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    body: Bytes,
) -> Response {
    if let Err(error) = require_section(&scope, "configuration.settings") {
        return ErrorResponse(error).into_response();
    }
    let result = state.console().custom().install(body).await;
    respond(&state, result).await
}

async fn remove<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
) -> Response {
    if let Err(error) = require_section(&scope, "configuration.settings") {
        return ErrorResponse(error).into_response();
    }
    let result = state.console().custom().remove().await;
    respond(&state, result).await
}

async fn respond<C>(state: &HostState<C>, result: std::io::Result<()>) -> Response {
    match result {
        Ok(()) => crate::error::ok_json(&state.console().custom_status().await),
        Err(error) => {
            ErrorResponse(gproxy_app::AppError::invalid(error.to_string())).into_response()
        }
    }
}
