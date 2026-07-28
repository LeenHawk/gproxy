//! Runtime channel catalog (`GET /admin/channels`).

use http::Method;

use crate::admin::guard::guard_admin;
use crate::api::error::ApiError;
use crate::app::AppState;

use super::{Request, Resp, segments};

pub(super) async fn dispatch(
    state: &AppState,
    request: &Request,
) -> Option<Result<Resp, ApiError>> {
    match (&request.method, segments(request).as_slice()) {
        (&Method::GET, ["admin", "channels"]) => Some(list_registered(state, request).await),
        _ => None,
    }
}

async fn list_registered(state: &AppState, request: &Request) -> Result<Resp, ApiError> {
    guard_admin(state, request).await?;
    Resp::json(200, &state.channels.catalog())
}
