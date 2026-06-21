//! Console SPA serving (B1): rust-embed'd `assets/console` under `/console`,
//! with SPA fallback for extension-less paths. Native-only.

use axum::Router;
use axum::extract::Path;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::get;
use rust_embed::RustEmbed;

use crate::app::AppState;

#[derive(RustEmbed)]
#[folder = "assets/console"]
#[exclude = ".*"]
struct ConsoleAssets;

pub fn router() -> Router<AppState> {
    Router::new()
        // Bare hostname bounces straight into the SPA.
        .route("/", get(|| async { Redirect::permanent("/console") }))
        .route("/console", get(console_index))
        .route("/console/", get(console_index))
        .route("/console/{*path}", get(console_path))
}

async fn console_index() -> Response {
    if ConsoleAssets::get("index.html").is_none() {
        // Binary was built without frontend artifacts (console/ not built).
        return (
            StatusCode::NOT_FOUND,
            "console assets not embedded — run `pnpm build` in console/ and rebuild",
        )
            .into_response();
    }
    render("index.html")
}

async fn console_path(Path(path): Path<String>) -> Response {
    let trimmed = path.trim_matches('/');
    if trimmed.is_empty() {
        return console_index().await;
    }
    if ConsoleAssets::get(trimmed).is_some() {
        return render(trimmed);
    }
    // Dotted final segment = a real (missing) file; anything else is an SPA route.
    if trimmed
        .rsplit('/')
        .next()
        .is_some_and(|segment| segment.contains('.'))
    {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }
    console_index().await
}

fn render(path: &str) -> Response {
    let Some(content) = ConsoleAssets::get(path) else {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    };

    let mime = mime_guess::from_path(path)
        .first_raw()
        .unwrap_or("application/octet-stream");

    let mut response = Response::new(axum::body::Body::from(content.data));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(mime)
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );

    // Hashed assets are immutable; index.html must always revalidate.
    let cache_control = if path == "index.html" {
        HeaderValue::from_static("no-cache")
    } else if path.starts_with("assets/") {
        HeaderValue::from_static("public, max-age=31536000, immutable")
    } else {
        HeaderValue::from_static("public, max-age=3600")
    };
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, cache_control);
    response
}
