//! What an upstream refusal says about the credential that received it.
//!
//! The host reads a bounded prefix of every non-2xx body (and the payload of
//! an error frame that opens a 2xx stream) and asks the channel what it means
//! before deciding whether to block the credential, for how long and for which
//! models, and whether to fail over. The standard reading inspects structured
//! fields only (OpenAI `error.code`/`type`, Anthropic error types, Google
//! `error.status` and `details[]`); a channel whose upstream writes the
//! decisive fact in prose overrides [`BaseChannel::classify_rejection`] and
//! keeps that text matching to itself.
//!
//! [`BaseChannel::classify_rejection`]: super::BaseChannel::classify_rejection

use gproxy_protocol::OperationKey;
use http::{HeaderMap, StatusCode};
use serde_json::Value;
use std::time::Duration;

/// One refusal, as seen by the host's attempt loop.
#[derive(Clone, Copy)]
pub struct RejectionContext<'a> {
    /// The answer's status; for an error frame opening a 2xx stream, the
    /// status that frame's error stands for.
    pub status: StatusCode,
    pub headers: &'a HeaderMap,
    /// A bounded prefix of the body, or the error frame's JSON payload.
    pub body: &'a [u8],
    pub operation: OperationKey,
    pub upstream_model: Option<&'a str>,
    /// Wall clock, for absolute reset times (`Retry-After` dates, timestamps).
    pub now_ms: i64,
}

/// What kind of refusal it was. The host derives the block from the kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectionKind {
    /// Too many requests for now; recovers by itself shortly.
    RateLimited,
    /// A usage allowance is spent until it resets.
    QuotaExhausted,
    /// The account is out of balance or billing is not active: does not
    /// recover until someone pays, so it is blocked long.
    Credit,
    /// The credential is not (or no longer) authenticated or permitted.
    Auth,
    /// This credential does not serve the requested model.
    ModelUnserved,
    /// The request content itself is at fault: no credential would answer it
    /// differently. Returned to the caller as-is; the credential is not blocked.
    Request,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rejection {
    pub kind: RejectionKind,
    /// How long until the credential (or the model) may be asked again, when
    /// the upstream said so. The host falls back to `Retry-After`, then to its
    /// own default for the kind.
    pub retry_after: Option<Duration>,
    /// The refusal concerns only the requested model, not the whole credential.
    pub model_scoped: bool,
}

impl Rejection {
    pub fn new(kind: RejectionKind) -> Self {
        Self {
            kind,
            retry_after: None,
            model_scoped: false,
        }
    }
}

/// The error object of a refusal body: OpenAI/Anthropic/Google `{"error":{..}}`,
/// a Responses `response.failed` event, a realtime `{"type":"error",..}` or the
/// first element of a Gemini JSON-array stream.
pub fn error_object(value: &Value) -> Option<&Value> {
    if let Some(first) = value.as_array().and_then(|items| items.first()) {
        return error_object(first);
    }
    if let Some(error) = value.get("error").filter(|e| e.is_object()) {
        return Some(error);
    }
    if let Some(error) = value.pointer("/response/error").filter(|e| e.is_object()) {
        return Some(error);
    }
    (value.get("type").and_then(Value::as_str) == Some("error")).then_some(value)
}

