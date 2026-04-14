use axum::body::Body;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use http_body_util::BodyExt;

pub use gproxy_engine::routing::classify::{
    Classification, classify_route, extract_model_from_uri_path, normalize_path,
};

/// Buffered request body bytes stored in extensions by `classify_middleware`.
/// This allows downstream middleware to read the body without consuming it.
#[derive(Debug, Clone)]
pub struct BufferedBodyBytes(pub Bytes);

/// Axum middleware that buffers the request body and stores route classification in extensions.
pub async fn classify_middleware(request: Request, next: Next) -> Response {
    let (parts, body) = request.into_parts();

    let body_bytes = match body.collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(_) => {
            return (StatusCode::BAD_REQUEST, "failed to read request body").into_response();
        }
    };

    let path_and_query = parts
        .uri
        .path_and_query()
        .map(|value| value.as_str())
        .unwrap_or_else(|| parts.uri.path());

    match classify_route(
        &parts.method,
        path_and_query,
        &parts.headers,
        Some(body_bytes.as_ref()),
    ) {
        Ok(classification) => {
            let mut request = Request::from_parts(parts, Body::from(body_bytes.clone()));
            request
                .extensions_mut()
                .insert(BufferedBodyBytes(body_bytes));
            request.extensions_mut().insert(classification);
            next.run(request).await
        }
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}
