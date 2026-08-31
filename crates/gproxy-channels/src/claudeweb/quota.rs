//! Claude Web account quota — `GET {base}/api/organizations/{organization}/usage`
//! with the browser sessionKey cookie. Rolling 5-hour and 7-day windows as
//! `{utilization, resets_at}` (percent 0–100, ISO-8601 reset) plus scoped
//! weekly `limits[]` entries; the live endpoint always carries the primary
//! windows, so a payload without them is invalid rather than "no usage".

use std::collections::HashSet;

use bytes::Bytes;
use gproxy_channel_api::{ChannelError, QuotaObservation};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::Value;

const FIVE_HOURS: i64 = 5 * 60 * 60;
const SEVEN_DAYS: i64 = 7 * 24 * 60 * 60;

pub(super) fn probe_request(
    secret: &Value,
    settings: &Value,
) -> Result<Option<http::Request<Bytes>>, ChannelError> {
    let auth = super::auth::Auth::read(secret)?;
    let base = super::auth::base(settings);
    let uri = super::endpoint::url(
        settings,
        base,
        &auth.organization,
        "",
        "claudeweb_usage",
        &format!("/api/organizations/{}/usage", auth.organization),
    )?;
    let mut request = http::Request::get(uri)
        .body(Bytes::new())
        .map_err(|error| ChannelError::Prepare(error.to_string()))?;
    *request.headers_mut() = auth.headers(base, &format!("{base}/new"))?;
    request.headers_mut().insert(
        http::header::ACCEPT,
        "application/json".parse().expect("static"),
    );
    request
        .extensions_mut()
        .insert(super::profile::CLIENT_PROFILE.clone());
    Ok(Some(request))
}

pub(super) fn parse_probe(status: http::StatusCode, body: &[u8]) -> Vec<QuotaObservation> {
    if !status.is_success() {
        return Vec::new();
    }
    let Ok(usage) = serde_json::from_slice::<WebUsage>(body) else {
        return Vec::new();
    };
    let mut observations = vec![
        observation("five_hour".into(), FIVE_HOURS, &usage.five_hour),
        observation("seven_day".into(), SEVEN_DAYS, &usage.seven_day),
    ];
    let mut scoped_seen = HashSet::new();
    // One seven_day_<model> window per metered model family (opus, sonnet,
    // fable, ...) — match the scheme, not a fixed model list.
    for (key, value) in &usage.extra {
        let Some(model) = key.strip_prefix("seven_day_") else {
            continue;
        };
        let Ok(window) = serde_json::from_value::<WebWindow>(value.clone()) else {
            continue;
        };
        scoped_seen.insert(format!("model:{model}"));
        observations.push(observation(key.clone(), SEVEN_DAYS, &window));
    }
    for limit in usage
        .limits
        .iter()
        .flatten()
        .filter(|limit| limit.kind.as_deref() == Some("weekly_scoped"))
    {
        let Some(scope) = limit.scoped() else {
            continue;
        };
        if !scoped_seen.insert(scope.identity()) {
            continue;
        }
        let period_end = limit
            .resets_at
            .as_deref()
            .and_then(crate::shared::quota::iso_to_unix);
        observations.push(QuotaObservation {
            window_key: scope.window_key(),
            period_start: period_end.map(|end| end - SEVEN_DAYS),
            period_end,
            used_percent: limit
                .percent
                .and_then(|value| Decimal::try_from(value).ok()),
            upstream_used: None,
            upstream_limit: None,
        });
    }
    observations
}

fn observation(window_key: String, duration: i64, window: &WebWindow) -> QuotaObservation {
    let period_end = crate::shared::quota::iso_to_unix(&window.resets_at);
    QuotaObservation {
        window_key,
        period_start: period_end.map(|end| end - duration),
        period_end,
        used_percent: window
            .utilization
            .and_then(|value| Decimal::try_from(value).ok()),
        upstream_used: None,
        upstream_limit: None,
    }
}

