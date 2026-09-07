mod auth;
mod context;
mod keys;
mod oauth_sessions;
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
    pub org_id: Option<i64>,
    pub team_id: Option<i64>,
    pub user_name: String,
}

impl PortalIdentity {
    fn quota_scope(&self, kind: &str, id: i64) -> Option<PortalQuotaScopeDto> {
        match kind {
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

pub async fn dispatch(state: &impl State, parts: &Parts, body: Bytes) -> Option<Response<Bytes>> {
    let path = parts.uri.path();
    if path != "/portal/api" && !path.starts_with("/portal/api/") {
        return None;
    }
    let result = async {
        match (&parts.method, path) {
            (&Method::POST, "/portal/api/login") => return auth::login(state, parts, &body).await,
            (&Method::GET, "/portal/api/session") => return auth::status(state, parts).await,
            (&Method::POST, "/portal/api/logout") => return auth::logout(state, parts).await,
            (&Method::POST, "/portal/api/password") => {
                return auth::change_password(state, parts, &body).await;
            }
            _ => {}
        }
        let identity = auth::identity(state, parts).await?;
        if path == "/portal/api/oauth-sessions" && parts.method == Method::GET {
            return oauth_sessions::list(state, &identity, parts).await;
        }
        if let Some(id) = path
            .strip_prefix("/portal/api/oauth-sessions/")
            .and_then(|id| id.parse::<i64>().ok())
            && parts.method == Method::DELETE
        {
            return oauth_sessions::revoke(state, &identity, parts, id).await;
        }
        if path == "/portal/api/keys" {
            return match parts.method {
                Method::GET => keys::list(state, &identity).await,
                Method::POST => keys::create(state, parts, &identity, &body).await,
                _ => Err(AdminError::NotFound),
            };
        }
        if let Some(id) = path
            .strip_prefix("/portal/api/keys/")
            .and_then(|value| value.strip_suffix("/reveal"))
            .and_then(|value| value.parse::<i64>().ok())
            && parts.method == Method::POST
        {
            return keys::reveal(state, parts, &identity, id).await;
        }
        if let Some(id) = path
            .strip_prefix("/portal/api/keys/")
            .and_then(|value| value.parse::<i64>().ok())
        {
            return match parts.method {
                Method::PATCH => keys::update(state, parts, &identity, id, &body).await,
                Method::DELETE => keys::delete(state, parts, &identity, id).await,
                _ => Err(AdminError::NotFound),
            };
        }
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
