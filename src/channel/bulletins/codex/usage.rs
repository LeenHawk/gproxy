//! Codex per-credential usage — `GET {backend-api}/wham/usage` (the endpoint the
//! CLI's `/status` reads). Returns the `RateLimitStatusPayload`: the account
//! plan, a primary (5h) + secondary (weekly) rate-limit window with a
//! used-percentage and unix reset, and an optional credit balance. The same
//! numbers also ride normal `/responses` calls as `x-codex-*` headers, but the
//! dedicated endpoint gives the full body without spending a turn.

use bytes::Bytes;
use http::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderName, HeaderValue, USER_AGENT};
use http::{HeaderMap, Method, Request, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{headers, model_metadata, token};
use crate::channel::ChannelError;
use crate::channel::http_util::{build_request, join_url};
use crate::channel::usage::{
    RateLimitResetCreditConsumeOutcome, RateLimitResetCreditConsumeResponse, RateLimitResetCredits,
    UsageCredits, UsageSnapshot, UsageWindow,
};

/// Build `GET {base-without-/codex}/wham/usage` with the codex fingerprint.
pub(super) fn request(
    secret: &Value,
    settings: &Value,
) -> Result<Option<Request<Bytes>>, ChannelError> {
    let access_token = token::access_token(secret)?;
    let base = usage_base(settings);
    let uri = join_url(&base, "/wham/usage", None)?;
    let mut req = build_request(Method::GET, uri, HeaderMap::new(), Bytes::new())?;
    apply_headers(&mut req, access_token, secret)?;
    Ok(Some(req))
}

pub(super) fn reset_credit_request(
    secret: &Value,
    settings: &Value,
    idempotency_key: &str,
) -> Result<Option<Request<Bytes>>, ChannelError> {
    let access_token = token::access_token(secret)?;
    let base = usage_base(settings);
    let uri = join_url(&base, "/wham/rate-limit-reset-credits/consume", None)?;
    let body = serde_json::to_vec(&ConsumeRateLimitResetCreditRequest {
        redeem_request_id: idempotency_key,
    })
    .map_err(|e| ChannelError::Build(format!("codex reset-credit request serialize: {e}")))?;
    let mut req = build_request(Method::POST, uri, HeaderMap::new(), Bytes::from(body))?;
    apply_headers(&mut req, access_token, secret)?;
    Ok(Some(req))
}

/// Parse the `/wham/usage` body (`RateLimitStatusPayload`) into a snapshot.
pub(super) fn parse(status: StatusCode, body: &Bytes) -> Option<UsageSnapshot> {
    if !status.is_success() {
        return None;
    }
    let raw: Value = serde_json::from_slice(body).ok()?;
    let payload: RateLimitStatusPayload = serde_json::from_value(raw.clone()).ok()?;

    let mut windows = Vec::new();
    if let Some(rl) = &payload.rate_limit {
        for (name, window) in [
            ("primary", &rl.primary_window),
            ("secondary", &rl.secondary_window),
        ] {
            if let Some(w) = window {
                windows.push(w.to_window(name));
            }
        }
    }
    if let Some(additional) = &payload.additional_rate_limits {
        for (idx, limit) in additional.iter().enumerate() {
            limit.append_windows(idx, &mut windows);
        }
    }
    let credits = payload
        .credits
        .as_ref()
        .map(CreditStatusDetails::to_credits);
    let rate_limit_reset_credits =
        payload
            .rate_limit_reset_credits
            .as_ref()
            .map(|c| RateLimitResetCredits {
                available_count: c.available_count,
            });

    Some(UsageSnapshot {
        plan: payload.plan_type.filter(|s| !s.is_empty()),
        windows,
        credits,
        rate_limit_reset_credits,
        raw,
    })
}

pub(super) fn parse_reset_credit(
    status: StatusCode,
    body: &Bytes,
) -> Option<RateLimitResetCreditConsumeResponse> {
    if !status.is_success() {
        return None;
    }
    let raw: Value = serde_json::from_slice(body).ok()?;
    let payload: ConsumeRateLimitResetCreditResponse = serde_json::from_value(raw.clone()).ok()?;
    Some(RateLimitResetCreditConsumeResponse {
        outcome: payload.code.into(),
        windows_reset: payload.windows_reset,
        raw,
    })
}

fn usage_base(settings: &Value) -> String {
    let base = settings
        .get("base_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(model_metadata::DEFAULT_BASE_URL);
    base.trim_end_matches('/')
        .strip_suffix("/codex")
        .unwrap_or_else(|| base.trim_end_matches('/'))
        .to_string()
}

fn apply_headers(
    req: &mut Request<Bytes>,
    access_token: &str,
    secret: &Value,
) -> Result<(), ChannelError> {
    let bearer = HeaderValue::from_str(&format!("Bearer {access_token}"))
        .map_err(|e| ChannelError::InvalidCredential(format!("bad access_token: {e}")))?;
    let h = req.headers_mut();
    h.insert(AUTHORIZATION, bearer);
    h.insert(ACCEPT, HeaderValue::from_static("application/json"));
    h.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    h.insert(
        USER_AGENT,
        HeaderValue::from_static(headers::USER_AGENT_VALUE),
    );
    h.insert(
        HeaderName::from_static("originator"),
        HeaderValue::from_static(headers::ORIGINATOR),
    );
    if let Some(acct) = token::account_id(secret) {
        let acct = HeaderValue::from_str(acct)
            .map_err(|e| ChannelError::InvalidCredential(format!("bad account_id: {e}")))?;
        h.insert(HeaderName::from_static("chatgpt-account-id"), acct);
    }
    Ok(())
}

#[derive(Deserialize)]
struct RateLimitStatusPayload {
    plan_type: Option<String>,
    rate_limit: Option<RateLimitStatusDetails>,
    additional_rate_limits: Option<Vec<AdditionalRateLimitDetails>>,
    credits: Option<CreditStatusDetails>,
    rate_limit_reset_credits: Option<RateLimitResetCreditsPayload>,
}

#[derive(Deserialize)]
struct RateLimitStatusDetails {
    primary_window: Option<RateLimitWindowSnapshot>,
    secondary_window: Option<RateLimitWindowSnapshot>,
}

/// `used_percent` 0–100, `limit_window_seconds` window length, `reset_at` unix s.
#[derive(Deserialize)]
struct RateLimitWindowSnapshot {
    used_percent: Option<f64>,
    limit_window_seconds: Option<i64>,
    reset_at: Option<i64>,
}

impl RateLimitWindowSnapshot {
    fn to_window(&self, name: &str) -> UsageWindow {
        let mut w = UsageWindow::percent(name, self.used_percent.unwrap_or(0.0));
        if let Some(secs) = self.limit_window_seconds {
            w = w.window_secs(secs);
        }
        if let Some(at) = self.reset_at {
            w = w.resets_unix(at);
        }
        w
    }
}

#[derive(Deserialize)]
struct AdditionalRateLimitDetails {
    limit_name: Option<String>,
    metered_feature: Option<String>,
    rate_limit: Option<RateLimitStatusDetails>,
}

impl AdditionalRateLimitDetails {
    fn append_windows(&self, idx: usize, windows: &mut Vec<UsageWindow>) {
        let Some(rate_limit) = &self.rate_limit else {
            return;
        };
        let key = self
            .metered_feature
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("additional_{idx}"));
        let label = self
            .limit_name
            .as_deref()
            .filter(|s| !s.is_empty())
            .or(self.metered_feature.as_deref())
            .unwrap_or("Additional limit");

        if let Some(window) = &rate_limit.primary_window {
            windows.push(
                window
                    .to_window(&format!("additional_primary:{key}"))
                    .label(label),
            );
        }
        if let Some(window) = &rate_limit.secondary_window {
            windows.push(
                window
                    .to_window(&format!("additional_secondary:{key}"))
                    .label(label),
            );
        }
    }
}

#[derive(Deserialize)]
struct CreditStatusDetails {
    has_credits: Option<bool>,
    unlimited: Option<bool>,
    balance: Option<String>,
}

impl CreditStatusDetails {
    fn to_credits(&self) -> UsageCredits {
        UsageCredits {
            has_credits: self.has_credits,
            unlimited: self.unlimited,
            balance: self.balance.clone(),
            ..Default::default()
        }
    }
}

#[derive(Deserialize)]
struct RateLimitResetCreditsPayload {
    available_count: i64,
}

#[derive(Serialize)]
struct ConsumeRateLimitResetCreditRequest<'a> {
    redeem_request_id: &'a str,
}

#[derive(Deserialize)]
struct ConsumeRateLimitResetCreditResponse {
    code: ConsumeRateLimitResetCreditCode,
    windows_reset: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum ConsumeRateLimitResetCreditCode {
    Reset,
    NothingToReset,
    NoCredit,
    AlreadyRedeemed,
}

impl From<ConsumeRateLimitResetCreditCode> for RateLimitResetCreditConsumeOutcome {
    fn from(value: ConsumeRateLimitResetCreditCode) -> Self {
        match value {
            ConsumeRateLimitResetCreditCode::Reset => Self::Reset,
            ConsumeRateLimitResetCreditCode::NothingToReset => Self::NothingToReset,
            ConsumeRateLimitResetCreditCode::NoCredit => Self::NoCredit,
            ConsumeRateLimitResetCreditCode::AlreadyRedeemed => Self::AlreadyRedeemed,
        }
    }
}
