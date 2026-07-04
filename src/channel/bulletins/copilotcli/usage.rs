//! GitHub Copilot per-credential usage — `GET api.github.com/copilot_internal/user`.
//! Returns the Copilot plan, a quota reset date, and a `quota_snapshots` object
//! with one entry per metered feature (`chat`, `completions`,
//! `premium_interactions`), each carrying an entitlement / remaining count, a
//! percent-remaining, and an `unlimited` flag. Authenticated with the long-lived
//! GitHub token (`token <…>`), not the short-lived Copilot bearer.

use bytes::Bytes;
use http::header::{ACCEPT, AUTHORIZATION, HeaderName, HeaderValue, USER_AGENT};
use http::{HeaderMap, Method, Request, StatusCode};
use serde::Deserialize;
use serde_json::Value;

use super::auth;
use crate::channel::ChannelError;
use crate::channel::http_util::{build_request, join_url};
use crate::channel::usage::{UsageSnapshot, UsageWindow};

const GITHUB_API_BASE: &str = "https://api.github.com";

/// Build `GET /copilot_internal/user` with the GitHub-token fingerprint headers.
pub(super) fn request(
    secret: &Value,
    _settings: &Value,
) -> Result<Option<Request<Bytes>>, ChannelError> {
    let github_token = auth::github_token(secret)?;
    let vscode_version = auth::vscode_version(secret);

    let uri = join_url(GITHUB_API_BASE, "/copilot_internal/user", None)?;
    let mut req = build_request(Method::GET, uri, HeaderMap::new(), Bytes::new())?;
    let auth_val = HeaderValue::from_str(&format!("token {github_token}"))
        .map_err(|e| ChannelError::InvalidCredential(format!("bad github_token: {e}")))?;
    let editor = HeaderValue::from_str(&format!("vscode/{vscode_version}"))
        .map_err(|e| ChannelError::Build(format!("bad editor-version: {e}")))?;
    let h = req.headers_mut();
    h.insert(AUTHORIZATION, auth_val);
    h.insert(ACCEPT, HeaderValue::from_static("application/json"));
    h.insert(HeaderName::from_static("editor-version"), editor);
    h.insert(
        HeaderName::from_static("editor-plugin-version"),
        HeaderValue::from_static(auth::EDITOR_PLUGIN_VERSION),
    );
    h.insert(USER_AGENT, HeaderValue::from_static(auth::USER_AGENT));
    h.insert(
        HeaderName::from_static("x-github-api-version"),
        HeaderValue::from_static(auth::API_VERSION),
    );
    Ok(Some(req))
}

/// Parse the `/copilot_internal/user` body into a snapshot — one window per
/// metered feature in `quota_snapshots`.
pub(super) fn parse(status: StatusCode, body: &Bytes) -> Option<UsageSnapshot> {
    if !status.is_success() {
        return None;
    }
    let raw: Value = serde_json::from_slice(body).ok()?;
    let resp: CopilotUsageResponse = serde_json::from_value(raw.clone()).ok()?;

    let reset = resp.quota_reset_date.as_deref();
    let snaps = resp.quota_snapshots.unwrap_or_default();
    let windows = [
        ("chat", &snaps.chat),
        ("completions", &snaps.completions),
        ("premium_interactions", &snaps.premium_interactions),
    ]
    .into_iter()
    .filter_map(|(name, detail)| detail.as_ref().map(|d| d.to_window(name, reset)))
    .collect();

    Some(UsageSnapshot {
        plan: resp.copilot_plan.filter(|s| !s.is_empty()),
        windows,
        credits: None,
        rate_limit_reset_credits: None,
        raw,
    })
}

#[derive(Deserialize)]
struct CopilotUsageResponse {
    copilot_plan: Option<String>,
    quota_reset_date: Option<String>,
    quota_snapshots: Option<QuotaSnapshots>,
}

#[derive(Deserialize, Default)]
struct QuotaSnapshots {
    chat: Option<QuotaDetail>,
    completions: Option<QuotaDetail>,
    premium_interactions: Option<QuotaDetail>,
}

/// `entitlement` is the period grant, `remaining` the count left,
/// `percent_remaining` 0–100; `unlimited` short-circuits all of these.
#[derive(Deserialize)]
struct QuotaDetail {
    entitlement: Option<f64>,
    remaining: Option<f64>,
    percent_remaining: Option<f64>,
    unlimited: Option<bool>,
}

impl QuotaDetail {
    fn to_window(&self, name: &str, reset: Option<&str>) -> UsageWindow {
        let mut w = UsageWindow {
            name: name.to_string(),
            ..Default::default()
        };
        if self.unlimited != Some(true) {
            w.used_percent = self
                .percent_remaining
                .map(|p| (100.0 - p).clamp(0.0, 100.0));
            w.limit = self.entitlement;
            w.used = match (self.entitlement, self.remaining) {
                (Some(e), Some(r)) => Some((e - r).max(0.0)),
                _ => None,
            };
        }
        if let Some(r) = reset {
            w = w.resets_iso(r);
        }
        w
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_quota_snapshots() {
        let body = Bytes::from_static(
            br#"{
              "copilot_plan": "pro",
              "quota_reset_date": "2026-07-01",
              "quota_snapshots": {
                "chat": {"entitlement": 0, "remaining": 0, "percent_remaining": 100, "unlimited": true},
                "completions": {"entitlement": 0, "remaining": 0, "percent_remaining": 100, "unlimited": true},
                "premium_interactions": {"entitlement": 300, "remaining": 270, "percent_remaining": 90,
                                         "unlimited": false, "overage_count": 0, "overage_permitted": false}
              }
            }"#,
        );
        let snap = parse(StatusCode::OK, &body).expect("snapshot");
        assert_eq!(snap.plan.as_deref(), Some("pro"));
        assert_eq!(snap.windows.len(), 3);
        let premium = snap
            .windows
            .iter()
            .find(|w| w.name == "premium_interactions")
            .unwrap();
        assert_eq!(premium.used_percent, Some(10.0));
        assert_eq!(premium.limit, Some(300.0));
        assert_eq!(premium.used, Some(30.0));
        assert_eq!(premium.resets_at.as_deref(), Some("2026-07-01"));
        // Unlimited features carry no percentage.
        let chat = snap.windows.iter().find(|w| w.name == "chat").unwrap();
        assert!(chat.used_percent.is_none());
    }
}
