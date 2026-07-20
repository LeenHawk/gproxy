//! Shared `POST /admin/batch/{entity}` dispatcher.
use bytes::Bytes;
use http::Method;

use crate::admin::guard::guard_admin;
use crate::admin::invalidate;
use crate::api::batch::{BatchRequest, run_batch};
use crate::api::error::ApiError;
use crate::app::AppState;
use crate::store::persistence::batch::AdminEntity;

use super::{Request, Resp, json_body, segments};

pub(super) async fn dispatch(
    state: &AppState,
    parts: &Request,
    body: &Bytes,
) -> Option<Result<Resp, ApiError>> {
    let segs = segments(parts);
    if let (&Method::POST, ["admin", "batch", entity]) = (&parts.method, segs.as_slice()) {
        let entity = *entity;
        return Some(
            async {
                guard_admin(state, parts).await?;
                let entity = AdminEntity::from_seg(entity)
                    .ok_or_else(|| ApiError::NotFound("unknown entity".into()))?;
                let req: BatchRequest = json_body(body)?;
                let outcome = run_batch(state, entity, req).await?;
                invalidate(state).await;
                Resp::json(200, &outcome)
            }
            .await,
        );
    }
    None
}