#[derive(Deserialize)]
struct WebUsage {
    five_hour: WebWindow,
    seven_day: WebWindow,
    #[serde(default)]
    limits: Option<Vec<WebLimit>>,
    #[serde(flatten)]
    extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Deserialize)]
struct WebWindow {
    utilization: Option<f64>,
    resets_at: String,
}

#[derive(Deserialize)]
struct WebLimit {
    kind: Option<String>,
    percent: Option<f64>,
    resets_at: Option<String>,
    scope: Option<WebLimitScope>,
}

#[derive(Deserialize)]
struct WebLimitScope {
    model: Option<WebLimitModel>,
    surface: Option<String>,
}

#[derive(Deserialize)]
struct WebLimitModel {
    display_name: Option<String>,
    id: Option<String>,
}

enum Scoped {
    Model { key: String, label: String },
    Surface { key: String },
}

impl WebLimit {
    fn scoped(&self) -> Option<Scoped> {
        let scope = self.scope.as_ref()?;
        if let Some(model) = &scope.model {
            let id = non_empty(model.id.as_deref());
            let display = non_empty(model.display_name.as_deref());
            let selector = id.or(display)?;
            return Some(Scoped::Model {
                key: crate::shared::quota::slug(selector, "scoped"),
                label: display.unwrap_or(selector).to_owned(),
            });
        }
        let surface = non_empty(scope.surface.as_deref())?;
        Some(Scoped::Surface {
            key: crate::shared::quota::slug(surface, "scoped"),
        })
    }
}

impl Scoped {
    fn identity(&self) -> String {
        match self {
            Self::Model { label, .. } => {
                format!("model:{}", crate::shared::quota::slug(label, "scoped"))
            }
            Self::Surface { key } => format!("surface:{key}"),
        }
    }

    fn window_key(&self) -> String {
        match self {
            Self::Model { key, .. } => format!("weekly_model:{key}"),
            Self::Surface { key } => format!("weekly_surface:{key}"),
        }
    }
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::parse_probe;

    #[test]
    fn primary_windows_and_scoped_limits_become_observations() {
        let body = json!({
            "five_hour": { "utilization": 3.0, "resets_at": "2026-07-12T16:29:59.581984+00:00" },
            "seven_day": { "utilization": 61.0, "resets_at": "2026-07-17T21:59:59+00:00" },
            "seven_day_opus": { "utilization": 12.0, "resets_at": "2026-07-17T21:59:59+00:00" },
            "seven_day_sonnet": null,
            "limits": [
                { "kind": "weekly_scoped", "percent": 12.0, "resets_at": "2026-07-17T21:59:59Z",
                  "scope": { "model": { "id": "claude-opus-5", "display_name": "Opus" } } },
                { "kind": "weekly_scoped", "percent": 40.0, "resets_at": "2026-07-17T21:59:59Z",
                  "scope": { "surface": "Claude Code" } },
                { "kind": "weekly_all", "percent": 61.0, "resets_at": "2026-07-17T21:59:59Z" }
            ]
        });
        let observed = parse_probe(http::StatusCode::OK, &serde_json::to_vec(&body).unwrap());
        let keys: Vec<&str> = observed
            .iter()
            .map(|value| value.window_key.as_str())
            .collect();
        // The Opus limit duplicates seven_day_opus and is dropped in its favour.
        assert_eq!(
            keys,
            [
                "five_hour",
                "seven_day",
                "seven_day_opus",
                "weekly_surface:claude_code"
            ]
        );
        assert_eq!(observed[0].used_percent, Some("3".parse().unwrap()));
        let end = observed[0].period_end.unwrap();
        assert_eq!(observed[0].period_start, Some(end - 5 * 60 * 60));
        let end = observed[3].period_end.unwrap();
        assert_eq!(observed[3].period_start, Some(end - 7 * 24 * 60 * 60));
        assert_eq!(observed[3].used_percent, Some("40".parse().unwrap()));
    }

    #[test]
    fn rejects_payload_without_required_primary_windows() {
        let body = br#"{"five_hour":null,"seven_day":null}"#;
        assert!(parse_probe(http::StatusCode::OK, body).is_empty());
    }
}
