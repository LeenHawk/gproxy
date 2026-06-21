//! Codex auth — OpenAI ChatGPT-backend OAuth2 `refresh_token` grant against
//! `auth.openai.com`; base `https://chatgpt.com/backend-api/codex`. Request
//! bodies are normalized to the private Responses API (see
//! [`normalize_responses_body`]); the inbound `/v1/responses` path is rewritten
//! to `/responses` by [`super::CodexChannel::prepare`].
//!
//! The login-time authorization-code + PKCE flow (client_id
//! `app_EMoamEEZ73f0CkXaXp7hrann`) is an M10 concern; this module covers only
//! the per-request access-token use and the refresh the pipeline drives. For
//! headless setups, the secret is provisioned via §14.5 remote-paste.
//!
//! As an impersonation channel it injects the codex-cli fingerprint / protocol
//! headers (its per-channel allow-list, applied after the global blacklist):
//! `user-agent`, `originator`, `session-id`, `thread-id`, `x-client-request-id`,
//! `x-codex-beta-features`, `x-codex-turn-metadata`, `x-codex-window-id`.
//! (`accept: text/event-stream` rides the base allow-list.)

use std::sync::Arc;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64URL;
use bytes::Bytes;
use http::Request;
use http::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderName, HeaderValue, USER_AGENT};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::channel::ChannelError;
use crate::channel::login::{DeviceInit, DevicePoll};
use crate::channel::oauth;
use crate::http::client::UpstreamClient;

/// Public Codex CLI OAuth client (the credentials the official CLI ships with).
pub(super) const OAUTH_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
pub(super) const TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
/// Authorization endpoint for the interactive authcode+PKCE login (§14.5).
pub(super) const AUTHORIZE_URL: &str = "https://auth.openai.com/oauth/authorize";

/// Default redirect_uri + scopes the Codex CLI uses (mined from v1 codex.rs).
/// The CLI listens on `localhost:1455` and exchanges the code with this exact
/// value, so `complete` must echo it back.
pub(super) const DEFAULT_REDIRECT_URI: &str = "http://localhost:1455/auth/callback";
pub(super) const OAUTH_SCOPE: &str =
    "openid profile email offline_access api.connectors.read api.connectors.invoke";

/// ChatGPT codex-backend host. The Responses endpoint lives at `/responses`.
pub(super) const DEFAULT_BASE_URL: &str = "https://chatgpt.com/backend-api/codex";

/// User-Agent the Codex CLI (`codex exec`) sends; rides the credential's
/// `tls_fingerprint` pool (M7a). The `originator` header carries the same id.
/// Captured from `codex exec` (docs/agent-tls-fingerprints.md §5); the
/// interactive TUI would instead be `codex_cli_rs` — keep UA + originator in
/// sync if switching forms.
pub(super) const ORIGINATOR: &str = "codex_exec";
pub(super) const USER_AGENT_VALUE: &str =
    "codex_exec/0.137.0 (Debian 13.0.0; x86_64) xterm-256color (codex_exec; 0.137.0)";
/// Codex client version — sent as the `client_version` query on the model-list
/// / model-get endpoint (v1 parity). Keep in sync with `USER_AGENT_VALUE`.
pub(super) const CODEX_VERSION: &str = "0.137.0";

/// Refresh slightly before expiry to avoid racing a 401 mid-flight.
const EXPIRY_SKEW_MS: i64 = 60_000;

/// Keys stripped from the Responses body — the ChatGPT backend rejects or
/// ignores them (it pins sampling itself and never persists).
const STRIP_KEYS: &[&str] = &[
    "max_output_tokens",
    "metadata",
    "stream_options",
    "temperature",
    "top_p",
    "top_logprobs",
    "safety_identifier",
    "truncation",
];

