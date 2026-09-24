//! HTTP bindings for scope-aware upstream account login and credential probes.
use super::reply;
use crate::{HostState, error::ErrorResponse};
use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use gproxy_app::{AdminScope, Caller, CredentialLogin, ScopedManage};
use gproxy_sdk::dto::{AuthCodeComplete, AuthCodeStart, CookieExchange, DeviceStart};
use gproxy_seaorm::BatchConnectionTrait;
use serde::Deserialize;

pub(super) fn routes<C: BatchConnectionTrait + Send + Sync + 'static>() -> Router<HostState<C>> {
    Router::new()
        .route("/credentials/providers", get(providers::<C>))
        .route("/credentials/owners", get(owners::<C>))
        .route("/credentials/{id}/models/discover", post(discover::<C>))
        .route("/credentials/{id}/models/test", post(test::<C>))
        .route(
            "/credential-login/authcode/start",
            post(authcode_start::<C>),
        )
        .route(
            "/credential-login/authcode/complete",
            post(authcode_complete::<C>),
        )
        .route("/credential-login/device/start", post(device_start::<C>))
        .route("/credential-login/device/poll", post(device_poll::<C>))
        .route(
            "/credential-login/cookie/exchange",
            post(cookie_exchange::<C>),
        )
}

async fn providers<C: BatchConnectionTrait + Send + Sync + 'static>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
) -> Response {
    crate::send(async move {
        gate!("credentials", scope);
        let data = state.app().data();
        let scoped = ScopedManage::new(state.app().gproxy(), &data, &scope);
        reply(scoped.credentials().providers().await)
    })
    .await
}

async fn owners<C: BatchConnectionTrait + Send + Sync + 'static>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
) -> Response {
    crate::send(async move {
        gate!("credentials", scope);
        let data = state.app().data();
        crate::error::ok_json(
            &ScopedManage::new(state.app().gproxy(), &data, &scope)
                .credentials()
                .owners(),
        )
    })
    .await
}

async fn discover<C: BatchConnectionTrait + Send + Sync + 'static>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Path(id): Path<String>,
) -> Response {
    crate::send(async move {
        scoped!(
            state,
            scope,
            "credentials",
            credentials.discover_models(&id)
        )
    })
    .await
}

#[derive(Deserialize)]
struct Model {
    model: String,
}
async fn test<C: BatchConnectionTrait + Send + Sync + 'static>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Path(id): Path<String>,
    Json(body): Json<Model>,
) -> Response {
    crate::send(async move {
        scoped!(
            state,
            scope,
            "credentials",
            credentials.model_test(&id, body.model)
        )
    })
    .await
}

macro_rules! login_handler {
    ($handler:ident, $request:ty, $method:ident) => {
        async fn $handler<C: BatchConnectionTrait + Send + Sync + 'static>(
            State(state): State<HostState<C>>,
            Extension(scope): Extension<AdminScope>,
            Extension(caller): Extension<Caller>,
            Json(request): Json<$request>,
        ) -> Response {
            crate::send(async move {
                gate!("credentials", scope);
                let data = state.app().data();
                let login = CredentialLogin::new(state.app().gproxy(), &data, &scope, &caller);
                reply(login.$method(request).await)
            })
            .await
        }
    };
}
login_handler!(authcode_start, AuthCodeStart, authcode_start);
login_handler!(authcode_complete, AuthCodeComplete, authcode_complete);
login_handler!(device_start, DeviceStart, device_start);
login_handler!(cookie_exchange, CookieExchange, cookie_exchange);

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Poll {
    login_session_id: String,
}
async fn device_poll<C: BatchConnectionTrait + Send + Sync + 'static>(
    State(state): State<HostState<C>>,
    Extension(scope): Extension<AdminScope>,
    Extension(caller): Extension<Caller>,
    Json(body): Json<Poll>,
) -> Response {
    crate::send(async move {
        gate!("credentials", scope);
        let data = state.app().data();
        reply(
            CredentialLogin::new(state.app().gproxy(), &data, &scope, &caller)
                .device_poll(&body.login_session_id)
                .await,
        )
    })
    .await
}
