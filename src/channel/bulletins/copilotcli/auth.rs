//! Copilot CLI auth: re-exchange a long-lived GitHub token for a short-lived
//! Copilot token, and build the OpenAI-chat upstream headers.
//!
//! Token exchange is a GET against `api.github.com/copilot_internal/v2/token`
//! (NOT a form POST), so it bypasses [`crate::channel::oauth::token_post`] and
//! issues `client.send` directly. There is no `refresh_token`: every refresh
//! re-exchanges from the GitHub token.
//!
//! Login — GitHub device flow ([`device_start`] / [`device_poll`]): `POST
//! github.com/login/device/code` with `client_id = Iv1.b507a08c87ecfe98` and
//! `scope = read:user`, show the `user_code`, poll
//! `github.com/login/oauth/access_token` until the user authorizes, then store
//! the returned `access_token` as `github_token`. See the TS sample
//! `services/github/{get-device-code,poll-access-token}.ts`.

use std::sync::Arc;

use bytes::Bytes;
use http::header::{CONTENT_TYPE, HeaderName, HeaderValue};
use http::{Request, Response};
use serde::Deserialize;
use serde_json::Value;

use crate::channel::ChannelError;
use crate::http::client::UpstreamClient;

/// Editor identity the GitHub/Copilot endpoints expect (mirrors the VS Code
/// Copilot Chat extension; from the TS sample's `lib/api-config.ts`).
const DEFAULT_VSCODE_VERSION: &str = "1.95.3";
pub(super) const EDITOR_PLUGIN_VERSION: &str = "copilot-chat/0.43.0";
pub(super) const USER_AGENT: &str = "copilot/1.0.61 (linux v24.16.0) term/unknown";
pub(super) const API_VERSION: &str = "2025-04-01";

/// Copilot CLI model-path identity (captured from `copilot/1.0.61`), distinct
/// from the VS Code Copilot Chat extension the token-exchange flow mimics.
const EDITOR_VERSION: &str = "copilot/1.0.61";
const CHAT_API_VERSION: &str = "2026-06-01";
const COPILOT_INTEGRATION_ID: &str = "copilot-developer-cli";
const GITHUB_TOKEN_URL: &str = "https://api.github.com/copilot_internal/v2/token";

/// GitHub device-flow OAuth client (the public Copilot CLI / VS Code id) +
/// endpoints, mined from the TS sample's `services/github/*.ts`.
const DEVICE_CLIENT_ID: &str = "Iv1.b507a08c87ecfe98";
const DEVICE_CODE_URL: &str = "https://github.com/login/device/code";
const DEVICE_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
const DEVICE_SCOPE: &str = "read:user";

/// `github.com/login/device/code` response (`Accept: application/json`).
#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    #[serde(default = "default_interval")]
    interval: u64,
}

fn default_interval() -> u64 {
    5
}

/// `github.com/login/oauth/access_token` device-poll response: either an
/// `access_token` (success) or an `error` (pending / slow_down / denied).
#[derive(Debug, Deserialize)]
struct DeviceTokenResponse {
    access_token: Option<String>,
    error: Option<String>,
}

/// Start the GitHub device flow: POST the client_id + scope, get back the
/// device + user codes and the verification URL the operator visits.
pub(super) async fn device_start(
    client: &Arc<dyn UpstreamClient>,
) -> Result<crate::channel::DeviceInit, ChannelError> {
    let form = format!("client_id={DEVICE_CLIENT_ID}&scope={DEVICE_SCOPE}");
    let req = Request::post(DEVICE_CODE_URL)
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(http::header::ACCEPT, "application/json")
        .body(Bytes::from(form))
        .map_err(|e| ChannelError::Build(format!("device code request build: {e}")))?;
    let parsed: DeviceCodeResponse = send_json(client, req, "device code").await?;
    Ok(crate::channel::DeviceInit {
        device_code: parsed.device_code,
        user_code: parsed.user_code,
        verification_url: parsed.verification_uri,
        interval_secs: parsed.interval,
    })
}

/// Poll the GitHub device flow once. The token endpoint returns `access_token`
/// on success or an `error` string; `authorization_pending` / `slow_down` map
/// to `Pending`, `access_denied` / `expired_token` to `Denied`. The minted
/// secret is `{github_token}` — the channel's `refresh` re-exchanges it for the
/// short-lived Copilot token (M7b).
pub(super) async fn device_poll(
    client: &Arc<dyn UpstreamClient>,
    device_code: &str,
) -> Result<crate::channel::DevicePoll, ChannelError> {
    use crate::channel::DevicePoll;
    let form = format!(
        "client_id={DEVICE_CLIENT_ID}&device_code={device_code}\
         &grant_type=urn:ietf:params:oauth:grant-type:device_code"
    );
    let req = Request::post(DEVICE_TOKEN_URL)
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(http::header::ACCEPT, "application/json")
        .body(Bytes::from(form))
        .map_err(|e| ChannelError::Build(format!("device token request build: {e}")))?;
    let parsed: DeviceTokenResponse = send_json(client, req, "device token").await?;

    if let Some(token) = parsed.access_token.filter(|s| !s.is_empty()) {
        return Ok(DevicePoll::Ready(
            serde_json::json!({ "github_token": token }),
        ));
    }
    match parsed.error.as_deref() {
        Some("authorization_pending") | Some("slow_down") => Ok(DevicePoll::Pending),
        Some("access_denied") | Some("expired_token") => Ok(DevicePoll::Denied),
        // An unknown error is terminal — surface it rather than poll forever.
        Some(other) => Err(ChannelError::Build(format!("device poll error: {other}"))),
        None => Err(ChannelError::Build(
            "device poll: neither access_token nor error".into(),
        )),
    }
}

