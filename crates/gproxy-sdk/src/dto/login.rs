//! What a credential login asks for and answers with.
//!
//! A login is three exchanges rather than one call, so each flow has a request
//! that starts it and a response that carries the opaque `loginSessionId` the
//! next step names. Nothing here carries a secret: the material a flow acquires
//! is sealed and written straight to the credential row, and all the caller
//! learns is the id of that row.

use serde::{Deserialize, Serialize};

/// Who the credential belongs to, in the application layer's own vocabulary.
/// Opaque to this crate: it copies the three columns through untouched, and
/// only the layer above knows what an organization or a team is.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialOwner {
    #[serde(default)]
    pub organization_id: Option<String>,
    #[serde(default)]
    pub team_id: Option<String>,
    #[serde(default)]
    pub user_id: Option<String>,
}

/// Begin a browser redirect login. The SDK mints the PKCE verifier and the
/// CSRF state itself, so neither is a field here.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthCodeStart {
    pub provider_id: String,
    /// Where the upstream should send the person back. Absent lets the channel
    /// use the redirect it registered with the upstream itself, which is what
    /// a CLI-shaped channel needs.
    #[serde(default)]
    pub redirect_uri: Option<String>,
    /// The label for the credential this login will create. Absent derives one
    /// from the channel and whatever account the upstream names.
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub owner: CredentialOwner,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthCodeStarted {
    /// Names the pending session in every later step. It is not a secret, but
    /// whoever holds it can complete this login.
    pub login_session_id: String,
    /// Where to send the person's browser.
    pub authorize_url: String,
    /// The redirect the channel actually asked for, which is what the callback
    /// will arrive at.
    pub redirect_uri: String,
}

/// Finish a browser redirect login, with either the whole callback URL or the
/// authorization code picked out of it — never both.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthCodeComplete {
    pub login_session_id: String,
    /// The callback as the browser received it; `code` and `state` are read
    /// out of its query.
    #[serde(default)]
    pub callback_url: Option<String>,
    /// The bare authorization code, for a caller that already parsed it.
    #[serde(default)]
    pub code: Option<String>,
    /// The state that came back with a bare code. Absent skips the comparison;
    /// present and different is refused.
    #[serde(default)]
    pub state: Option<String>,
}

/// A login that produced a credential row. The secret is already sealed in the
/// database; nothing about it travels back.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialCreated {
    pub credential_id: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceStart {
    pub provider_id: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub owner: CredentialOwner,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceStarted {
    pub login_session_id: String,
    /// What the person types on the other device.
    pub user_code: String,
    pub verification_uri: String,
    /// The same page with the code already filled in, where the upstream
    /// offers one.
    pub verification_uri_complete: Option<String>,
    /// How long to wait before the first poll, and between polls. The upstream
    /// may raise it; a `Pending` answer always carries the current value.
    pub interval_secs: u64,
    /// When the upstream stops accepting this user code, if it said.
    pub expires_at_ms: Option<i64>,
}

/// One polling step. The SDK never sleeps and never loops: the caller owns the
/// cadence, and `interval_secs` is what it should honour.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "status",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum DevicePollOutcome {
    /// Nobody has approved it yet. Poll again after `interval_secs`.
    Pending {
        interval_secs: u64,
    },
    Ready {
        credential_id: String,
    },
    /// The person refused. The session is gone; starting again is the only way
    /// forward.
    Denied,
    /// The user code outlived its window. The session is gone.
    Expired,
}

/// Exchange a browser session cookie for a credential. One call: there is no
/// pending state to park.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CookieExchange {
    pub provider_id: String,
    /// The cookie as the browser holds it; the channel normalizes it.
    pub cookie: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub owner: CredentialOwner,
}
