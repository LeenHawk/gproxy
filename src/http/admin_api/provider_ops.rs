//! Live provider operations for the cross-target edge admin dispatcher.

use http::Method;
use http::request::Parts;

use crate::admin::guard::guard_admin;
use crate::api::error::ApiError;
use crate::app::AppState;

use super::{Resp, parse_i64, segments};

/// Handle `GET /admin/providers/{id}/upstream-models`.
pub(super) async fn dispatch(state: &AppState, parts: &Parts) -> Option<Result<Resp, ApiError>> {
    let segs = segments(parts);
    let (&Method::GET, ["admin", "providers", provider_id, "upstream-models"]) =
        (&parts.method, segs.as_slice())
    else {
        return None;
    };

    Some(
        async {
            guard_admin(state, parts).await?;
            let provider_id = parse_i64(provider_id)?;
            let models = crate::credentials::upstream_models::fetch_models(state, provider_id)
                .await
                .map_err(models_error)?;
            Resp::json(200, &models)
        }
        .await,
    )
}

fn models_error(error: crate::credentials::upstream_models::ModelsError) -> ApiError {
    use crate::credentials::upstream_models::ModelsError as M;
    match error {
        M::ProviderNotFound | M::UnknownChannel(_) => ApiError::NotFound(error.to_string()),
        M::NoCredential | M::NoAvailableCredential | M::Channel(_) | M::Status(_) => {
            ApiError::BadRequest(error.to_string())
        }
        M::Decrypt(_) | M::Upstream(_) | M::Internal(_) => ApiError::Internal(error.to_string()),
    }
}
