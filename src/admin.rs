use std::sync::Arc;

use crate::config::{AppConfig, AppSection};
use crate::context::AppContext;
use axum::Router;
use axum::extract::{Extension, Json};
use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};

mod providers;
mod usage;
mod email;

pub fn router(ctx: Arc<AppContext>) -> Router {
    let mut router = Router::new()
        .route("/config", get(get_admin_config).put(put_admin_config))
        .route("/config/export", get(export_admin_config))
        .route("/config/import", post(import_admin_config))
        .nest("/usage", usage::router())
        .nest("/providers", providers::router());
    #[cfg(feature = "provider-geminicli")]
    {
        router = router.route("/geminicli/fetch-email", post(email::fetch_geminicli_email));
    }
    #[cfg(feature = "provider-antigravity")]
    {
        router = router.route("/antigravity/fetch-email", post(email::fetch_antigravity_email));
    }
    router.layer(Extension(ctx))
}

pub(crate) fn ensure_admin(headers: &HeaderMap, ctx: &AppContext) -> Result<(), StatusCode> {
    let key = headers
        .get("x-admin-key")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if key.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    if key != ctx.get_config().app.admin_key {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(())
}

pub(crate) async fn get_admin_config(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
) -> Result<Json<AppSection>, StatusCode> {
    ensure_admin(&headers, &ctx)?;
    Ok(Json(ctx.get_config().app.clone()))
}

pub(crate) async fn put_admin_config(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    Json(app): Json<AppSection>,
) -> Result<Json<AppSection>, StatusCode> {
    ensure_admin(&headers, &ctx)?;
    let before = ctx.get_config().app.clone();
    let app = ctx
        .update_config(|config| {
            config.app = app;
            config.app.clone()
        })
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if before.host != app.host || before.port != app.port {
        let reload_tx = ctx.reload_tx();
        let next = (*reload_tx.borrow()).wrapping_add(1);
        let _ = reload_tx.send(next);
    }
    Ok(Json(app))
}

pub(crate) async fn export_admin_config(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
) -> Result<(HeaderMap, String), StatusCode> {
    ensure_admin(&headers, &ctx)?;
    let config = ctx.get_config();
    let contents =
        toml::to_string(config.as_ref()).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut response_headers = HeaderMap::new();
    response_headers.insert(
        CONTENT_TYPE,
        "application/toml; charset=utf-8".parse().unwrap(),
    );
    Ok((response_headers, contents))
}

pub(crate) async fn import_admin_config(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    body: String,
) -> Result<Json<AppConfig>, StatusCode> {
    ensure_admin(&headers, &ctx)?;
    let before = ctx.get_config();
    let imported: AppConfig = toml::from_str(&body).map_err(|_| StatusCode::BAD_REQUEST)?;
    let updated = ctx
        .update_config(|config| {
            *config = imported;
            config.clone()
        })
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if before.app.host != updated.app.host || before.app.port != updated.app.port {
        let reload_tx = ctx.reload_tx();
        let next = (*reload_tx.borrow()).wrapping_add(1);
        let _ = reload_tx.send(next);
    }
    Ok(Json(updated))
}
