use bytes::Bytes;
use http::request::Parts;
use http::{Method, Response};

use crate::{AdminError, State, auth, handlers, response, route};

pub async fn dispatch(state: &impl State, parts: &Parts, body: Bytes) -> Option<Response<Bytes>> {
    let path = parts.uri.path();
    if path != "/admin/api" && !path.starts_with("/admin/api/") {
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
        let audit = route::audit(&route, &body);
        let response = handlers::dispatch(state, &admin, route, parts, &body).await?;
        if response.status().is_success()
            && let Some(mut event) = audit
        {
            if event.target_id.is_none() && event.action.ends_with(".create") {
                event.target_id = serde_json::from_slice::<serde_json::Value>(response.body())
                    .ok()
                    .and_then(|value| value.get("id")?.as_i64());
            }
            handlers::audit::record(state, admin.id, auth::source_ip(parts), event).await?;
        }
        Ok(response)
    }
    .await;
    Some(response::render(result, "admin"))
}
