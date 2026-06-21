//! OAuth authorization-code login for channels (§14.5).
//!
//! [`Channel`](crate::channel::Channel) covers *per-request* upstream access and
//! the silent `refresh_token` rotation; [`ChannelLogin`] is the orthogonal
//! *first-time* credential acquisition — the interactive authcode + PKCE dance
//! that mints the very secret a channel later refreshes. A channel may impl one,
//! both, or neither (an API-key channel impls neither).
//!
//! Pure async + `serde_json` — no cipher, no persistence, no axum. Compiles on
//! native AND wasm (the registry that holds these is used on every target); the
//! admin HTTP endpoints that drive the flow are native-only. The dual
//! `#[cfg_attr]` async_trait Send/?Send split mirrors [`Channel`].

use std::sync::Arc;

use crate::channel::ChannelError;
use crate::http::client::UpstreamClient;

/// The output of [`ChannelLogin::authcode_start`]: where to send the user, and
/// the redirect_uri the channel actually used.
///
/// `redirect_uri` is echoed back so the matching `complete` exchanges the code
/// with the SAME value the authorize step advertised — OAuth requires them to
/// match. A channel with a fixed redirect_uri ignores the caller's hint and
/// returns its own here.
pub struct AuthCodeStart {
    pub authorize_url: String,
    pub redirect_uri: String,
    /// Opaque channel-owned state minted at start (e.g. a dynamically-registered
    /// OAuth client's `client_id`/`client_secret`/`region` for AWS IdC) that the
    /// matching `complete` must hand back to
    /// [`authcode_exchange`](ChannelLogin::authcode_exchange). `None` for static
    /// flows whose authorize URL needs no prior network call.
    pub extra: Option<serde_json::Value>,
}

/// The output of [`ChannelLogin::device_start`]: the device + user codes and
/// the URL the operator visits to authorize, plus the poll interval the
/// provider asked for (seconds).
pub struct DeviceInit {
    pub device_code: String,
    pub user_code: String,
    pub verification_url: String,
    pub interval_secs: u64,
}

/// One poll tick of a device-code login.
#[derive(Debug)]
pub enum DevicePoll {
    /// The user has not finished authorizing yet — poll again after the
    /// interval (covers both `authorization_pending` and `slow_down`).
    Pending,
    /// Authorized: the PLAINTEXT secret Value the caller seals + persists.
    Ready(serde_json::Value),
    /// The user denied access or the device code expired — abandon the flow.
    Denied,
}

/// Interactive OAuth authorization-code (+PKCE) login for a channel.
///
/// Defaults make the trait opt-in: a channel that does not override returns
/// `None` from [`authcode_start`](ChannelLogin::authcode_start) (no authcode
/// flow) and `Unsupported` from
/// [`authcode_exchange`](ChannelLogin::authcode_exchange).
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait ChannelLogin: Send + Sync {
    /// Build the provider authorize URL for an authcode+PKCE login. `Ok(None)`
    /// means the channel has no authcode flow. An empty `redirect_uri` tells the
    /// channel to use its own default (returned in [`AuthCodeStart`]).
    ///
    /// Async + client-bearing so a channel can do a pre-authorize round-trip
    /// (e.g. AWS IdC dynamic client registration) before producing the URL.
    /// `params` is opaque operator-supplied input (`auth_method`, `region`,
    /// `start_url`, …); `{}` for the common static flow.
    async fn authcode_start(
        &self,
        _client: &Arc<dyn UpstreamClient>,
        _params: &serde_json::Value,
        _redirect_uri: &str,
        _state: &str,
        _pkce_challenge: &str,
    ) -> Result<Option<AuthCodeStart>, ChannelError> {
        Ok(None)
    }

    /// Exchange an authorization `code` (+ the PKCE `verifier`) for the
    /// PLAINTEXT secret Value. `redirect_uri` MUST equal the one
    /// [`authcode_start`](ChannelLogin::authcode_start) used. `extra` is the
    /// `AuthCodeStart::extra` that start stashed (e.g. the registered IdC client
    /// creds); `None` for static flows. The caller seals + persists the returned
    /// Value (purity: the channel never touches cipher/persistence).
    async fn authcode_exchange(
        &self,
        _client: &Arc<dyn UpstreamClient>,
        _code: &str,
        _verifier: &str,
        _redirect_uri: &str,
        _extra: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, ChannelError> {
        Err(ChannelError::Unsupported("authcode login"))
    }

    /// Begin a device-code login: ask the provider for a device + user code.
    /// `None`-by-default channels return `Unsupported`. The caller stashes the
    /// returned `device_code` server-side and polls [`device_poll`].
    ///
    /// `params` is opaque operator-supplied input (mirrors
    /// [`authcode_start`](ChannelLogin::authcode_start)) — e.g.
    /// `{"login_provider":"google"}` to pick the Kiro social provider; `{}` for
    /// the channel default.
    async fn device_start(
        &self,
        _client: &Arc<dyn UpstreamClient>,
        _params: &serde_json::Value,
    ) -> Result<DeviceInit, ChannelError> {
        Err(ChannelError::Unsupported("device login"))
    }

    /// Poll a pending device-code login with the `device_code` from
    /// [`device_start`]. Returns [`DevicePoll::Ready`] with the PLAINTEXT secret
    /// once authorized (the caller seals + persists), else `Pending`/`Denied`.
    async fn device_poll(
        &self,
        _client: &Arc<dyn UpstreamClient>,
        _device_code: &str,
    ) -> Result<DevicePoll, ChannelError> {
        Err(ChannelError::Unsupported("device login"))
    }

    /// Exchange a session `cookie` for the PLAINTEXT secret Value (the caller
    /// seals + persists). For channels whose first-credential bootstrap is a
    /// browser session cookie rather than an interactive OAuth dance.
    async fn cookie_exchange(
        &self,
        _client: &Arc<dyn UpstreamClient>,
        _cookie: &str,
    ) -> Result<serde_json::Value, ChannelError> {
        Err(ChannelError::Unsupported("cookie login"))
    }
}