/// Build the authorize URL for the interactive authcode+PKCE login. An empty
/// `redirect_uri` falls back to [`DEFAULT_REDIRECT_URI`]. Returns the URL plus
/// the effective redirect_uri (so `complete` exchanges with the same value).
///
/// The query carries the standard authcode+PKCE set plus the two codex-specific
/// flags the CLI sends (`id_token_add_organizations`, `codex_cli_simplified_flow`)
/// and `originator`, mined from v1 codex.rs.
pub(super) fn authcode_start(redirect_uri: &str, state: &str, challenge: &str) -> (String, String) {
    let redirect_uri = if redirect_uri.trim().is_empty() {
        DEFAULT_REDIRECT_URI
    } else {
        redirect_uri
    };
    let query = [
        ("response_type", "code"),
        ("client_id", OAUTH_CLIENT_ID),
        ("redirect_uri", redirect_uri),
        ("scope", OAUTH_SCOPE),
        ("code_challenge", challenge),
        ("code_challenge_method", "S256"),
        ("id_token_add_organizations", "true"),
        ("codex_cli_simplified_flow", "true"),
        ("state", state),
        ("originator", ORIGINATOR),
    ]
    .iter()
    .map(|(k, v)| format!("{k}={}", pct(v)))
    .collect::<Vec<_>>()
    .join("&");
    (format!("{AUTHORIZE_URL}?{query}"), redirect_uri.to_string())
}

/// Exchange an authorization code (+PKCE verifier) for the plaintext secret
/// `{access_token, refresh_token?, expires_at_ms, account_id?, id_token?}`. The
/// `account_id` is decoded from the id_token's `chatgpt_account_id` claim so
/// `prepare` can send the `chatgpt-account-id` header.
pub(super) async fn authcode_exchange(
    client: &Arc<dyn UpstreamClient>,
    code: &str,
    verifier: &str,
    redirect_uri: &str,
) -> Result<Value, ChannelError> {
    let form = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", OAUTH_CLIENT_ID),
        ("code_verifier", verifier),
    ];
    let resp = oauth::token_post(client, TOKEN_URL, &form, &[]).await?;

    let access_token = resp
        .access_token
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ChannelError::Build("token response missing access_token".into()))?;
    let expires_at_ms = crate::util::time::unix_now().saturating_mul(1000)
        + resp.expires_in.unwrap_or(3600) as i64 * 1000;

    let mut secret = json!({
        "access_token": access_token,
        "expires_at_ms": expires_at_ms,
    });
    if let Some(rt) = resp.refresh_token.filter(|s| !s.is_empty()) {
        secret["refresh_token"] = Value::String(rt);
    }
    // Extract the ChatGPT account id from the id_token (JWT) so prepare can send
    // `chatgpt-account-id`; keep the id_token for a later refresh re-extract.
    if let Some(id_token) = resp.id_token.filter(|s| !s.is_empty()) {
        if let Some(account_id) = account_id_from_id_token(&id_token) {
            secret["account_id"] = Value::String(account_id);
        }
        if let Some(email) = email_from_id_token(&id_token) {
            secret["user_email"] = Value::String(email);
        }
        secret["id_token"] = Value::String(id_token);
    }
    Ok(secret)
}

// ── Device-code login (`codex login --device-auth`) ─────────────────────────────
// OpenAI's CUSTOM device grant (NOT RFC 8628): request a one-time `user_code`,
// poll the token endpoint until the operator approves in the browser, then run
// the SAME authorization_code + PKCE exchange as the loopback flow (only the
// redirect_uri differs). Confirmed against `samples/codex/.../device_code_auth.rs`.

/// `POST {client_id}` → `{device_auth_id, user_code, interval}`.
const DEVICE_USERCODE_URL: &str = "https://auth.openai.com/api/accounts/deviceauth/usercode";
/// `POST {device_auth_id, user_code}` → 2xx `{authorization_code, code_verifier}`;
/// 403/404 = still pending.
const DEVICE_TOKEN_URL: &str = "https://auth.openai.com/api/accounts/deviceauth/token";
/// Page the operator opens to enter the one-time code.
const DEVICE_VERIFICATION_URL: &str = "https://auth.openai.com/codex/device";
/// redirect_uri bound in the device-code token exchange (the code was issued
/// against it, so the exchange must echo it).
const DEVICE_REDIRECT_URI: &str = "https://auth.openai.com/deviceauth/callback";

