//! Claude Code per-credential usage — `GET /api/oauth/usage` (the undocumented
//! OAuth usage endpoint the CLI's `/status` reads). Returns the rolling 5-hour
//! and 7-day rate-limit windows (utilization % + ISO reset), optional per-model
//! weekly windows, and an `extra_usage` on-demand-credit block. The many
//! experimental codename windows (`tangelo`, `iguana_necktie`, `seven_day_cowork`
//! …) are almost always `null` and are left in [`UsageSnapshot::raw`] rather than
//! modeled. The `claude-code` User-Agent is required — without it the endpoint
//! serves an aggressively rate-limited bucket.

use bytes::Bytes;
use http::header::{
    ACCEPT, ACCEPT_ENCODING, AUTHORIZATION, CONTENT_TYPE, HeaderName, HeaderValue, USER_AGENT,
};
use http::{HeaderMap, Method, Request, StatusCode};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;

use super::auth;
use crate::channel::ChannelError;
use crate::channel::http_util::{build_request, exact_url, join_url};
use crate::channel::usage::{UsageCredits, UsageSnapshot, UsageWindow};
use crate::http::client::TransportOptions;

const USAGE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Build `GET {base}/api/oauth/usage` with the CLI fingerprint headers.
pub(super) fn request(
    secret: &Value,
    settings: &Value,
) -> Result<Option<Request<Bytes>>, ChannelError> {
    let access_token = auth::access_token(secret)?;
    let uri = match crate::channel::settings::endpoint_by_key(settings, "usage", "") {
        Some(url) => exact_url(&url, None)?,
        None => {
            let base = settings
                .get("base_url")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|base| !base.is_empty())
                .unwrap_or(auth::DEFAULT_BASE_URL);
            join_url(base, "/api/oauth/usage", None)?
        }
    };
    let mut req = build_request(Method::GET, uri, HeaderMap::new(), Bytes::new())?;
    req.extensions_mut().insert(TransportOptions {
        total_timeout: Some(USAGE_TIMEOUT),
        max_redirects: Some(21),
        http_version: Some(http::Version::HTTP_11),
        omit_body: true,
    });
    let bearer = HeaderValue::from_str(&format!("Bearer {access_token}"))
        .map_err(|e| ChannelError::InvalidCredential(format!("bad access_token: {e}")))?;
    let h = req.headers_mut();
    h.insert(AUTHORIZATION, bearer);
    h.insert(
        ACCEPT,
        HeaderValue::from_static("application/json, text/plain, */*"),
    );
    h.insert(
        ACCEPT_ENCODING,
        HeaderValue::from_static("gzip, compress, deflate, br"),
    );
    h.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    h.insert(
        USER_AGENT,
        HeaderValue::from_static(auth::CLAUDE_CODE_USER_AGENT),
    );
    h.insert(
        HeaderName::from_static("anthropic-beta"),
        HeaderValue::from_static(auth::ANTHROPIC_BETA),
    );
    Ok(Some(req))
}

/// Parse the `/api/oauth/usage` body into a normalized snapshot.
pub(super) fn parse(status: StatusCode, body: &Bytes) -> Option<UsageSnapshot> {
    if !status.is_success() {
        return None;
    }
    let raw: Value = serde_json::from_slice(body).ok()?;
    let usage: ClaudeUsage = serde_json::from_value(raw.clone()).ok()?;

    let mut windows = Vec::new();
    let mut scoped_seen = HashSet::new();
    for (name, label, window) in [
        ("five_hour", None, &usage.five_hour),
        ("seven_day", None, &usage.seven_day),
        ("seven_day_opus", Some("Opus"), &usage.seven_day_opus),
        ("seven_day_sonnet", Some("Sonnet"), &usage.seven_day_sonnet),
    ] {
        if let Some(w) = window {
            let mut w = w.to_window(name);
            if let Some(label) = label {
                scoped_seen.insert(scope_key(label));
                w = w.label(label);
            }
            windows.push(w);
        }
    }

    if let Some(limits) = &usage.limits {
        if usage.five_hour.is_none()
            && let Some(limit) = limits.iter().find(|limit| limit.kind_is("session"))
        {
            windows.push(limit.to_window("five_hour"));
        }
        if usage.seven_day.is_none()
            && let Some(limit) = limits.iter().find(|limit| limit.kind_is("weekly_all"))
        {
            windows.push(limit.to_window("seven_day"));
        }
        for limit in limits.iter().filter(|limit| limit.kind_is("weekly_scoped")) {
            let Some(label) = limit.scope_label() else {
                continue;
            };
            let key = scope_key(&label);
            if !scoped_seen.insert(key.clone()) {
                continue;
            }
            windows.push(limit.to_window(format!("weekly_scoped:{key}")).label(label));
        }
    }

    // Keep disabled/unlimited states so the Console can render them explicitly.
    let credits = usage
        .extra_usage
        .as_ref()
        .and_then(ClaudeExtraUsage::to_credits);

    Some(UsageSnapshot {
        plan: None,
        windows,
        credits,
        rate_limit_reset_credits: None,
        raw,
    })
}

