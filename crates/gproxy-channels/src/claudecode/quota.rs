//! Claude OAuth quota windows — `GET {base}/api/oauth/usage`, the endpoint
//! the Claude Code CLI's `/status` reads. Rolling 5-hour and 7-day windows
//! as `{utilization, resets_at}` (percent 0–100, ISO-8601 reset), plus a
//! `limits[]` array on newer accounts (`session`, `weekly_all`,
//! `weekly_scoped` kinds). The `claude-code` User-Agent is required —
//! without it the endpoint serves an aggressively rate-limited bucket.

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
    let token = super::auth::access_token(secret)?;
    let base = settings
        .get("base_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|base| !base.is_empty())
        .unwrap_or(super::auth::DEFAULT_BASE_URL);
    let uri = crate::shared::http::join(base, "/api/oauth/usage", None)?;
    http::Request::get(uri)
        .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
        .header(http::header::ACCEPT, "application/json, text/plain, */*")
        .header(http::header::USER_AGENT, super::auth::CLI_USER_AGENT)
        .header("anthropic-beta", super::auth::OAUTH_BETA)
        .body(Bytes::new())
        .map(Some)
        .map_err(|error| ChannelError::Prepare(error.to_string()))
}

pub(super) fn parse_probe(status: http::StatusCode, body: &[u8]) -> Vec<QuotaObservation> {
    if !status.is_success() {
        return Vec::new();
    }
    let Ok(usage) = serde_json::from_slice::<ClaudeUsage>(body) else {
        return Vec::new();
    };
    let mut observations = Vec::new();
    for (key, value) in &usage.windows {
        let duration = match key.as_str() {
            "five_hour" => Some(FIVE_HOURS),
            key if key == "seven_day" || key.starts_with("seven_day_") => Some(SEVEN_DAYS),
            // An unrecognised top-level key is only a window when it carries
            // both fields for real — `Map::get` also returns `Some` for an
            // explicit JSON null, which would mint an empty perpetual window.
            _ if present(value, "utilization") && present(value, "resets_at") => None,
            _ => continue,
        };
        let Ok(window) = serde_json::from_value::<ClaudeWindow>(value.clone()) else {
            continue;
        };
        observations.push(observation(
            key.clone(),
            duration,
            window.utilization,
            window.resets_at.as_deref(),
        ));
    }
    for limit in usage.limits.iter().flatten() {
        let Some(kind) = limit.kind.as_deref() else {
            continue;
        };
        let (key, duration) = match kind {
            "session" => ("five_hour".to_owned(), FIVE_HOURS),
            "weekly_all" => ("seven_day".to_owned(), SEVEN_DAYS),
            "weekly_scoped" => match limit.scope_key() {
                Some(key) => (key, SEVEN_DAYS),
                None => continue,
            },
            _ => continue,
        };
        if observations
            .iter()
            .any(|existing| existing.window_key == key)
        {
            continue;
        }
        let mut observed = observation(
            key,
            Some(duration),
            limit.percent,
            limit.resets_at.as_deref(),
        );
        observed.label = limit
            .scope
            .as_ref()
            .and_then(|scope| scope.model.as_ref())
            .and_then(|model| model.display_name.clone());
        observations.push(observed);
    }
    observations
}

fn observation(
    window_key: String,
    duration: Option<i64>,
    percent: Option<f64>,
    resets_at: Option<&str>,
) -> QuotaObservation {
    let period_end = resets_at.and_then(iso_to_unix);
    QuotaObservation {
        unit: None,
        reset_behavior: gproxy_channel_api::QuotaResetBehavior::Periodic,
        scope: if matches!(window_key.as_str(), "five_hour" | "seven_day") {
            gproxy_channel_api::QuotaScope::All
        } else {
            gproxy_channel_api::QuotaScope::Unknown
        },
        sample: None,
        window_key,
        label: None,
        period_start: period_end
            .zip(duration)
            .map(|(end, duration)| end - duration),
        period_end,
        used_percent: percent.and_then(|value| Decimal::try_from(value).ok()),
        upstream_used: None,
        upstream_limit: None,
    }
}

fn present(value: &Value, field: &str) -> bool {
    value.get(field).is_some_and(|value| !value.is_null())
}

fn iso_to_unix(value: &str) -> Option<i64> {
    time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
        .ok()
        .map(|stamp| stamp.unix_timestamp())
}

/// One rolling window: `utilization` is a percentage (0–100), `resets_at`
/// an ISO-8601 timestamp.
#[derive(Deserialize)]
struct ClaudeWindow {
    utilization: Option<f64>,
    resets_at: Option<String>,
}

#[derive(Deserialize)]
struct ClaudeUsage {
    #[serde(default)]
    limits: Option<Vec<ClaudeLimit>>,
    #[serde(flatten)]
    windows: serde_json::Map<String, serde_json::Value>,
}

#[derive(Deserialize)]
struct ClaudeLimit {
    kind: Option<String>,
    percent: Option<f64>,
    resets_at: Option<String>,
    scope: Option<ClaudeLimitScope>,
}

#[derive(Deserialize)]
struct ClaudeLimitScope {
    model: Option<ClaudeScopeModel>,
    surface: Option<String>,
}

#[derive(Deserialize)]
struct ClaudeScopeModel {
    id: Option<String>,
    display_name: Option<String>,
}

impl ClaudeLimit {
    fn scope_key(&self) -> Option<String> {
        let scope = self.scope.as_ref()?;
        if let Some(model) = &scope.model {
            let selector = [model.id.as_deref(), model.display_name.as_deref()]
                .into_iter()
                .flatten()
                .map(str::trim)
                .find(|value| !value.is_empty())?;
            return Some(format!("weekly_model:{}", slug(selector)));
        }
        let surface = scope.surface.as_deref().map(str::trim)?;
        (!surface.is_empty()).then(|| format!("weekly_surface:{}", slug(surface)))
    }
}

fn slug(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::parse_probe;

    #[test]
    fn windows_and_scoped_limits_become_observations() {
        let body = json!({
            "five_hour": { "utilization": 34.5, "resets_at": "2026-08-31T15:00:00Z" },
            "seven_day": { "utilization": 61.0, "resets_at": "2026-09-03T00:00:00+00:00" },
            "seven_day_fable": { "utilization": 22.0, "resets_at": "2026-09-03T00:00:00Z" },
            // An unknown key whose fields are explicitly null carries nothing;
            // it used to become a perpetual window with no period or percent.
            "nimbus_quill": { "utilization": null, "resets_at": null },
            "limits": [
                { "kind": "weekly_all", "percent": 99.0, "resets_at": "2026-09-03T00:00:00Z" },
                { "kind": "weekly_scoped", "percent": 12.0, "resets_at": "2026-09-03T00:00:00Z",
                  "scope": { "model": { "id": "claude-opus-5" } } }
            ]
        });
        let observed = parse_probe(http::StatusCode::OK, &serde_json::to_vec(&body).unwrap());
        let keys: Vec<&str> = observed
            .iter()
            .map(|value| value.window_key.as_str())
            .collect();
        assert_eq!(
            keys,
            [
                "five_hour",
                "seven_day",
                "seven_day_fable",
                "weekly_model:claude_opus_5"
            ]
        );
        assert_eq!(observed[0].used_percent, Some("34.5".parse().unwrap()));
        let end = observed[0].period_end.unwrap();
        assert_eq!(observed[0].period_start, Some(end - 5 * 60 * 60));
        // weekly_all duplicated seven_day and was dropped in its favour.
        assert_eq!(observed[1].used_percent, Some("61".parse().unwrap()));
    }
}