fn default_interval() -> u64 {
    5
}

/// `usercode` response. `interval` may arrive as a number or a string.
#[derive(Deserialize)]
struct UserCodeResp {
    device_auth_id: String,
    #[serde(alias = "usercode")]
    user_code: String,
    #[serde(default = "default_interval", deserialize_with = "de_interval")]
    interval: u64,
}

fn de_interval<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    match Value::deserialize(d)? {
        Value::Number(n) => Ok(n.as_u64().unwrap_or_else(default_interval)),
        Value::String(s) => Ok(s.trim().parse().unwrap_or_else(|_| default_interval())),
        _ => Ok(default_interval()),
    }
}

/// Authorized `token` poll response (the PKCE verifier is minted server-side).
#[derive(Deserialize)]
struct DevicePollResp {
    authorization_code: String,
    code_verifier: String,
}

/// Opaque server-side state packed into `DeviceInit::device_code` — the device
/// poll needs BOTH `device_auth_id` and `user_code`, but the shared
/// `DeviceSession` stashes only one string. Never shown to the operator.
#[derive(serde::Serialize, Deserialize)]
struct DeviceState {
    device_auth_id: String,
    user_code: String,
}

/// Begin a device-code login: `POST {usercode}` `{client_id}` → the one-time
/// `user_code` + the verification URL the operator visits. The `device_auth_id`
/// is packed (with the user_code) into the opaque `device_code` for polling.
pub(super) async fn device_start(
    client: &Arc<dyn UpstreamClient>,
) -> Result<DeviceInit, ChannelError> {
    let (status, body) = device_post(
        client,
        DEVICE_USERCODE_URL,
        &json!({ "client_id": OAUTH_CLIENT_ID }),
    )
    .await?;
    if !status.is_success() {
        let snippet: String = String::from_utf8_lossy(&body).chars().take(256).collect();
        return Err(ChannelError::Build(format!(
            "codex deviceauth usercode {status}: {snippet}"
        )));
    }
    let resp: UserCodeResp = serde_json::from_slice(&body)
        .map_err(|e| ChannelError::Build(format!("codex usercode response parse: {e}")))?;
    let device_code = serde_json::to_string(&DeviceState {
        device_auth_id: resp.device_auth_id,
        user_code: resp.user_code.clone(),
    })
    .map_err(|e| ChannelError::Build(format!("codex device state serialize: {e}")))?;

    Ok(DeviceInit {
        device_code,
        user_code: resp.user_code,
        verification_url: DEVICE_VERIFICATION_URL.to_string(),
        interval_secs: resp.interval.max(1),
    })
}

/// Poll a pending device-code login once. `403`/`404` → [`DevicePoll::Pending`];
/// `2xx` → exchange the issued authorization code (reusing [`authcode_exchange`]
/// with the device redirect) → [`DevicePoll::Ready`] with the plaintext secret;
/// anything else → [`DevicePoll::Denied`].
pub(super) async fn device_poll(
    client: &Arc<dyn UpstreamClient>,
    device_code: &str,
) -> Result<DevicePoll, ChannelError> {
    let state: DeviceState = serde_json::from_str(device_code)
        .map_err(|e| ChannelError::Build(format!("codex device state parse: {e}")))?;
    let (status, body) = device_post(
        client,
        DEVICE_TOKEN_URL,
        &json!({ "device_auth_id": state.device_auth_id, "user_code": state.user_code }),
    )
    .await?;

    match status.as_u16() {
        403 | 404 => Ok(DevicePoll::Pending),
        200..=299 => {
            let parsed: DevicePollResp = serde_json::from_slice(&body)
                .map_err(|e| ChannelError::Build(format!("codex device token parse: {e}")))?;
            let secret = authcode_exchange(
                client,
                &parsed.authorization_code,
                &parsed.code_verifier,
                DEVICE_REDIRECT_URI,
            )
            .await?;
            Ok(DevicePoll::Ready(secret))
        }
        _ => Ok(DevicePoll::Denied),
    }
}

