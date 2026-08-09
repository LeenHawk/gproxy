//! OpenCode Console REST client — the read side of a console account.
//!
//! Two endpoints matter to this channel: `/api/orgs` picks the workspace, and
//! `/api/config` carries that workspace's managed OpenCode config. The console
//! speaks JSON (not form-encoded OAuth bodies), so the request builders here
//! are shared by the device flow in [`super::login`].

use std::sync::Arc;

use bytes::Bytes;
use http::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderName};
use http::{Request, Response};
use serde::Deserialize;
use serde_json::Value;

use crate::channel::ChannelError;
use crate::http::client::UpstreamClient;

pub(super) const DEFAULT_CONSOLE_URL: &str = "https://console.opencode.ai";

/// Console base URL: the credential's own record of where it was minted, then
/// the provider override (self-hosted enterprise consoles), then the default.
pub(super) fn base_url<'a>(settings: &'a Value, secret: &'a Value) -> &'a str {
    for source in [secret, settings] {
        if let Some(url) = source
            .get("console_base_url")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|url| !url.is_empty())
        {
            return url.trim_end_matches('/');
        }
    }
    DEFAULT_CONSOLE_URL
}

#[derive(Deserialize)]
pub(super) struct Org {
    pub id: String,
    pub name: String,
}

/// The workspace whose managed config to read: first by name, then by id —
/// the selection the OpenCode CLI makes.
pub(super) async fn first_org(
    client: &Arc<dyn UpstreamClient>,
    base: &str,
    access: &str,
) -> Result<Option<Org>, ChannelError> {
    let mut orgs: Vec<Org> = send_json(
        client,
        get(&format!("{base}/api/orgs"), access, None)?,
        "orgs",
    )
    .await?;
    orgs.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));
    Ok(orgs.into_iter().next())
}

/// `provider.<tier>.options.apiKey` from the managed config, when the workspace
/// publishes one. A 404 — no managed config at all — reads as "no key" rather
/// than an error, because that is the ordinary shape of a personal account.
pub(super) async fn workspace_key(
    client: &Arc<dyn UpstreamClient>,
    base: &str,
    access: &str,
    org_id: Option<&str>,
    tier: &str,
) -> Result<Option<String>, ChannelError> {
    let resp = client
        .send(get(&format!("{base}/api/config"), access, org_id)?)
        .await
        .map_err(|e| ChannelError::Build(format!("config request failed: {e}")))?;
    if resp.status() == http::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let body = read_ok(resp, "config")?;
    let config: Value = serde_json::from_slice(&body)
        .map_err(|e| ChannelError::Build(format!("config response parse: {e}")))?;
    Ok(config
        .pointer("/config/provider")
        .and_then(|providers| providers.get(tier))
        .and_then(|provider| provider.pointer("/options/apiKey"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .map(str::to_string))
}

pub(super) fn post(url: &str, body: &Value) -> Result<Request<Bytes>, ChannelError> {
    Request::post(url)
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json")
        .body(Bytes::from(body.to_string()))
        .map_err(|e| ChannelError::Build(format!("console request build: {e}")))
}

fn get(url: &str, access: &str, org_id: Option<&str>) -> Result<Request<Bytes>, ChannelError> {
    let mut builder = Request::get(url)
        .header(ACCEPT, "application/json")
        .header(AUTHORIZATION, format!("Bearer {access}"));
    if let Some(org) = org_id.filter(|org| !org.is_empty()) {
        builder = builder.header(HeaderName::from_static("x-org-id"), org);
    }
    builder
        .body(Bytes::new())
        .map_err(|e| ChannelError::Build(format!("console request build: {e}")))
}

/// Non-2xx → `Build` with the status and a truncated snippet. The snippet never
/// includes the request, which carries the device code.
fn read_ok(resp: Response<Bytes>, what: &str) -> Result<Bytes, ChannelError> {
    let (parts, body) = resp.into_parts();
    if !parts.status.is_success() {
        let snippet: String = String::from_utf8_lossy(&body).chars().take(256).collect();
        return Err(ChannelError::Build(format!(
            "{what} endpoint {}: {snippet}",
            parts.status
        )));
    }
    Ok(body)
}

pub(super) async fn send_json<T: serde::de::DeserializeOwned>(
    client: &Arc<dyn UpstreamClient>,
    req: Request<Bytes>,
    what: &str,
) -> Result<T, ChannelError> {
    let resp = client
        .send(req)
        .await
        .map_err(|e| ChannelError::Build(format!("{what} request failed: {e}")))?;
    let body = read_ok(resp, what)?;
    serde_json::from_slice(&body)
        .map_err(|e| ChannelError::Build(format!("{what} response parse: {e}")))
}

/// Parse the body whatever the status: the device poll signals "not yet" with a
/// non-2xx `{"error": …}` payload.
pub(super) async fn send_json_any_status<T: serde::de::DeserializeOwned>(
    client: &Arc<dyn UpstreamClient>,
    req: Request<Bytes>,
    what: &str,
) -> Result<T, ChannelError> {
    let resp = client
        .send(req)
        .await
        .map_err(|e| ChannelError::Build(format!("{what} request failed: {e}")))?;
    let body = resp.into_body();
    serde_json::from_slice(&body)
        .map_err(|e| ChannelError::Build(format!("{what} response parse: {e}")))
}