/// Send `req` and parse a 2xx JSON body into `T`. Non-2xx → `Build` with the
/// status + a truncated snippet (never the request form, which carries the
/// device_code). GitHub's device endpoints answer 200 even for a `slow_down`
/// payload, so the JSON `error` field — not the status — drives the decision.
async fn send_json<T: serde::de::DeserializeOwned>(
    client: &Arc<dyn UpstreamClient>,
    req: Request<Bytes>,
    what: &str,
) -> Result<T, ChannelError> {
    let resp: Response<Bytes> = client
        .send(req)
        .await
        .map_err(|e| ChannelError::Build(format!("{what} request failed: {e}")))?;
    let (parts, body) = resp.into_parts();
    if !parts.status.is_success() {
        let snippet: String = String::from_utf8_lossy(&body).chars().take(256).collect();
        return Err(ChannelError::Build(format!(
            "{what} endpoint {}: {snippet}",
            parts.status
        )));
    }
    serde_json::from_slice(&body)
        .map_err(|e| ChannelError::Build(format!("{what} response parse: {e}")))
}

/// `api.github.com/copilot_internal/v2/token` response. Tolerant of unknown
/// fields; `expires_at` is unix SECONDS, `refresh_in` is unused here (we key
/// off `expires_at` with a skew in [`super`]).
#[derive(Debug, Deserialize)]
pub(super) struct CopilotTokenResponse {
    pub token: String,
    pub expires_at: i64,
}

/// Read the required long-lived GitHub token from the secret.
pub(super) fn github_token(secret: &Value) -> Result<&str, ChannelError> {
    secret
        .get("github_token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ChannelError::InvalidCredential("missing github_token".into()))
}

/// Emulated VS Code version (secret override, else the baked default).
pub(super) fn vscode_version(secret: &Value) -> &str {
    secret
        .get("vscode_version")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_VSCODE_VERSION)
}

/// Copilot API base URL by `account_type` (default `individual`).
pub(super) fn base_url(secret: &Value) -> String {
    match secret.get("account_type").and_then(Value::as_str) {
        Some("business") => "https://api.business.githubcopilot.com".into(),
        Some("enterprise") => "https://api.enterprise.githubcopilot.com".into(),
        _ => "https://api.githubcopilot.com".into(),
    }
}

/// GET the Copilot token from GitHub using the long-lived `github_token`. The
/// caller persists the result; this only performs the exchange. Non-2xx →
/// [`ChannelError::Build`] with the status + a truncated body snippet.
pub(super) async fn exchange_copilot_token(
    client: &Arc<dyn UpstreamClient>,
    github_token: &str,
    vscode_version: &str,
) -> Result<CopilotTokenResponse, ChannelError> {
    let req = Request::get(GITHUB_TOKEN_URL)
        .header("authorization", format!("token {github_token}"))
        .header("editor-version", format!("vscode/{vscode_version}"))
        .header("editor-plugin-version", EDITOR_PLUGIN_VERSION)
        .header("user-agent", USER_AGENT)
        .header("x-github-api-version", API_VERSION)
        .header(http::header::ACCEPT, "application/json")
        .body(Bytes::new())
        .map_err(|e| ChannelError::Build(format!("copilot token request build: {e}")))?;

    let resp: Response<Bytes> = client
        .send(req)
        .await
        .map_err(|e| ChannelError::Build(format!("copilot token request failed: {e}")))?;
    let (parts, body) = resp.into_parts();
    if !parts.status.is_success() {
        let snippet: String = String::from_utf8_lossy(&body).chars().take(256).collect();
        return Err(ChannelError::Build(format!(
            "copilot token endpoint {}: {snippet}",
            parts.status
        )));
    }
    serde_json::from_slice(&body)
        .map_err(|e| ChannelError::Build(format!("copilot token response parse: {e}")))
}

/// Inject the Copilot OpenAI-chat headers onto the prepared upstream request.
/// `body` is the already-assembled upstream body, inspected only to pick
/// `X-Initiator` (agent when assistant/tool turns are present, else user).
/// Stable per-credential `x-client-machine-id` (v4-shaped UUID) the Copilot CLI
/// persists per machine. Derived from the long-lived `github_token` so it stays
/// constant for the credential (the short-lived copilot_token rotates).
pub(super) fn machine_id(secret: &Value) -> String {
    let seed = secret
        .get("github_token")
        .and_then(Value::as_str)
        .or_else(|| secret.get("copilot_token").and_then(Value::as_str))
        .unwrap_or("");
    let d = blake3::hash(format!("copilot-machine:{seed}").as_bytes());
    let mut b = [0u8; 16];
    b.copy_from_slice(&d.as_bytes()[..16]);
    crate::util::rand::uuid_v4_from(&b)
}