/// POST a JSON `body` to a device endpoint, returning `(status, body)` so the
/// caller can branch on the status (the poll endpoint uses 403/404 for pending).
async fn device_post(
    client: &Arc<dyn UpstreamClient>,
    url: &str,
    body: &Value,
) -> Result<(http::StatusCode, Bytes), ChannelError> {
    let payload = serde_json::to_vec(body)
        .map_err(|e| ChannelError::Build(format!("codex device request serialize: {e}")))?;
    let req = Request::builder()
        .method(http::Method::POST)
        .uri(url)
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json")
        .body(Bytes::from(payload))
        .map_err(|e| ChannelError::Build(format!("codex device request build: {e}")))?;
    let resp = client
        .send(req)
        .await
        .map_err(|e| ChannelError::Build(format!("codex device request failed: {e}")))?;
    let (parts, body) = resp.into_parts();
    Ok((parts.status, body))
}

/// Percent-encode a query value, leaving the RFC 3986 unreserved set verbatim.
fn pct(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(
                char::from_digit((b >> 4) as u32, 16)
                    .unwrap()
                    .to_ascii_uppercase(),
            );
            out.push(
                char::from_digit((b & 0xf) as u32, 16)
                    .unwrap()
                    .to_ascii_uppercase(),
            );
        }
    }
    out
}

/// Read a trimmed, non-empty string field from the secret.
fn secret_str<'a>(secret: &'a Value, key: &str) -> Option<&'a str> {
    secret
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

/// The OAuth access token, required by [`super::CodexChannel::prepare`].
pub(super) fn access_token(secret: &Value) -> Result<&str, ChannelError> {
    secret_str(secret, "access_token")
        .ok_or_else(|| ChannelError::InvalidCredential("missing access_token".into()))
}

/// The ChatGPT account id, sent as `chatgpt-account-id` when present.
pub(super) fn account_id(secret: &Value) -> Option<&str> {
    secret_str(secret, "account_id")
}

