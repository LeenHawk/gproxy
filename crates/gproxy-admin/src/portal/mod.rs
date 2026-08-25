mod context;
mod quota;
mod recent;
mod usage;

use bytes::Bytes;
use gproxy_store::records::SettingRecord;
use http::request::Parts;
use http::{Method, Response, StatusCode};

use crate::dto::{PortalModelDto, PortalQuotaScopeDto};
use crate::{AdminError, State, response};

pub(crate) const RECENT_REQUESTS_SETTING: &str = "portal_recent_requests_enabled";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortalIdentity {
    pub user_id: i64,
    pub user_key_id: i64,
    pub org_id: Option<i64>,
    pub team_id: Option<i64>,
    pub user_name: String,
    pub key_prefix: Option<String>,
    pub key_label: Option<String>,
    pub expires_at: Option<i64>,
}

impl PortalIdentity {
    fn quota_scope(&self, kind: &str, id: i64) -> Option<PortalQuotaScopeDto> {
        match kind {
            "user_key" if id == self.user_key_id => Some(PortalQuotaScopeDto::UserKey),
            "user" if id == self.user_id => Some(PortalQuotaScopeDto::User),
            "organization" if Some(id) == self.org_id => Some(PortalQuotaScopeDto::Organization),
            "team" if Some(id) == self.team_id => Some(PortalQuotaScopeDto::Team),
            _ => None,
        }
    }
}

enum Route {
    Context,
    Models,
    Usage,
    QuotaWindows,
    RecentRequests,
}

pub async fn dispatch(state: &impl State, parts: &Parts, _body: Bytes) -> Option<Response<Bytes>> {
    let path = parts.uri.path();
    if path != "/portal/api" && !path.starts_with("/portal/api/") {
        return None;
    }
    let result = async {
        let identity = state.portal_identity(&parts.headers)?;
        match route(&parts.method, path)? {
            Route::Context => context::get(state, &identity).await,
            Route::Models => response::json(StatusCode::OK, &models(state, &identity)),
            Route::Usage => usage::get(state, &identity, parts).await,
            Route::QuotaWindows => quota::get(state, &identity).await,
            Route::RecentRequests => recent::get(state, &identity, parts).await,
        }
    }
    .await;
    Some(response::render(result, "portal"))
}

pub(crate) fn recent_requests_enabled(settings: &[SettingRecord]) -> bool {
    settings.iter().any(|setting| {
        setting.key == RECENT_REQUESTS_SETTING && setting.value == serde_json::Value::Bool(true)
    })
}

fn models(state: &impl State, identity: &PortalIdentity) -> Vec<PortalModelDto> {
    state.portal_models(identity)
}

fn route(method: &Method, path: &str) -> Result<Route, AdminError> {
    if method != Method::GET {
        return Err(AdminError::NotFound);
    }
    match path {
        "/portal/api/context" => Ok(Route::Context),
        "/portal/api/models" => Ok(Route::Models),
        "/portal/api/usage" => Ok(Route::Usage),
        "/portal/api/quota-windows" => Ok(Route::QuotaWindows),
        "/portal/api/recent-requests" => Ok(Route::RecentRequests),
        _ => Err(AdminError::NotFound),
    }
}
