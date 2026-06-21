//! Codex per-credential usage — `GET {backend-api}/wham/usage` (the endpoint the
//! CLI's `/status` reads). Returns the `RateLimitStatusPayload`: the account
//! plan, a primary (5h) + secondary (weekly) rate-limit window with a
//! used-percentage and unix reset, and an optional credit balance. The same
//! numbers also ride normal `/responses` calls as `x-codex-*` headers, but the
//! dedicated endpoint gives the full body without spending a turn.

use bytes::Bytes;
use http::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderName, HeaderValue, USER_AGENT};
use http::{HeaderMap, Method, Request, StatusCode};
use serde::Deserialize;
use serde_json::Value;

use super::auth;
use crate::channel::ChannelError;
use crate::channel::http_util::{build_request, join_url};
use crate::channel::usage::{UsageCredits, UsageSnapshot, UsageWindow};

/// Build `GET {base-without-/codex}/wham/usage` with the codex fingerprint.
pub(super) fn request(
    secret: &Value,
    settings: &Value,
) -> Result<Option<Request<Bytes>>, ChannelError> {
    let access_token = auth::access_token(secret)?;
    let base = settings
        .get("base_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(auth::DEFAULT_BASE_URL);
    // The usage endpoint sits one level up from the codex responses base.
    let base = base.trim_end_matches('/');
    let base = base.strip_suffix("/codex").unwrap_or(base);

    let uri = join_url(base, "/wham/usage", None)?;
    let mut req = build_request(Method::GET, uri, HeaderMap::new(), Bytes::new())?;
    let bearer = HeaderValue::from_str(&format!("Bearer {access_token}"))
        .map_err(|e| ChannelError::InvalidCredential(format!("bad access_token: {e}")))?;
    let h = req.headers_mut();
    h.insert(AUTHORIZATION, bearer);
    h.insert(ACCEPT, HeaderValue::from_static("application/json"));
    h.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    h.insert(USER_AGENT, HeaderValue::from_static(auth::USER_AGENT_VALUE));
    h.insert(
        HeaderName::from_static("originator"),
        HeaderValue::from_static(auth::ORIGINATOR),
    );
    if let Some(acct) = auth::account_id(secret) {
        let acct = HeaderValue::from_str(acct)
            .map_err(|e| ChannelError::InvalidCredential(format!("bad account_id: {e}")))?;
        h.insert(HeaderName::from_static("chatgpt-account-id"), acct);
    }
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
    let credits = payload
        .credits
        .as_ref()
        .map(CreditStatusDetails::to_credits);

    Some(UsageSnapshot {
        plan: payload.plan_type.filter(|s| !s.is_empty()),
        windows,
        credits,
        raw,
    })
}

#[derive(Deserialize)]
struct RateLimitStatusPayload {
    plan_type: Option<String>,
    rate_limit: Option<RateLimitStatusDetails>,
    credits: Option<CreditStatusDetails>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rate_limit_payload() {
        let body = Bytes::from_static(
            br#"{
              "plan_type": "pro",
              "rate_limit": {
                "allowed": true, "limit_reached": false,
                "primary_window": {"used_percent": 42, "limit_window_seconds": 300, "reset_at": 1704069000},
                "secondary_window": {"used_percent": 84, "limit_window_seconds": 604800, "reset_at": 1704074400}
              },
              "credits": {"has_credits": true, "unlimited": false, "balance": "9.99"}
            }"#,
        );
        let snap = parse(StatusCode::OK, &body).expect("snapshot");
        assert_eq!(snap.plan.as_deref(), Some("pro"));
        assert_eq!(snap.windows.len(), 2);
        assert_eq!(snap.windows[0].name, "primary");
        assert_eq!(snap.windows[0].used_percent, Some(42.0));
        assert_eq!(snap.windows[0].window_seconds, Some(300));
        assert_eq!(snap.windows[0].resets_at_unix, Some(1704069000));
        let credits = snap.credits.expect("credits");
        assert_eq!(credits.has_credits, Some(true));
        assert_eq!(credits.balance.as_deref(), Some("9.99"));
    }
}
