//! OAuth login-flow DTOs (§14.5). axum-free serde so they compile on every
//! target; the admin HTTP endpoints that use them are native-only.

/// `POST /admin/login-flows/start` body. `redirect_uri` is optional — when
/// omitted the channel picks its own default.
#[derive(serde::Deserialize)]
pub struct LoginStartRequest {
    pub channel: String,
    #[serde(default)]
    pub redirect_uri: Option<String>,
    /// Opaque channel-specific login inputs forwarded to `authcode_start` (e.g.
    /// `{"auth_method":"idc","region":"…","start_url":"…"}` for Kiro IdC). Static
    /// social/OAuth flows omit it.
    #[serde(default)]
    pub params: Option<serde_json::Value>,
}

/// `start` response: the one-shot session id to feed back into `complete`, and
/// the authorize URL to send the user to.
#[derive(serde::Serialize)]
pub struct LoginStartResponse {
    pub login_session_id: String,
    pub authorize_url: String,
}

/// `POST /admin/login-flows/complete` body. `callback_url` is the full redirect
/// URL the provider sent the user back to (it carries `code` + `state`). The
/// minted credential lands in `provider_id`'s pool under the optional `name`.
///
/// `code` is the alternative for CODE-ONLY flows (e.g. geminicli's
/// `codeassist.google.com/authcode`) where the provider shows a bare
/// authorization code with NO callback URL / `state`: when `code` is present it
/// takes precedence and `callback_url` may be empty.
#[derive(serde::Deserialize)]
pub struct LoginCompleteRequest {
    pub login_session_id: String,
    #[serde(default)]
    pub callback_url: String,
    #[serde(default)]
    pub code: Option<String>,
    pub provider_id: i64,
    #[serde(default)]
    pub name: Option<String>,
}

/// `POST /admin/login-flows/device/start` body. The minted credential lands in
/// `provider_id`'s pool under the optional `name`.
#[derive(serde::Deserialize)]
pub struct DeviceStartRequest {
    pub channel: String,
    pub provider_id: i64,
    #[serde(default)]
    pub name: Option<String>,
    /// Opaque channel-specific device-login inputs forwarded to `device_start`
    /// (e.g. `{"login_provider":"google"}` for the Kiro social provider). Omit
    /// for the channel default.
    #[serde(default)]
    pub params: Option<serde_json::Value>,
}

/// `device/start` response: the one-shot session id to poll with, plus the
/// user-facing code + verification URL and the requested poll interval.
#[derive(serde::Serialize)]
pub struct DeviceStartResponse {
    pub login_session_id: String,
    pub user_code: String,
    pub verification_url: String,
    pub interval_secs: u64,
}

/// `POST /admin/login-flows/device/poll` body.
#[derive(serde::Deserialize)]
pub struct DevicePollRequest {
    pub login_session_id: String,
}

/// `POST /admin/login-flows/cookie` body. The minted credential lands in
/// `provider_id`'s pool under the optional `name`.
#[derive(serde::Deserialize)]
pub struct CookieLoginRequest {
    pub channel: String,
    pub cookie: String,
    pub provider_id: i64,
    #[serde(default)]
    pub name: Option<String>,
}