/// Decode the ChatGPT account id from an OAuth `id_token` (a JWT). The payload
/// is base64url-decoded WITHOUT signature verification — we trust our own OAuth
/// token exchange transport, not the token itself — and the id lives at the
/// claim `https://api.openai.com/auth` → `chatgpt_account_id`.
fn account_id_from_id_token(id_token: &str) -> Option<String> {
    let payload_b64 = id_token.split('.').nth(1)?;
    let bytes = B64URL.decode(payload_b64).ok()?;
    let payload: Value = serde_json::from_slice(&bytes).ok()?;
    payload
        .get("https://api.openai.com/auth")
        .and_then(|auth| auth.get("chatgpt_account_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
}

/// Decode the user email from an OAuth `id_token` (JWT). Tries the top-level
/// `email` claim first, then `https://api.openai.com/profile` → `email`.
fn email_from_id_token(id_token: &str) -> Option<String> {
    let payload_b64 = id_token.split('.').nth(1)?;
    let bytes = B64URL.decode(payload_b64).ok()?;
    let payload: Value = serde_json::from_slice(&bytes).ok()?;
    payload
        .get("email")
        .or_else(|| {
            payload
                .get("https://api.openai.com/profile")
                .and_then(|p| p.get("email"))
        })
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
}

/// Whether the access token is absent or within the skew window of expiry.
pub(super) fn needs_refresh(secret: &Value) -> bool {
    if secret_str(secret, "access_token").is_none() {
        return true;
    }
    let expires_at_ms = secret
        .get("expires_at_ms")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    // `expires_at_ms == 0` means "unknown" → treat as valid; the 401-driven
    // refresh path still covers stale tokens.
    if expires_at_ms == 0 {
        return false;
    }
    let now_ms = crate::util::time::unix_now().saturating_mul(1000);
    now_ms > expires_at_ms - EXPIRY_SKEW_MS
}

/// Refresh via the OpenAI `refresh_token` grant, returning the new plaintext
/// secret (`access_token` + `expires_at_ms` rotate; `refresh_token` and
/// `id_token` rotate when the response carries them, else the old ones are
/// kept; every other field — notably `account_id` — is preserved).
pub(super) async fn refresh(
    client: &Arc<dyn UpstreamClient>,
    secret: &Value,
) -> Result<Value, ChannelError> {
    let refresh_token = secret_str(secret, "refresh_token")
        .ok_or_else(|| ChannelError::InvalidCredential("missing refresh_token".into()))?;

    let form = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", OAUTH_CLIENT_ID),
    ];
    let resp = oauth::token_post(client, TOKEN_URL, &form, &[]).await?;

    let new_access = resp
        .access_token
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ChannelError::Build("refresh response missing access_token".into()))?;
    let expires_at_ms = crate::util::time::unix_now().saturating_mul(1000)
        + resp.expires_in.unwrap_or(3600) as i64 * 1000;

    let mut out = secret.clone();
    let obj = out
        .as_object_mut()
        .ok_or_else(|| ChannelError::Build("secret is not an object".into()))?;
    obj.insert("access_token".into(), Value::String(new_access));
    // refresh_token ROTATES when present — store the new one, else keep the old.
    if let Some(rt) = resp.refresh_token.filter(|s| !s.is_empty()) {
        obj.insert("refresh_token".into(), Value::String(rt));
    }
    // A rotated id_token re-backfills `account_id` (and is itself preserved);
    // otherwise the clone keeps the existing account_id from login.
    if let Some(id_token) = resp.id_token.filter(|s| !s.is_empty()) {
        if let Some(account_id) = account_id_from_id_token(&id_token) {
            obj.insert("account_id".into(), Value::String(account_id));
        }
        obj.insert("id_token".into(), Value::String(id_token));
    }
    obj.insert("expires_at_ms".into(), Value::Number(expires_at_ms.into()));
    Ok(out)
}

/// Inject the OAuth bearer + Codex CLI fingerprint headers onto the prepared
/// upstream request. A fresh per-request session id is generated and shared by
/// `session_id` and `x-client-request-id` (the backend expects them to match).
/// `accept` is `text/event-stream` since the body always forces `stream:true`.
pub(super) fn apply(
    req: &mut Request<Bytes>,
    access_token: &str,
    account_id: Option<&str>,
) -> Result<(), ChannelError> {
    let bearer = HeaderValue::from_str(&format!("Bearer {access_token}"))
        .map_err(|e| ChannelError::InvalidCredential(format!("bad access_token: {e}")))?;
    let session_id = HeaderValue::from_str(&new_session_id())
        .map_err(|e| ChannelError::Build(format!("bad session id: {e}")))?;

    let h = req.headers_mut();
    h.insert(AUTHORIZATION, bearer);
    h.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    h.insert(ACCEPT, HeaderValue::from_static("text/event-stream"));
    h.insert(USER_AGENT, HeaderValue::from_static(USER_AGENT_VALUE));
    h.insert(
        HeaderName::from_static("originator"),
        HeaderValue::from_static(ORIGINATOR),
    );
    // `session-id` / `x-client-request-id`: forward a codex-aware client's own
    // values (allow-listed in `prepare`, kept consistent with its turn-metadata);
    // generate a matching pair only as a fallback when the client omits them.
    h.entry(HeaderName::from_static("session-id"))
        .or_insert_with(|| session_id.clone());
    h.entry(HeaderName::from_static("x-client-request-id"))
        .or_insert(session_id);
    if let Some(acct) = account_id {
        let acct = HeaderValue::from_str(acct)
            .map_err(|e| ChannelError::InvalidCredential(format!("bad account_id: {e}")))?;
        h.insert(HeaderName::from_static("chatgpt-account-id"), acct);
    }
    Ok(())
}

