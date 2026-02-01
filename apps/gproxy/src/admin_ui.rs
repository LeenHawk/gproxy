use axum::extract::OriginalUri;
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "frontend/dist"]
struct AdminUi;

pub async fn ui_fallback(uri: OriginalUri) -> Response {
    let mut path = uri.0.path().trim_start_matches('/');
    if path.is_empty() {
        path = "index.html";
    }
    match AdminUi::get(path).or_else(|| AdminUi::get("index.html")) {
        Some(content) => {
            let body = axum::body::Body::from(content.data);
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            let mut response = Response::new(body);
            response.headers_mut().insert(
                axum::http::header::CONTENT_TYPE,
                HeaderValue::from_str(mime.as_ref()).unwrap_or_else(|_| {
                    HeaderValue::from_static("application/octet-stream")
                }),
            );
            response
        }
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}
