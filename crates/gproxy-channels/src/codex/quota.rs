//! Quota windows from the `x-codex` rate-limit response headers — the same
//! family the codex CLI itself parses (codex-rs/codex-api/src/rate_limits.rs):
//! `x-codex-{primary,secondary}-used-percent` (f64, 0–100),
//! `-window-minutes` (i64), `-reset-at` (unix seconds). A window only counts
//! when its used-percent parses, and an all-zero window carries no
//! information — both rules mirror the CLI's own `has_data` check.
//!
//! The probe half queries `GET {backend-api}/wham/usage` (what the CLI's
//! `/status` reads): the same primary/secondary windows as the headers, as
//! `{used_percent, limit_window_seconds, reset_at}` objects.

use bytes::Bytes;
use gproxy_channel_api::{
    ChannelError, QuotaObservation, QuotaResetCredits, QuotaResetOutcome, QuotaResetResult,
};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::Value;

pub(super) fn from_headers(headers: &http::HeaderMap) -> Vec<QuotaObservation> {
    ["primary", "secondary"]
        .into_iter()
        .filter_map(|window| observe(headers, window))
        .collect()
}

pub(super) fn probe_request(
    secret: &Value,
    settings: &Value,
) -> Result<Option<http::Request<Bytes>>, ChannelError> {
    let token = super::auth::access_token(secret)?;
    // The usage endpoint lives beside the codex API root, not under it.
    let uri = crate::shared::http::join(&backend_base(settings), "/wham/usage", None)?;
    authenticated(http::Request::get(uri), token, secret)
        .body(Bytes::new())
        .map(Some)
        .map_err(|error| ChannelError::Prepare(error.to_string()))
}

pub(super) fn credits_probe_request(
    secret: &Value,
    settings: &Value,
) -> Result<Option<http::Request<Bytes>>, ChannelError> {
    let token = super::auth::access_token(secret)?;
    let uri = crate::shared::http::join(
        &backend_base(settings),
        "/wham/rate-limit-reset-credits",
        None,
    )?;
    authenticated(http::Request::get(uri), token, secret)
        .body(Bytes::new())
        .map(Some)
        .map_err(|error| ChannelError::Prepare(error.to_string()))
}

pub(super) fn reset_request(
    secret: &Value,
    settings: &Value,
    redeem_request_id: &str,
) -> Result<Option<http::Request<Bytes>>, ChannelError> {
    let token = super::auth::access_token(secret)?;
    let uri = crate::shared::http::join(
        &backend_base(settings),
        "/wham/rate-limit-reset-credits/consume",
        None,
    )?;
    let body = serde_json::to_vec(&ResetRequest { redeem_request_id })
        .map_err(|error| ChannelError::Prepare(error.to_string()))?;
    authenticated(http::Request::post(uri), token, secret)
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(Bytes::from(body))
        .map(Some)
        .map_err(|error| ChannelError::Prepare(error.to_string()))
}

pub(super) fn parse_probe(status: http::StatusCode, body: &[u8]) -> Vec<QuotaObservation> {
    if !status.is_success() {
        return Vec::new();
    }
    let Ok(payload) = serde_json::from_slice::<ProbePayload>(body) else {
        return Vec::new();
    };
    let Some(rate_limit) = payload.rate_limit else {
        return Vec::new();
    };
    [
        ("primary", rate_limit.primary_window),
        ("secondary", rate_limit.secondary_window),
    ]
    .into_iter()
    .filter_map(|(window_key, window)| {
        let window = window?;
        let period_end = window.reset_at;
        let period_start = period_end
            .zip(window.limit_window_seconds.filter(|seconds| *seconds > 0))
            .map(|(end, seconds)| end - seconds);
        Some(QuotaObservation {
            window_key: window_key.to_owned(),
            period_start,
            period_end,
            used_percent: window
                .used_percent
                .and_then(|value| Decimal::try_from(value).ok()),
            upstream_used: None,
            upstream_limit: None,
        })
    })
    .collect()
}