pub(super) fn apply_chat_headers(
    req: &mut Request<Bytes>,
    copilot_token: &str,
    machine_id: &str,
) -> Result<(), ChannelError> {
    let bearer = HeaderValue::from_str(&format!("Bearer {copilot_token}"))
        .map_err(|e| ChannelError::InvalidCredential(format!("bad copilot_token: {e}")))?;
    let machine = HeaderValue::from_str(machine_id)
        .map_err(|e| ChannelError::Build(format!("bad x-client-machine-id: {e}")))?;
    let interaction = HeaderValue::from_str(&new_request_id())
        .map_err(|e| ChannelError::Build(format!("bad x-interaction-id: {e}")))?;
    let initiator = if has_assistant_or_tool_turn(req.body()) {
        "agent"
    } else {
        "user"
    };

    // Copilot CLI model-path header set (captured from copilot/1.0.61), NOT the
    // VS Code Copilot Chat extension's.
    let h = req.headers_mut();
    h.insert(http::header::AUTHORIZATION, bearer);
    h.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    h.insert(
        HeaderName::from_static("copilot-integration-id"),
        HeaderValue::from_static(COPILOT_INTEGRATION_ID),
    );
    h.insert(
        HeaderName::from_static("editor-version"),
        HeaderValue::from_static(EDITOR_VERSION),
    );
    h.insert(
        http::header::USER_AGENT,
        HeaderValue::from_static(USER_AGENT),
    );
    h.insert(
        HeaderName::from_static("openai-intent"),
        HeaderValue::from_static("conversation-agent"),
    );
    h.insert(
        HeaderName::from_static("x-github-api-version"),
        HeaderValue::from_static(CHAT_API_VERSION),
    );
    h.insert(HeaderName::from_static("x-interaction-id"), interaction);
    h.insert(HeaderName::from_static("x-client-machine-id"), machine);
    h.insert(
        HeaderName::from_static("x-initiator"),
        HeaderValue::from_static(initiator),
    );
    Ok(())
}

/// `X-Initiator` heuristic: Copilot expects `agent` once the conversation
/// contains an assistant or tool turn, else `user`. A cheap substring probe on
/// the JSON body is enough — a false `user` is harmless, and we never mutate
/// the body. Falls back to `user` on non-JSON / parse failure.
fn has_assistant_or_tool_turn(body: &Bytes) -> bool {
    let Ok(v) = serde_json::from_slice::<Value>(body) else {
        return false;
    };
    v.get("messages")
        .and_then(Value::as_array)
        .is_some_and(|msgs| {
            msgs.iter().any(|m| {
                matches!(
                    m.get("role").and_then(Value::as_str),
                    Some("assistant") | Some("tool")
                )
            })
        })
}

/// Fresh per-request id for `x-request-id` (cross-target, cryptographically
/// random).
fn new_request_id() -> String {
    crate::util::rand::uuid_v4()
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::channel::DevicePoll;
    use crate::http::client::ClientError;
    use http::Response;
    use std::sync::Mutex;

    /// Returns the next queued body per `send`, so one mock drives the whole
    /// pending → ready / denied poll sequence.
    struct QueueUpstream(Mutex<Vec<&'static [u8]>>);
    #[async_trait::async_trait]
    impl UpstreamClient for QueueUpstream {
        async fn send(&self, _req: Request<Bytes>) -> Result<Response<Bytes>, ClientError> {
            let body = self.0.lock().unwrap().remove(0);
            Ok(Response::builder()
                .status(200)
                .body(Bytes::from_static(body))
                .unwrap())
        }
    }

    /// device_poll maps GitHub's 200-with-`error` payloads to Pending/Denied and
    /// an `access_token` payload to Ready with a `{github_token}` secret.
    #[tokio::test]
    async fn device_poll_maps_states() {
        let client: Arc<dyn UpstreamClient> = Arc::new(QueueUpstream(Mutex::new(vec![
            br#"{"error":"authorization_pending"}"#,
            br#"{"error":"access_denied"}"#,
            br#"{"access_token":"ghu_tok"}"#,
        ])));
        assert!(matches!(
            device_poll(&client, "dc").await.unwrap(),
            DevicePoll::Pending
        ));
        assert!(matches!(
            device_poll(&client, "dc").await.unwrap(),
            DevicePoll::Denied
        ));
        match device_poll(&client, "dc").await.unwrap() {
            DevicePoll::Ready(secret) => assert_eq!(secret["github_token"], "ghu_tok"),
            other => panic!("expected Ready, got {other:?}"),
        }
    }
}
