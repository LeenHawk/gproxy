//! Upstream response classification (§6.4).

use std::time::Duration;

use http::{HeaderMap, StatusCode};

/// 5-state classification driving failover + cooldown + credential health +
/// billing (§6.4). Same shape as v1's `ResponseClassification`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Disposition {
    /// 2xx — return; mark credential healthy; bill (§17).
    Success,
    /// 401/402/403 — refresh once; still dead → mark dead, next credential.
    AuthDead,
    /// 429 — with `retry_after`: cool this credential + switch; else limited
    /// same-credential retries.
    RateLimited { retry_after: Option<Duration> },
    /// 5xx / network — next credential.
    Transient,
    /// 4xx validation — return immediately, no retry.
    Permanent,
}

impl Disposition {
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Success)
    }

    /// Whether failover should advance to the next candidate.
    pub fn should_failover(&self) -> bool {
        matches!(
            self,
            Self::AuthDead | Self::RateLimited { .. } | Self::Transient
        )
    }

    /// Generic HTTP-status → disposition mapping shared by all channels (the
    /// `Channel::classify` default). Channels override only if they need
    /// provider-specific signals (e.g. a 200 body carrying an error envelope).
    pub fn from_http(status: StatusCode, headers: &HeaderMap) -> Self {
        let code = status.as_u16();
        match code {
            200..=299 => Self::Success,
            401..=403 => Self::AuthDead,
            429 => Self::RateLimited {
                retry_after: parse_retry_after(headers),
            },
            500..=599 => Self::Transient,
            _ => Self::Permanent,
        }
    }
}

/// Parse a `Retry-After` header into a `Duration`. Accepts both forms (RFC 7231):
/// a delay in seconds (`Retry-After: 120`) and an HTTP-date
/// (`Retry-After: Wed, 21 Oct 2025 07:28:00 GMT`), the latter converted to a
/// delay from the current time. A past date or unparseable value → `None`.
fn parse_retry_after(headers: &HeaderMap) -> Option<Duration> {
    let val = headers
        .get(http::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim();

    // delta-seconds form
    if let Ok(secs) = val.parse::<u64>() {
        return Some(Duration::from_secs(secs));
    }

    // HTTP-date form → delay from now (dual-target clock; no SystemTime::now,
    // so this stays wasm-safe).
    let target = httpdate::parse_http_date(val)
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs() as i64;
    let now = crate::util::time::unix_now();
    (target > now).then(|| Duration::from_secs((target - now) as u64))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_mapping() {
        let h = HeaderMap::new();
        assert_eq!(
            Disposition::from_http(StatusCode::OK, &h),
            Disposition::Success
        );
        assert_eq!(
            Disposition::from_http(StatusCode::UNAUTHORIZED, &h),
            Disposition::AuthDead
        );
        assert_eq!(
            Disposition::from_http(StatusCode::BAD_GATEWAY, &h),
            Disposition::Transient
        );
        assert_eq!(
            Disposition::from_http(StatusCode::BAD_REQUEST, &h),
            Disposition::Permanent
        );
        assert!(Disposition::from_http(StatusCode::OK, &h).is_success());
    }

    #[test]
    fn retry_after_parsed() {
        let mut h = HeaderMap::new();
        h.insert(http::header::RETRY_AFTER, "12".parse().unwrap());
        assert_eq!(
            Disposition::from_http(StatusCode::TOO_MANY_REQUESTS, &h),
            Disposition::RateLimited {
                retry_after: Some(Duration::from_secs(12))
            }
        );
    }

    #[test]
    fn retry_after_http_date_form() {
        // A far-future HTTP-date yields a positive (large) delay; a past date → None.
        let mut h = HeaderMap::new();
        h.insert(
            http::header::RETRY_AFTER,
            "Wed, 21 Oct 2099 07:28:00 GMT".parse().unwrap(),
        );
        match Disposition::from_http(StatusCode::TOO_MANY_REQUESTS, &h) {
            Disposition::RateLimited {
                retry_after: Some(d),
            } => assert!(d.as_secs() > 0, "future date → positive delay"),
            other => panic!("expected RateLimited with delay, got {other:?}"),
        }

        let mut past = HeaderMap::new();
        past.insert(
            http::header::RETRY_AFTER,
            "Wed, 21 Oct 1999 07:28:00 GMT".parse().unwrap(),
        );
        assert_eq!(
            Disposition::from_http(StatusCode::TOO_MANY_REQUESTS, &past),
            Disposition::RateLimited { retry_after: None },
            "past date → no cooldown"
        );
    }
}
