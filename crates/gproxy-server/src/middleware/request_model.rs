use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;

pub use gproxy_engine::routing::model_extraction::extract_model;

use crate::middleware::classify::{BufferedBodyBytes, Classification};

/// Extracted model stored in request extensions.
#[derive(Debug, Clone)]
pub struct ExtractedModel(pub Option<String>);

/// Axum middleware: extract the model name from the request body or URI path
/// and store `ExtractedModel` in extensions.
///
/// Requires `Classification` and `BufferedBodyBytes` to already be in extensions
/// (run after classify_middleware).
pub async fn request_model_middleware(request: Request, next: Next) -> Response {
    let classification = request.extensions().get::<Classification>().cloned();
    let body_bytes = request.extensions().get::<BufferedBodyBytes>().cloned();
    let model = classification.as_ref().and_then(|classification| {
        extract_model(
            request.uri().path(),
            body_bytes.as_ref().map(|bytes| bytes.0.as_ref()),
            classification.operation,
            classification.protocol,
        )
    });
    let mut request = request;
    request.extensions_mut().insert(ExtractedModel(model));
    next.run(request).await
}