/// Normalize a Responses request body to what the ChatGPT codex backend expects.
///
/// - forces `stream:true`, `store:false`;
/// - drops the [`STRIP_KEYS`] (sampling / persistence / metadata fields);
/// - lifts instructions: a string `input` becomes a single user message; system
///   messages in an array `input` are pulled out, their text appended to the
///   top-level `instructions` (newline-joined), and removed from `input`.
///
/// Non-JSON bodies are forwarded verbatim (this never fails the request).
pub(super) fn normalize_responses_body(body: &Bytes) -> Bytes {
    let Ok(mut value) = serde_json::from_slice::<Value>(body) else {
        return body.clone();
    };
    let Some(map) = value.as_object_mut() else {
        return body.clone();
    };

    map.insert("stream".into(), Value::Bool(true));
    map.insert("store".into(), Value::Bool(false));
    for key in STRIP_KEYS {
        map.remove(*key);
    }

    let mut instructions = map
        .get("instructions")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    // A string `input` is the bare user prompt → wrap as a message item.
    if let Some(text) = map.get("input").and_then(Value::as_str) {
        let text = text.to_string();
        map.insert(
            "input".into(),
            json!([{ "type": "message", "role": "user", "content": text }]),
        );
    }

    // Lift `role:"system"` messages out of an array `input` into instructions.
    if let Some(Value::Array(items)) = map.get_mut("input") {
        let mut retained = Vec::with_capacity(items.len());
        for item in std::mem::take(items) {
            if is_system_role(&item) {
                append_instruction(&mut instructions, item_text(&item));
            } else {
                retained.push(item);
            }
        }
        *items = retained;
    }

    map.insert("instructions".into(), Value::String(instructions));
    serde_json::to_vec(&value)
        .map(Bytes::from)
        .unwrap_or_else(|_| body.clone())
}

/// Whether an `input` item is a `role:"system"` message (case-insensitive).
fn is_system_role(item: &Value) -> bool {
    item.get("role")
        .and_then(Value::as_str)
        .is_some_and(|r| r.eq_ignore_ascii_case("system"))
}

/// Append `text` to `instructions`, newline-joining onto any existing content.
fn append_instruction(instructions: &mut String, text: String) {
    if text.is_empty() {
        return;
    }
    if !instructions.is_empty() {
        instructions.push('\n');
    }
    instructions.push_str(&text);
}

/// Flatten a message item's `content` to text: a bare string, or the joined
/// `text` fields of an array of content parts.
fn item_text(item: &Value) -> String {
    let mut parts = Vec::new();
    collect_text(item.get("content").unwrap_or(&Value::Null), &mut parts);
    parts.join("\n")
}

fn collect_text(value: &Value, parts: &mut Vec<String>) {
    match value {
        Value::String(text) if !text.is_empty() => parts.push(text.clone()),
        Value::Array(items) => items.iter().for_each(|item| collect_text(item, parts)),
        Value::Object(object) => {
            if let Some(text) = object.get("text").and_then(Value::as_str)
                && !text.is_empty()
            {
                parts.push(text.to_string());
            }
        }
        _ => {}
    }
}

