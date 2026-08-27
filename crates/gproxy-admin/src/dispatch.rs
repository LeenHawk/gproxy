use bytes::Bytes;
use http::request::Parts;
use http::{Method, Response};

use crate::{AdminError, State, auth, handlers, response, route};

pub async fn dispatch(state: &impl State, parts: &Parts, body: Bytes) -> Option<Response<Bytes>> {
    let path = parts.uri.path();
    if path != "/admin" && !path.starts_with("/admin/") {
        return None;
    }
    if matches!(path, "/admin" | "/admin/") {
        return None;
    }
    if let Some(result) = auth::dispatch_public(state, parts, &body).await {
        return Some(response::render(result, "admin"));
    }
    let Some(route) = route::parse(&parts.method, path) else {
        return Some(response::error(&AdminError::NotFound));
    };
    let result = async {
        let admin = auth::authenticate(state, parts).await?;
        if !admin.api_key && parts.method != Method::GET && parts.method != Method::HEAD {
            auth::verify_same_origin(parts)?;
        }
        handlers::dispatch(state, &admin, route, parts, &body).await
    }
    .await;
    Some(response::render(result, "admin"))
}