pub(super) fn parse_probe_credits(
    status: http::StatusCode,
    body: &[u8],
) -> Option<QuotaResetCredits> {
    if !status.is_success() {
        return None;
    }
    // The credits endpoint returns per-credit detail; the usage summary only
    // a count. Available credits carry `status: "available"`.
    if let Ok(details) = serde_json::from_slice::<CreditsDetails>(body) {
        let expires_at = details
            .credits
            .iter()
            .filter(|credit| credit.status.as_deref() == Some("available"))
            .filter_map(|credit| credit.expires_at.as_deref())
            .filter_map(crate::shared::quota::iso_to_unix)
            .min();
        return Some(QuotaResetCredits {
            available_count: details.available_count,
            expires_at,
        });
    }
    let payload = serde_json::from_slice::<ProbeCreditsEnvelope>(body).ok()?;
    let credits = payload.rate_limit_reset_credits?;
    Some(QuotaResetCredits {
        available_count: credits.available_count,
        expires_at: None,
    })
}

pub(super) fn parse_reset(status: http::StatusCode, body: &[u8]) -> Option<QuotaResetResult> {
    if !status.is_success() {
        return None;
    }
    let payload = serde_json::from_slice::<ResetResponse>(body).ok()?;
    Some(QuotaResetResult {
        outcome: match payload.code {
            ResetCode::Reset => QuotaResetOutcome::Reset,
            ResetCode::NothingToReset => QuotaResetOutcome::NothingToReset,
            ResetCode::NoCredit => QuotaResetOutcome::NoCredit,
            ResetCode::AlreadyRedeemed => QuotaResetOutcome::AlreadyRedeemed,
        },
        windows_reset: payload.windows_reset,
    })
}

fn backend_base(settings: &Value) -> String {
    let base = settings
        .get("base_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|base| !base.is_empty())
        .unwrap_or(super::auth::DEFAULT_BASE_URL);
    base.trim_end_matches('/')
        .strip_suffix("/codex")
        .unwrap_or_else(|| base.trim_end_matches('/'))
        .to_owned()
}

fn authenticated(
    mut builder: http::request::Builder,
    token: &str,
    secret: &Value,
) -> http::request::Builder {
    builder = builder
        .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
        .header(http::header::ACCEPT, "application/json")
        .header(http::header::USER_AGENT, super::auth::USER_AGENT)
        .header("originator", super::auth::ORIGINATOR);
    if let Some(account_id) = super::auth::account_id(secret) {
        builder = builder.header("chatgpt-account-id", account_id);
    }
    builder
}

#[derive(Deserialize)]
struct ProbePayload {
    rate_limit: Option<ProbeRateLimit>,
}

#[derive(Deserialize)]
struct ProbeCreditsEnvelope {
    rate_limit_reset_credits: Option<ProbeCredits>,
}

#[derive(Deserialize)]
struct ProbeCredits {
    available_count: i64,
}

#[derive(Deserialize)]
struct CreditsDetails {
    credits: Vec<CreditDetail>,
    available_count: i64,
}

#[derive(Deserialize)]
struct CreditDetail {
    status: Option<String>,
    expires_at: Option<String>,
}

#[derive(serde::Serialize)]
struct ResetRequest<'a> {
    redeem_request_id: &'a str,
}

#[derive(Deserialize)]
struct ResetResponse {
    code: ResetCode,
    windows_reset: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum ResetCode {
    Reset,
    NothingToReset,
    NoCredit,
    AlreadyRedeemed,
}

#[derive(Deserialize)]
struct ProbeRateLimit {
    primary_window: Option<ProbeWindow>,
    secondary_window: Option<ProbeWindow>,
}

/// `used_percent` 0–100, `limit_window_seconds` window length, `reset_at` unix s.
#[derive(Deserialize)]
struct ProbeWindow {
    used_percent: Option<f64>,
    limit_window_seconds: Option<i64>,
    reset_at: Option<i64>,
}

fn observe(headers: &http::HeaderMap, window: &str) -> Option<QuotaObservation> {
    let used_percent = float(headers, &format!("x-codex-{window}-used-percent"))?;
    let reset_at = integer(headers, &format!("x-codex-{window}-reset-at"));
    let window_minutes = integer(headers, &format!("x-codex-{window}-window-minutes"));
    if used_percent == 0.0 && reset_at.is_none() && window_minutes.unwrap_or(0) == 0 {
        return None;
    }
    let period_start = reset_at
        .zip(window_minutes.filter(|minutes| *minutes > 0))
        .map(|(end, minutes)| end - minutes * 60);
    Some(QuotaObservation {
        window_key: window.to_owned(),
        period_start,
        period_end: reset_at,
        used_percent: Decimal::try_from(used_percent).ok(),
        upstream_used: None,
        upstream_limit: None,
    })
}

fn float(headers: &http::HeaderMap, name: &str) -> Option<f64> {
    text(headers, name)?
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
}

fn integer(headers: &http::HeaderMap, name: &str) -> Option<i64> {
    text(headers, name)?.parse().ok()
}

fn text<'a>(headers: &'a http::HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name)?.to_str().ok()
}

