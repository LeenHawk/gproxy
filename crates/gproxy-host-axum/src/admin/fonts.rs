//! Font installation is an explicit, instance-administrator operation.
use axum::{
    Extension, Router,
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
        "/fonts",
        get(status::<C>).post(download::<C>).delete(remove::<C>),
    )
}

async fn status<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
) -> Response {
    if let Err(error) = require_section(&scope, "configuration.settings") {
        return ErrorResponse(error).into_response();
    }
    crate::error::ok_json(&state.console().fonts().status().await)
}

async fn download<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
) -> Response {
    if let Err(error) = require_section(&scope, "configuration.settings") {
        return ErrorResponse(error).into_response();
    }
    result(state.console().download_fonts().await)
}

async fn remove<C>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
) -> Response {
    if let Err(error) = require_section(&scope, "configuration.settings") {
        return ErrorResponse(error).into_response();
    }
    result(state.console().fonts().remove().await)
}

fn result(value: std::io::Result<crate::console::fonts::FontStatus>) -> Response {
    match value {
        Ok(status) => crate::error::ok_json(&status),
        Err(error) => {
            ErrorResponse(gproxy_app::AppError::invalid(error.to_string())).into_response()
        }
    }
}