/// Fresh per-request v4 session id (cross-target, cryptographically random).
fn new_session_id() -> String {
    crate::util::rand::uuid_v4()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// A fake [`UpstreamClient`] that records the outbound request and replays a
    /// canned response. Used to assert the exact authcode-exchange wire shape.
    struct CapturingUpstream {
        seen: Mutex<Option<Request<Bytes>>>,
        body: Bytes,
    }

    #[async_trait::async_trait]
    impl UpstreamClient for CapturingUpstream {
        async fn send(
            &self,
            req: Request<Bytes>,
        ) -> Result<http::Response<Bytes>, crate::http::client::ClientError> {
            *self.seen.lock().unwrap() = Some(req);
            Ok(http::Response::builder()
                .status(200)
                .body(self.body.clone())
                .unwrap())
        }
    }

    /// `authcode_exchange` POSTs the PKCE authorization-code form to the OpenAI
    /// token endpoint and maps the response into the plaintext secret — the
    /// account id is decoded from the id_token's `chatgpt_account_id` claim.
    #[tokio::test]
    async fn authcode_exchange_builds_request_and_maps_secret() {
        // id_token carrying the account-id claim (signature is opaque/unverified).
        let id_payload = json!({
            "https://api.openai.com/auth": { "chatgpt_account_id": "acct-77" }
        });
        let id_token = format!(
            "hdr.{}.sig",
            B64URL.encode(serde_json::to_vec(&id_payload).unwrap())
        );
        let token_resp = json!({
            "access_token": "at-new",
            "refresh_token": "rt-new",
            "id_token": id_token,
            "expires_in": 3600,
            "token_type": "Bearer",
        });
        let upstream = Arc::new(CapturingUpstream {
            seen: Mutex::new(None),
            body: Bytes::from(serde_json::to_vec(&token_resp).unwrap()),
        });
        let client: Arc<dyn UpstreamClient> = upstream.clone();

        let secret = authcode_exchange(
            &client,
            "the-code",
            "the-verifier",
            "http://localhost:1455/auth/callback",
        )
        .await
        .expect("exchange ok");

        // --- request shape: method, URL, and the exact form body ---
        let req = upstream.seen.lock().unwrap().take().expect("a request");
        assert_eq!(req.method(), http::Method::POST);
        assert_eq!(req.uri().to_string(), "https://auth.openai.com/oauth/token");
        let body = String::from_utf8(req.body().to_vec()).unwrap();
        // Form is application/x-www-form-urlencoded; assert each pct-encoded pair.
        assert!(body.contains("grant_type=authorization_code"), "{body}");
        assert!(body.contains("code=the-code"), "{body}");
        assert!(
            body.contains("redirect_uri=http%3A%2F%2Flocalhost%3A1455%2Fauth%2Fcallback"),
            "{body}"
        );
        assert!(
            body.contains(&format!("client_id={OAUTH_CLIENT_ID}")),
            "{body}"
        );
        assert!(body.contains("code_verifier=the-verifier"), "{body}");

        // --- secret mapping: tokens + account_id from id_token + expiry ---
        assert_eq!(secret["access_token"], "at-new");
        assert_eq!(secret["refresh_token"], "rt-new");
        assert_eq!(secret["account_id"], "acct-77");
        assert_eq!(secret["id_token"], id_token);
        assert!(secret["expires_at_ms"].as_i64().unwrap() > crate::util::time::unix_now() * 1000);
    }

    #[test]
    fn account_id_decoded_from_id_token_claim() {
        // A JWT whose payload carries the OpenAI auth claim (header/sig are
        // opaque — we never verify the signature, only base64-decode the payload).
        let payload = json!({
            "https://api.openai.com/auth": { "chatgpt_account_id": "acct-xyz" },
            "email": "u@example.com"
        });
        let payload_b64 = B64URL.encode(serde_json::to_vec(&payload).unwrap());
        let jwt = format!("eyJhbG.{payload_b64}.sig");
        assert_eq!(account_id_from_id_token(&jwt).as_deref(), Some("acct-xyz"));

        // Missing claim / malformed token → None (not an error).
        let no_claim = B64URL.encode(br#"{"email":"x"}"#);
        assert_eq!(account_id_from_id_token(&format!("h.{no_claim}.s")), None);
        assert_eq!(account_id_from_id_token("not-a-jwt"), None);
    }

    /// A mock that replays a queue of `(status, body)` responses in order and
    /// records the request URIs — drives the multi-call device-poll path.
    struct QueueUpstream {
        responses: Mutex<Vec<(u16, Vec<u8>)>>,
        seen: Mutex<Vec<String>>,
    }
    #[async_trait::async_trait]
    impl UpstreamClient for QueueUpstream {
        async fn send(
            &self,
            req: Request<Bytes>,
        ) -> Result<http::Response<Bytes>, crate::http::client::ClientError> {
            self.seen.lock().unwrap().push(req.uri().to_string());
            let (status, body) = self.responses.lock().unwrap().remove(0);
            Ok(http::Response::builder()
                .status(status)
                .body(Bytes::from(body))
                .unwrap())
        }
    }
    fn queue(responses: Vec<(u16, Value)>) -> Arc<QueueUpstream> {
        Arc::new(QueueUpstream {
            responses: Mutex::new(
                responses
                    .into_iter()
                    .map(|(s, v)| (s, serde_json::to_vec(&v).unwrap()))
                    .collect(),
            ),
            seen: Mutex::new(Vec::new()),
        })
    }

    /// `device_start` POSTs `{client_id}` to the usercode endpoint and packs the
    /// device_auth_id + user_code into the opaque device_code.
    #[tokio::test]
    async fn device_start_requests_user_code() {
        let up = queue(vec![(
            200,
            json!({ "device_auth_id": "dev-1", "user_code": "WXYZ-1234", "interval": "7" }),
        )]);
        let client: Arc<dyn UpstreamClient> = up.clone();
        let init = device_start(&client).await.expect("device_start ok");

        assert_eq!(
            up.seen.lock().unwrap()[0],
            "https://auth.openai.com/api/accounts/deviceauth/usercode"
        );
        assert_eq!(init.user_code, "WXYZ-1234");
        assert_eq!(
            init.verification_url,
            "https://auth.openai.com/codex/device"
        );
        assert_eq!(init.interval_secs, 7); // string "7" parsed
        // device_code carries the poll state (device_auth_id + user_code).
        let state: Value = serde_json::from_str(&init.device_code).unwrap();
        assert_eq!(state["device_auth_id"], "dev-1");
        assert_eq!(state["user_code"], "WXYZ-1234");
    }

    /// `device_poll`: 403 → Pending; an authorized poll → exchange the issued
    /// code → Ready with the mapped secret.
    #[tokio::test]
    async fn device_poll_pending_then_authorized() {
        let device_code = serde_json::to_string(&json!({
            "device_auth_id": "dev-1", "user_code": "WXYZ-1234"
        }))
        .unwrap();

        // 403 → still pending; the request hit the token endpoint.
        let up = queue(vec![(403, json!({}))]);
        let client: Arc<dyn UpstreamClient> = up.clone();
        assert!(matches!(
            device_poll(&client, &device_code).await.unwrap(),
            DevicePoll::Pending
        ));
        assert_eq!(
            up.seen.lock().unwrap()[0],
            "https://auth.openai.com/api/accounts/deviceauth/token"
        );

        // Authorized: poll returns the code, then the token exchange returns the
        // tokens → Ready with the account-id-bearing secret.
        let id_payload =
            json!({ "https://api.openai.com/auth": { "chatgpt_account_id": "acct-9" } });
        let id_token = format!(
            "h.{}.s",
            B64URL.encode(serde_json::to_vec(&id_payload).unwrap())
        );
        let up = queue(vec![
            (
                200,
                json!({ "authorization_code": "auth-code", "code_verifier": "ver" }),
            ),
            (
                200,
                json!({ "access_token": "at-d", "refresh_token": "rt-d", "id_token": id_token, "expires_in": 3600 }),
            ),
        ]);
        let client: Arc<dyn UpstreamClient> = up.clone();
        let secret = match device_poll(&client, &device_code).await.unwrap() {
            DevicePoll::Ready(v) => v,
            other => panic!("expected Ready, got {other:?}"),
        };
        assert_eq!(secret["access_token"], "at-d");
        assert_eq!(secret["refresh_token"], "rt-d");
        assert_eq!(secret["account_id"], "acct-9");
        // Second call hit the OAuth token endpoint with the device redirect.
        let seen = up.seen.lock().unwrap();
        assert_eq!(
            seen[0],
            "https://auth.openai.com/api/accounts/deviceauth/token"
        );
        assert_eq!(seen[1], "https://auth.openai.com/oauth/token");
    }
}