#[cfg(test)]
mod tests {
    use gproxy_channel_api::QuotaResetOutcome;

    use super::{from_headers, parse_probe, parse_probe_credits, parse_reset};

    fn headers(pairs: &[(&str, &str)]) -> http::HeaderMap {
        pairs
            .iter()
            .map(|(name, value)| {
                (
                    http::HeaderName::try_from(*name).unwrap(),
                    http::HeaderValue::from_str(value).unwrap(),
                )
            })
            .collect()
    }

    #[test]
    fn reads_both_windows_and_derives_period_start() {
        let observed = from_headers(&headers(&[
            ("x-codex-primary-used-percent", "37.5"),
            ("x-codex-primary-window-minutes", "300"),
            ("x-codex-primary-reset-at", "1756900000"),
            ("x-codex-secondary-used-percent", "12"),
            ("x-codex-secondary-window-minutes", "10080"),
            ("x-codex-secondary-reset-at", "1757300000"),
        ]));
        assert_eq!(observed.len(), 2);
        assert_eq!(observed[0].window_key, "primary");
        assert_eq!(observed[0].period_end, Some(1_756_900_000));
        assert_eq!(observed[0].period_start, Some(1_756_900_000 - 300 * 60));
        assert_eq!(observed[0].used_percent, Some("37.5".parse().unwrap()));
        assert_eq!(observed[1].window_key, "secondary");
        assert_eq!(observed[1].period_start, Some(1_757_300_000 - 10_080 * 60));
    }

    #[test]
    fn skips_missing_percent_and_empty_windows() {
        assert!(from_headers(&headers(&[("x-codex-primary-reset-at", "1756900000")])).is_empty());
        assert!(from_headers(&headers(&[("x-codex-primary-used-percent", "0")])).is_empty());
        let observed = from_headers(&headers(&[
            ("x-codex-primary-used-percent", "0"),
            ("x-codex-primary-reset-at", "1756900000"),
        ]));
        assert_eq!(observed.len(), 1);
        assert_eq!(observed[0].period_start, None);
    }

    #[test]
    fn probe_body_yields_windows_and_reset_credit_count() {
        let body = br#"{
            "rate_limit": {
                "primary_window": {
                    "used_percent": 42,
                    "limit_window_seconds": 18000,
                    "reset_at": 1756900000
                }
            },
            "rate_limit_reset_credits": {"available_count": 3}
        }"#;
        let windows = parse_probe(http::StatusCode::OK, body);
        let credits = parse_probe_credits(http::StatusCode::OK, body).unwrap();
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].period_start, Some(1_756_900_000 - 18_000));
        assert_eq!(credits.available_count, 3);
    }

    #[test]
    fn credit_details_carry_the_soonest_available_expiry() {
        let body = serde_json::json!({
            "credits": [
                { "id": "c1", "status": "available", "expires_at": "2026-09-20T00:00:00Z" },
                { "id": "c2", "status": "available", "expires_at": "2026-09-10T00:00:00Z" },
                { "id": "c3", "status": "redeemed", "expires_at": "2026-09-01T00:00:00Z" }
            ],
            "available_count": 2
        });
        let credits =
            super::parse_probe_credits(http::StatusCode::OK, &serde_json::to_vec(&body).unwrap())
                .unwrap();
        assert_eq!(credits.available_count, 2);
        assert_eq!(
            credits.expires_at,
            crate::shared::quota::iso_to_unix("2026-09-10T00:00:00Z")
        );
    }

    #[test]
    fn parses_every_reset_outcome() {
        let cases = [
            ("reset", QuotaResetOutcome::Reset),
            ("nothing_to_reset", QuotaResetOutcome::NothingToReset),
            ("no_credit", QuotaResetOutcome::NoCredit),
            ("already_redeemed", QuotaResetOutcome::AlreadyRedeemed),
        ];
        for (code, expected) in cases {
            let body = serde_json::to_vec(&serde_json::json!({
                "code": code,
                "windows_reset": 2
            }))
            .unwrap();
            let result = parse_reset(http::StatusCode::OK, &body).unwrap();
            assert_eq!(result.outcome, expected);
            assert_eq!(result.windows_reset, Some(2));
        }
        assert!(parse_reset(http::StatusCode::BAD_REQUEST, b"{}").is_none());
    }
}
