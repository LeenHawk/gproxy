//! Instance settings handler for the edge admin dispatcher.
//!
//! Routes:
//!   `GET  /admin/instance-settings` — list all instance settings
//!   `POST /admin/instance-settings` — upsert (no per-id get or delete)

use bytes::Bytes;
use http::Method;
use http::request::Parts;

use crate::admin::guard::guard_admin;
use crate::admin::invalidate;
use crate::api::error::ApiError;
use crate::app::AppState;
use crate::store::persistence::records::{InstanceSettings, InstanceSettingsInput};

use super::{Resp, internal, json_body, segments};

/// Handle `GET /admin/instance-settings` and `POST /admin/instance-settings`.
///
/// Returns `Some(result)` when the path matches; `None` to fall through.
pub(super) async fn dispatch(
    state: &AppState,
    parts: &Parts,
    body: &Bytes,
) -> Option<Result<Resp, ApiError>> {
    let segs = segments(parts);
    match (&parts.method, segs.as_slice()) {
        (&Method::GET, ["admin", "instance-settings"]) => Some(
            async {
                guard_admin(state, parts).await?;
                let recs: Vec<InstanceSettings> = state
                    .persistence
                    .list_instance_settings()
                    .await
                    .map_err(internal)?;
                Resp::json(200, &recs)
            }
            .await,
        ),

        (&Method::POST, ["admin", "instance-settings"]) => Some(
            async {
                guard_admin(state, parts).await?;
                let input: InstanceSettingsInput = json_body(body)?;
                let rec = state
                    .persistence
                    .upsert_instance_settings(input)
                    .await
                    .map_err(ApiError::from_upsert)?;
                invalidate(state).await;
                Resp::json(200, &rec)
            }
            .await,
        ),

        _ => None,
    }
}
