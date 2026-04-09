use axum::Router;
use axum::extract::Path;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "web/console"]
pub struct ConsoleAssets;

pub fn router() -> Router {
    Router::new()
        .route("/console", get(console_index))
        .route("/console/", get(console_index))
        .route("/console/{*path}", get(console_path))
}

pub async fn console_index() -> Response {
    render("index.html")
}

pub async fn console_path(Path(path): Path<String>) -> Response {
    let trimmed = path.trim_matches('/');
    if trimmed.is_empty() {
        return render("index.html");
    }
    if ConsoleAssets::get(trimmed).is_some() {
        return render(trimmed);
    }
    if trimmed
        .rsplit('/')
        .next()
        .is_some_and(|segment| segment.contains('.'))
    {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }
    render("index.html")
}

fn render(path: &str) -> Response {
    let Some(content) = ConsoleAssets::get(path) else {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    };

    let mime = mime_guess::from_path(path)
        .first_raw()
        .unwrap_or("application/octet-stream");

    let mut response = Response::new(axum::body::Body::from(content.data.into_owned()));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(mime)
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );

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
