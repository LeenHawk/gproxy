use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, Method, Uri};
use axum::response::Response;
use bytes::Bytes;
use gproxy_provider_core::{CallContext, ProxyResponse, UpstreamPassthroughError};
use http::header::CONTENT_TYPE;

use crate::auth::AuthError;
use crate::classify::classify_request;
use crate::core::CoreState;
use crate::error::ProxyError;

pub async fn proxy_handler(
    State(state): State<Arc<CoreState>>,
    Path((provider, path)): Path<(String, String)>,
    method: Method,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Response {
    let Some(provider_handle) = (state.lookup)(provider.as_str()) else {
        return error_response(ProxyError::not_found("unknown provider"));
    };

    let auth_ctx = match state.auth.authenticate(&headers) {
        Ok(ctx) => ctx,
        Err(err) => return auth_error_response(err),
    };

    let classified = match classify_request(
        &method,
        &path,
        uri.query(),
        &headers,
        body,
    ) {
        Ok(req) => req,
        Err(err) => return error_response(err),
    };

    let ctx = CallContext {
        request_id: request_id(&headers),
        user_id: auth_ctx.user_id,
        user_key_id: auth_ctx.key_id,
        proxy: state.proxy.read().ok().and_then(|guard| guard.clone()),
    };

    match provider_handle.call(classified.request, ctx).await {
        Ok(response) => proxy_response(response),
        Err(err) => passthrough_error(err),
    }
}

fn proxy_response(response: ProxyResponse) -> Response {
    match response {
        ProxyResponse::Json {
            status,
            headers,
            body,
        } => {
            let mut resp = Response::new(Body::from(body));
            *resp.status_mut() = status;
            resp.headers_mut().extend(headers);
            resp
        }
        ProxyResponse::Stream {
            status,
            headers,
            body,
        } => {
            let mut resp = Response::new(Body::from_stream(body.stream));
            *resp.status_mut() = status;
            resp.headers_mut().extend(headers);
            if !resp.headers().contains_key(CONTENT_TYPE) {
                resp.headers_mut()
                    .insert(CONTENT_TYPE, HeaderValue::from_static(body.content_type));
            }
            resp
        }
    }
}

fn passthrough_error(err: UpstreamPassthroughError) -> Response {
    let mut resp = Response::new(Body::from(err.body));
    *resp.status_mut() = err.status;
    resp.headers_mut().extend(err.headers);
    resp
}

fn error_response(err: ProxyError) -> Response {
    let mut resp = Response::new(Body::from(err.body));
    *resp.status_mut() = err.status;
    resp
}

fn auth_error_response(err: AuthError) -> Response {
    let mut resp = Response::new(Body::from(err.body));
    *resp.status_mut() = err.status;
    resp.headers_mut().extend(err.headers);
    resp
}

fn request_id(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-request-id")
        .or_else(|| headers.get("request-id"))
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string())
}