/// The standard reading of a refusal from structured fields only. `None`
/// when the body names nothing the host does not already infer from status.
pub fn standard_rejection(context: RejectionContext<'_>) -> Option<Rejection> {
    let value: Value = serde_json::from_slice(context.body).ok()?;
    let error = error_object(&value)?;
    let details = error
        .get("details")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let retry_delay = details
        .iter()
        .filter(|d| type_is(d, "google.rpc.RetryInfo"))
        .find_map(|d| d.get("retryDelay").and_then(proto_duration));
    let info = details.iter().find(|d| type_is(d, "google.rpc.ErrorInfo"));
    let reason = info.and_then(|i| i.get("reason")).and_then(Value::as_str);
    let metadata = info.and_then(|i| i.get("metadata"));
    let reset_delay = metadata
        .and_then(|m| m.get("quotaResetDelay"))
        .and_then(proto_duration)
        .or_else(|| {
            metadata
                .and_then(|m| m.get("quotaResetTimeStamp"))
                .and_then(Value::as_str)
                .and_then(rfc3339_ms)
                .and_then(|at| until(at, context.now_ms))
        });
    // Google writes per-model allowances with the model in the error's
    // metadata or in the violated quota's dimensions.
    let google_model_scoped = metadata.and_then(|m| m.get("model")).is_some()
        || details.iter().any(|d| {
            type_is(d, "google.rpc.QuotaFailure")
                && d.get("violations")
                    .and_then(Value::as_array)
                    .is_some_and(|violations| {
                        violations
                            .iter()
                            .any(|v| v.pointer("/quotaDimensions/model").is_some())
                    })
        });
    if let Some(reason) = reason {
        let kind = match reason {
            "QUOTA_EXHAUSTED" => Some(RejectionKind::QuotaExhausted),
            "RATE_LIMIT_EXCEEDED" => Some(RejectionKind::RateLimited),
            "API_KEY_INVALID"
            | "API_KEY_SERVICE_BLOCKED"
            | "ACCESS_TOKEN_EXPIRED"
            | "ACCESS_TOKEN_SCOPE_INSUFFICIENT"
            | "SERVICE_DISABLED" => Some(RejectionKind::Auth),
            "BILLING_DISABLED" => Some(RejectionKind::Credit),
            _ => None,
        };
        if let Some(kind) = kind {
            return Some(Rejection {
                kind,
                retry_after: reset_delay.or(retry_delay),
                model_scoped: google_model_scoped,
            });
        }
    }
    let mut codes = ["code", "type", "status"]
        .into_iter()
        .filter_map(|key| error.get(key).and_then(Value::as_str));
    let kind = codes.find_map(code_kind)?;
    if kind == RejectionKind::Request && !context.status.is_client_error() {
        return None;
    }
    let google = error.get("status").and_then(Value::as_str) == Some("RESOURCE_EXHAUSTED");
    Some(Rejection {
        kind,
        retry_after: reset_delay.or(retry_delay),
        model_scoped: kind == RejectionKind::ModelUnserved || (google && google_model_scoped),
    })
}

fn code_kind(code: &str) -> Option<RejectionKind> {
    use RejectionKind::*;
    Some(match code {
        "rate_limit_error"
        | "rate_limit_exceeded"
        | "RESOURCE_EXHAUSTED"
        | "ThrottlingException"
        | "throttlingException"
        | "too_many_requests" => RateLimited,
        "quota_exceeded"
        | "usage_limit_reached"
        | "usage_limit_exceeded"
        | "ServiceQuotaExceededException" => QuotaExhausted,
        "insufficient_quota"
        | "billing_error"
        | "billing_hard_limit_reached"
        | "billing_not_active"
        | "credit_balance_too_low"
        | "insufficient_balance"
        | "payment_required" => Credit,
        "authentication_error"
        | "invalid_api_key"
        | "UNAUTHENTICATED"
        | "permission_error"
        | "PERMISSION_DENIED" => Auth,
        "model_not_found" => ModelUnserved,
        "invalid_request_error"
        | "invalid_prompt"
        | "context_length_exceeded"
        | "request_too_large"
        | "INVALID_ARGUMENT"
        | "FAILED_PRECONDITION"
        | "OUT_OF_RANGE"
        | "ValidationException"
        | "validationException" => Request,
        _ => return None,
    })
}

fn type_is(detail: &Value, name: &str) -> bool {
    detail
        .get("@type")
        .and_then(Value::as_str)
        .is_some_and(|t| t.rsplit('/').next() == Some(name))
}

fn until(at_ms: i64, now_ms: i64) -> Option<Duration> {
    (at_ms > now_ms).then(|| Duration::from_millis((at_ms - now_ms) as u64))
}

/// A protobuf JSON duration (`"3.5s"`) or a Go duration (`"3h19m30s"`,
/// `"250ms"`) as Google and Code Assist write them.
pub fn proto_duration(value: &Value) -> Option<Duration> {
    go_duration(value.as_str()?)
}

