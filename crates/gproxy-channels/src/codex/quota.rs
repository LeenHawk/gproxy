//! Quota windows from the `x-{limit}` rate-limit response header families —
//! the same families the codex CLI parses (codex-rs/codex-api/src/rate_limits.rs):
//! `x-{limit}-{primary,secondary}-used-percent` (f64, 0–100),
//! `-window-minutes` (i64), `-reset-at` (unix seconds). The default family is
//! `x-codex`; additional metered features ship their own family (e.g.
//! `x-codex-spark-…`). A window only counts when its used-percent parses, and
//! an all-zero window carries no information — both rules mirror the CLI's
//! own `has_data` check.
//!
//! The probe half queries `GET {backend-api}/wham/usage` (what the CLI's
//! `/status` reads): the default windows under `rate_limit`, plus one entry
//! per metered feature under `additional_rate_limits`, each as
//! `{used_percent, limit_window_seconds, reset_at}` objects.

use bytes::Bytes;
use gproxy_channel_api::{
    ChannelError, QuotaObservation, QuotaResetCredits, QuotaResetOutcome, QuotaResetResult,
};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::Value;

pub(super) fn from_headers(headers: &http::HeaderMap) -> Vec<QuotaObservation> {
    let mut limits = vec!["codex"];
    for name in headers.keys() {
        if let Some(limit) = name
            .as_str()
            .strip_prefix("x-")
            .and_then(|name| name.strip_suffix("-primary-used-percent"))
            && limit != "codex"
        {
            limits.push(limit);
        }
    }
    limits.sort_unstable();
    limits.dedup();
    limits
        .into_iter()
        .flat_map(|limit| {
            ["primary", "secondary"]
                .into_iter()
                .filter_map(move |window| observe(headers, limit, window))
        })
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
    let mut observations = Vec::new();
    if let Some(rate_limit) = payload.rate_limit {
        push_windows(&mut observations, rate_limit, None);
    }
    for (index, additional) in payload
        .additional_rate_limits
        .unwrap_or_default()
        .into_iter()
        .enumerate()
    {
        let Some(rate_limit) = additional.rate_limit else {
            continue;
        };
        // `metered_feature` is the limit id the CLI keys snapshots by; the
        // header family for the same limit converges on the same stable key.
        let feature = [&additional.metered_feature, &additional.limit_name]
            .into_iter()
            .map(|value| stable_key(value))
            .find(|key| !key.is_empty())
            .unwrap_or_else(|| format!("additional_{index}"));
        let label = Some(additional.limit_name)
            .filter(|name| !name.is_empty())
            .or_else(|| Some(additional.metered_feature).filter(|name| !name.is_empty()));
        push_windows(&mut observations, rate_limit, Some((&feature, label)));
    }
    observations
}

fn push_windows(
    observations: &mut Vec<QuotaObservation>,
    rate_limit: ProbeRateLimit,
    feature: Option<(&str, Option<String>)>,
) {
    let windows = [
        ("primary", rate_limit.primary_window),
        ("secondary", rate_limit.secondary_window),
    ];
    for (window, snapshot) in windows {
        let Some(snapshot) = snapshot else { continue };
        let (window_key, label) = match &feature {
            None => (window.to_owned(), None),
            Some((feature, label)) => (format!("additional_{window}:{feature}"), label.clone()),
        };
        let period_end = snapshot.reset_at;
        let period_start = period_end
            .zip(snapshot.limit_window_seconds.filter(|seconds| *seconds > 0))
            .map(|(end, seconds)| end - seconds);
        observations.push(QuotaObservation {
            window_key,
            label,
            period_start,
            period_end,
            used_percent: snapshot
                .used_percent
                .and_then(|value| Decimal::try_from(value).ok()),
            upstream_used: None,
            upstream_limit: None,
        });
    }
}

fn stable_key(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_end_matches('_').to_owned()
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
        .header(http::header::USER_AGENT, super::auth::fallback_user_agent())
        .header("originator", super::auth::ORIGINATOR)
        .header("version", super::auth::VERSION);
    if let Some(account_id) = super::auth::account_id(secret) {
        builder = builder.header("chatgpt-account-id", account_id);
    }
    if secret
        .get("chatgpt_account_is_fedramp")
        .and_then(Value::as_bool)
        == Some(true)
    {
        builder = builder.header("x-openai-fedramp", "true");
    }
    builder
}

#[derive(Deserialize)]
struct ProbePayload {
    rate_limit: Option<ProbeRateLimit>,
    additional_rate_limits: Option<Vec<ProbeAdditionalLimit>>,
}

#[derive(Deserialize)]
struct ProbeAdditionalLimit {
    #[serde(default)]
    limit_name: String,
    #[serde(default)]
    metered_feature: String,
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

fn observe(headers: &http::HeaderMap, limit: &str, window: &str) -> Option<QuotaObservation> {
    let prefix = format!("x-{limit}-{window}");
    let used_percent = float(headers, &format!("{prefix}-used-percent"))?;
    let reset_at = integer(headers, &format!("{prefix}-reset-at"));
    let window_minutes = integer(headers, &format!("{prefix}-window-minutes"));
    if used_percent == 0.0 && reset_at.is_none() && window_minutes.unwrap_or(0) == 0 {
        return None;
    }
    let period_start = reset_at
        .zip(window_minutes.filter(|minutes| *minutes > 0))
        .map(|(end, minutes)| end - minutes * 60);
    let (window_key, label) = if limit == "codex" {
        (window.to_owned(), None)
    } else {
        (
            format!("additional_{window}:{}", stable_key(limit)),
            text(headers, &format!("x-{limit}-limit-name"))
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_owned),
        )
    };
    Some(QuotaObservation {
        window_key,
        label,
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