/// One rolling window: `utilization` is a percentage (0–100, sometimes a bare
/// int), `resets_at` an ISO-8601 timestamp.
#[derive(Deserialize)]
struct ClaudeWindow {
    utilization: Option<f64>,
    resets_at: Option<String>,
}

impl ClaudeWindow {
    fn to_window(&self, name: &str) -> UsageWindow {
        let mut w = UsageWindow::percent(name, self.utilization.unwrap_or(0.0));
        if let Some(iso) = &self.resets_at {
            w = w.resets_iso(iso.clone());
        }
        w
    }
}

/// On-demand overage credits (`extra_usage`). Upstream cents are normalized to
/// major currency units for display.
#[derive(Deserialize)]
struct ClaudeExtraUsage {
    is_enabled: Option<bool>,
    monthly_limit: Option<f64>,
    used_credits: Option<f64>,
    currency: Option<String>,
}

impl ClaudeExtraUsage {
    fn to_credits(&self) -> Option<UsageCredits> {
        let enabled = self.is_enabled?;
        Some(UsageCredits {
            has_credits: Some(enabled),
            unlimited: enabled.then_some(self.monthly_limit.is_none()),
            used_credits: self.used_credits.map(|value| value / 100.0),
            monthly_limit: self.monthly_limit.map(|value| value / 100.0),
            currency: self.currency.clone(),
            ..Default::default()
        })
    }
}

#[derive(Deserialize)]
struct ClaudeLimit {
    kind: Option<String>,
    percent: Option<f64>,
    resets_at: Option<String>,
    scope: Option<ClaudeLimitScope>,
}

impl ClaudeLimit {
    fn kind_is(&self, kind: &str) -> bool {
        self.kind.as_deref() == Some(kind)
    }

    fn to_window(&self, name: impl Into<String>) -> UsageWindow {
        let mut w = UsageWindow::percent(name, self.percent.unwrap_or(0.0));
        if let Some(iso) = &self.resets_at {
            w = w.resets_iso(iso.clone());
        }
        w
    }

    fn scope_label(&self) -> Option<String> {
        let scope = self.scope.as_ref()?;
        scope
            .model
            .as_ref()
            .and_then(|model| {
                non_empty(&model.display_name)
                    .or_else(|| non_empty(&model.id))
                    .map(str::to_owned)
            })
            .or_else(|| non_empty(&scope.surface).map(str::to_owned))
    }
}

#[derive(Deserialize)]
struct ClaudeLimitScope {
    model: Option<ClaudeLimitModel>,
    surface: Option<String>,
}

#[derive(Deserialize)]
struct ClaudeLimitModel {
    display_name: Option<String>,
    id: Option<String>,
}

#[derive(Deserialize)]
struct ClaudeUsage {
    five_hour: Option<ClaudeWindow>,
    seven_day: Option<ClaudeWindow>,
    seven_day_opus: Option<ClaudeWindow>,
    seven_day_sonnet: Option<ClaudeWindow>,
    extra_usage: Option<ClaudeExtraUsage>,
    limits: Option<Vec<ClaudeLimit>>,
}

fn non_empty(value: &Option<String>) -> Option<&str> {
    let value = value.as_deref()?.trim();
    if value.is_empty() { None } else { Some(value) }
}