/// `[0-9.]+(h|m|s|ms)` repeated, as Go's `time.Duration.String` prints.
pub fn go_duration(text: &str) -> Option<Duration> {
    let mut rest = text.trim();
    if rest.is_empty() {
        return None;
    }
    let mut total = 0f64;
    while !rest.is_empty() {
        let digits = rest
            .find(|c: char| !(c.is_ascii_digit() || c == '.'))
            .filter(|&end| end > 0)?;
        let number: f64 = rest[..digits].parse().ok()?;
        rest = &rest[digits..];
        let (unit, len) = if rest.starts_with("ms") {
            (0.001, 2)
        } else if rest.starts_with('h') {
            (3600.0, 1)
        } else if rest.starts_with('m') {
            (60.0, 1)
        } else if rest.starts_with('s') {
            (1.0, 1)
        } else {
            return None;
        };
        total += number * unit;
        rest = &rest[len..];
    }
    (total.is_finite() && total >= 0.0).then(|| Duration::from_secs_f64(total))
}

/// `Retry-After` as delta-seconds or an HTTP-date (RFC 9110 §10.2.3).
pub fn retry_after(headers: &HeaderMap, now_ms: i64) -> Option<Duration> {
    let value = headers
        .get(http::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim();
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    let at = http_date_ms(value)?;
    Some(until(at, now_ms).unwrap_or_default())
}

/// IMF-fixdate (`Sun, 06 Nov 1994 08:49:37 GMT`), the form RFC 9110 requires
/// senders to use.
fn http_date_ms(value: &str) -> Option<i64> {
    let (_, rest) = value.split_once(", ")?;
    let mut parts = rest.split_ascii_whitespace();
    let day: u32 = parts.next()?.parse().ok()?;
    let month = match parts.next()? {
        "Jan" => 1,
        "Feb" => 2,
        "Mar" => 3,
        "Apr" => 4,
        "May" => 5,
        "Jun" => 6,
        "Jul" => 7,
        "Aug" => 8,
        "Sep" => 9,
        "Oct" => 10,
        "Nov" => 11,
        "Dec" => 12,
        _ => return None,
    };
    let year: i64 = parts.next()?.parse().ok()?;
    let (h, m, s) = clock(parts.next()?)?;
    if parts.next()? != "GMT" {
        return None;
    }
    epoch_ms(year, month, day, h, m, s, 0)
}

/// `2025-01-02T03:04:05.678Z` or with a numeric offset.
fn rfc3339_ms(value: &str) -> Option<i64> {
    let (date, time) = value.split_once(['T', 't', ' '])?;
    let mut ymd = date.split('-');
    let year: i64 = ymd.next()?.parse().ok()?;
    let month: u32 = ymd.next()?.parse().ok()?;
    let day: u32 = ymd.next()?.parse().ok()?;
    let (clock_part, offset_ms) = if let Some(clock_part) = time.strip_suffix(['Z', 'z']) {
        (clock_part, 0)
    } else {
        let at = time.rfind(['+', '-'])?;
        let (clock_part, offset) = time.split_at(at);
        let sign = if offset.starts_with('-') { -1 } else { 1 };
        let (oh, om) = offset[1..].split_once(':')?;
        let minutes = oh.parse::<i64>().ok()? * 60 + om.parse::<i64>().ok()?;
        (clock_part, sign * minutes * 60_000)
    };
    let (whole, fraction) = clock_part.split_once('.').unwrap_or((clock_part, ""));
    let (h, m, s) = clock(whole)?;
    let millis = format!("{fraction:0<3}")
        .get(..3)
        .and_then(|ms| ms.parse::<u32>().ok())?;
    Some(epoch_ms(year, month, day, h, m, s, millis)? - offset_ms)
}

fn clock(value: &str) -> Option<(u32, u32, u32)> {
    let mut parts = value.split(':');
    let h = parts.next()?.parse().ok()?;
    let m = parts.next()?.parse().ok()?;
    let s = parts.next()?.parse().ok()?;
    (parts.next().is_none() && h < 24 && m < 60 && s <= 60).then_some((h, m, s))
}

fn epoch_ms(year: i64, month: u32, day: u32, h: u32, m: u32, s: u32, ms: u32) -> Option<i64> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    // Howard Hinnant's days_from_civil.
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (i64::from(month) + 9) % 12;
    let doy = (153 * mp + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(
        ((days * 86_400 + i64::from(h) * 3600 + i64::from(m) * 60 + i64::from(s)) * 1000)
            + i64::from(ms),
    )
}
