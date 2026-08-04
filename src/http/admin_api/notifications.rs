//! Signed maintainer announcements with explicit edge degradation.

use http::Method;

use crate::admin::guard::guard_admin;
use crate::api::error::ApiError;
use crate::app::AppState;

use super::{Request, Resp, segments};

pub(super) async fn dispatch(
    state: &AppState,
    request: &Request,
) -> Option<Result<Resp, ApiError>> {
    if !matches!(
        (&request.method, segments(request).as_slice()),
        (&Method::GET, ["admin", "notifications"])
    ) {
        return None;
    }
    Some(get(state, request).await)
}

async fn get(state: &AppState, request: &Request) -> Result<Resp, ApiError> {
    guard_admin(state, request).await?;
    #[cfg(not(target_arch = "wasm32"))]
    let notifications = crate::announce::list(state).await;
    #[cfg(target_arch = "wasm32")]
    let notifications: Vec<serde_json::Value> = Vec::new();
    Resp::json(200, &serde_json::json!({ "notifications": notifications }))
}
