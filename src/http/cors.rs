//! CORS policy shared by native HTTP routers.

use ::http::HeaderValue;

/// Parse explicit `scheme://host[:port]` origins, rejecting wildcard and
/// scheme-less entries. Credentialed CORS cannot use `*`, and a browser Origin
/// header is always a full origin rather than a bare host.
fn parsed_allowed_origins(cors_origins: &[String]) -> Vec<HeaderValue> {
    cors_origins
        .iter()
        .filter_map(|o| {
            let o = o.trim();
            if o == "*" || !o.contains("://") {
                tracing::warn!(
                    origin = %o,
                    "CORS origin must be an explicit scheme://host[:port] (not '*') — skipped"
                );
                return None;
            }
            o.parse::<HeaderValue>()
                .map_err(
                    |e| tracing::warn!(origin = %o, error = %e, "invalid CORS origin — skipped"),
                )
                .ok()
        })
        .collect()
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn credentialed_admin_layer(cors_origins: &[String]) -> tower_http::cors::CorsLayer {
    use ::http::Method;
    use ::http::header::{AUTHORIZATION, CONTENT_TYPE};
    use tower_http::cors::{AllowOrigin, CorsLayer};

    let x_api_key: ::http::HeaderName = "x-api-key".parse().unwrap();
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(parsed_allowed_origins(cors_origins)))
        .allow_credentials(true)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([CONTENT_TYPE, AUTHORIZATION, x_api_key])
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn credentialed_gateway_layer(cors_origins: &[String]) -> tower_http::cors::CorsLayer {
    use ::http::Method;
    use tower_http::cors::{AllowHeaders, AllowOrigin, CorsLayer};

    CorsLayer::new()
        .allow_origin(AllowOrigin::list(parsed_allowed_origins(cors_origins)))
        .allow_credentials(true)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers(AllowHeaders::mirror_request())
}