fn scope_key(label: &str) -> String {
    let mut out = String::new();
    for c in label.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    let out = out.trim_matches('_');
    if out.is_empty() {
        "scoped".to_owned()
    } else {
        out.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_live_usage_shape() {
        // The exact body the live endpoint returns (codename windows are null).
        let body = Bytes::from_static(
            br#"{
              "five_hour": {"utilization": 27, "resets_at": "2026-06-12T16:20:00.899712+00:00"},
              "seven_day": {"utilization": 95, "resets_at": "2026-06-16T08:00:00.899839+00:00"},
              "seven_day_oauth_apps": null,
              "seven_day_opus": null,
              "seven_day_sonnet": {"utilization": 2, "resets_at": "2026-06-16T07:59:59.899847+00:00"},
              "tangelo": null,
              "extra_usage": {"is_enabled": false, "monthly_limit": null, "used_credits": null,
                              "utilization": null, "currency": null, "disabled_reason": null}
            }"#,
        );
        let snap = parse(StatusCode::OK, &body).expect("snapshot");
        // Only the three non-null windows are surfaced (opus null dropped).
        let names: Vec<&str> = snap.windows.iter().map(|w| w.name.as_str()).collect();
        assert_eq!(names, ["five_hour", "seven_day", "seven_day_sonnet"]);
        assert_eq!(snap.windows[1].used_percent, Some(95.0));
        assert_eq!(
            snap.windows[0].resets_at.as_deref(),
            Some("2026-06-12T16:20:00.899712+00:00")
        );
        assert_eq!(snap.credits.unwrap().has_credits, Some(false));
    }

    #[test]
    fn parses_scoped_weekly_limits() {
        let body = Bytes::from_static(
            br#"{
              "extra_usage": {"is_enabled": false, "monthly_limit": null, "used_credits": null,
                              "utilization": null, "currency": null, "disabled_reason": null},
              "five_hour": {"limit_dollars": null, "remaining_dollars": null,
                             "resets_at": "2026-07-02T18:09:59.956325+00:00",
                             "used_dollars": null, "utilization": 23},
              "limits": [
                {"group": "session", "is_active": true, "kind": "session", "percent": 23,
                 "resets_at": "2026-07-02T18:09:59.956325+00:00", "scope": null,
                 "severity": "normal"},
                {"group": "weekly", "is_active": false, "kind": "weekly_all", "percent": 3,
                 "resets_at": "2026-07-03T02:59:59.956351+00:00", "scope": null,
                 "severity": "normal"},
                {"group": "weekly", "is_active": false, "kind": "weekly_scoped", "percent": 0,
                 "resets_at": null,
                 "scope": {"model": {"display_name": "Fable", "id": null}, "surface": null},
                 "severity": "normal"}
              ],
              "seven_day": {"limit_dollars": null, "remaining_dollars": null,
                            "resets_at": "2026-07-03T02:59:59.956351+00:00",
                            "used_dollars": null, "utilization": 3},
              "seven_day_opus": null,
              "seven_day_sonnet": null
            }"#,
        );
        let snap = parse(StatusCode::OK, &body).expect("snapshot");
        let names: Vec<&str> = snap.windows.iter().map(|w| w.name.as_str()).collect();
        assert_eq!(names, ["five_hour", "seven_day", "weekly_scoped:fable"]);

        let fable = snap
            .windows
            .iter()
            .find(|w| w.name == "weekly_scoped:fable")
            .expect("fable window");
        assert_eq!(fable.label.as_deref(), Some("Fable"));
        assert_eq!(fable.used_percent, Some(0.0));
        assert!(fable.resets_at.is_none());
    }

    #[test]
    fn non_success_is_none() {
        assert!(parse(StatusCode::TOO_MANY_REQUESTS, &Bytes::from_static(b"{}")).is_none());
    }

    #[test]
    fn normalizes_extra_usage_money_and_unlimited_state() {
        let finite = ClaudeExtraUsage {
            is_enabled: Some(true),
            monthly_limit: Some(5000.0),
            used_credits: Some(1234.0),
            currency: Some("USD".into()),
        }
        .to_credits()
        .unwrap();
        assert_eq!(finite.used_credits, Some(12.34));
        assert_eq!(finite.monthly_limit, Some(50.0));
        assert_eq!(finite.unlimited, Some(false));

        let unlimited = ClaudeExtraUsage {
            is_enabled: Some(true),
            monthly_limit: None,
            used_credits: None,
            currency: Some("USD".into()),
        }
        .to_credits()
        .unwrap();
        assert_eq!(unlimited.unlimited, Some(true));
    }
}
