//! The Go tier's usage windows.
//!
//! `GET {base}/usage` answers `{"usage": {"rolling": …, "weekly": …,
//! "monthly": …}}`, each window a `{status, percent, resetsAt}` where
//! `percent` is the share already used and `status` is `ok` or
//! `rate-limited` (v3 `shared/quota_management/opencode.rs`). Only
//! `opencodego` has it; the Zen channel declares no quota query.
//!
//! No `QuotaModel`: how long a "rolling" window rolls for is never stated on
//! the wire, and a declared dimension has to name a window length. The three
//! readings arrive as observations under the keys the reply names.
//!
//! **Not ported**: v3's `console_balance` source read the Zen wallet and the
//! monthly spending limit by fetching the authenticated Console billing
//! *page* and parsing the SolidJS hydration script embedded in its HTML,
//! using a separate `quota_cookie` and `quota_workspace_id`
//! (`shared/quota_management/opencode_console.rs`). That is a screen scrape
//! of a private web page rather than an upstream quota endpoint, it needs a
//! second credential the channel contract has no place for, and it breaks
//! whenever the page is rebuilt. It is left out deliberately.

use super::config::{GO_ID as ID, OpenCodeConfig, Tier};
use super::request::api_key;
use crate::channel::{
    ChannelError, CredentialContext, OperationFuture, QuotaAllowance, QuotaEntry,
    QuotaResetBehavior, QuotaScope, QuotaSnapshot, QuotaSubject, QuotaValue,
};
use crate::channels::shared::compatible::ability::{bearer, decimal, require_success, send};
use crate::channels::shared::compatible::http::invalid_response;
use http::Method;
use rust_decimal::Decimal;
use serde_json::Value;

/// The source every Go window is observed under.
pub const GO_SOURCE: &str = "opencode_go";

/// The windows the Go tier reports, with the label each is shown under.
const WINDOWS: [(&str, &str); 3] = [
    ("rolling", "rolling Go quota"),
    ("weekly", "weekly Go quota"),
    ("monthly", "monthly Go quota"),
];

fn text(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn iso_to_ms(value: &str) -> Option<i64> {
    time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
        .ok()
        .map(|stamp| stamp.unix_timestamp() * 1_000)
}

/// Every window has to read, because a partial reading would look like a
/// reset rather than like a failure to read.
fn entries(payload: &Value) -> Result<Vec<QuotaEntry>, ChannelError> {
    let usage = payload
        .get("usage")
        .ok_or_else(|| invalid_response(format!("{ID} usage: no usage")))?;
    WINDOWS
        .into_iter()
        .map(|(key, label)| {
            let window = usage
                .get(key)
                .ok_or_else(|| invalid_response(format!("{ID} usage: no `{key}` window")))?;
            if !matches!(text(window.get("status")), Some("ok" | "rate-limited")) {
                return Err(invalid_response(format!(
                    "{ID} usage: `{key}` reported no usable status"
                )));
            }
            let used_percent = window
                .get("percent")
                .and_then(decimal)
                .filter(|percent| (Decimal::ZERO..=Decimal::from(100)).contains(percent))
                .ok_or_else(|| {
                    invalid_response(format!("{ID} usage: `{key}` has no percentage"))
                })?;
            let period_end_ms = text(window.get("resetsAt"))
                .and_then(iso_to_ms)
                .ok_or_else(|| {
                    invalid_response(format!("{ID} usage: `{key}` has no reset time"))
                })?;
            Ok(QuotaEntry {
                id: key.to_owned(),
                source_id: GO_SOURCE.into(),
                label: Some(label.to_owned()),
                subject: QuotaSubject::Account,
                model_scope: QuotaScope::All,
                value: QuotaValue::Window(QuotaAllowance {
                    used_percent: Some(used_percent),
                    period_end_ms: Some(period_end_ms),
                    reset_behavior: QuotaResetBehavior::Periodic,
                    ..QuotaAllowance::default()
                }),
            })
        })
        .collect()
}

impl crate::channel::QuotaQuery for super::OpenCode {
    fn query<'a>(&'a self, context: CredentialContext<'a>) -> OperationFuture<'a, QuotaSnapshot> {
        Box::pin(async move {
            let key = api_key(&context.credential)?;
            let (status, _, body) = send(
                context.client,
                Method::GET,
                &format!(
                    "{}/usage",
                    OpenCodeConfig::base_url(context.provider, Tier::Go)
                ),
                bearer(key)?,
                None,
            )
            .await?;
            let body = require_success(status, body)?;
            let payload: Value = serde_json::from_slice(&body)
                .map_err(|error| invalid_response(format!("{ID} usage: {error}")))?;
            Ok(QuotaSnapshot {
                // The host stamps receipt; the payload carries no clock.
                observed_at_ms: 0,
                entries: entries(&payload)?,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn payload() -> Value {
        json!({"usage": {
            "rolling": {"status": "ok", "percent": 12.5, "resetsAt": "2030-01-01T05:00:00Z"},
            "weekly": {"status": "rate-limited", "percent": 100,
                       "resetsAt": "2030-01-05T00:00:00Z"},
            "monthly": {"status": "ok", "percent": 0, "resetsAt": "2030-02-01T00:00:00Z"},
        }})
    }

    #[test]
    fn the_three_windows_report_the_share_used_and_their_reset() {
        let entries = entries(&payload()).unwrap();
        assert_eq!(
            entries.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            ["rolling", "weekly", "monthly"]
        );
        assert!(entries.iter().all(|entry| entry.source_id == GO_SOURCE));
        let QuotaValue::Window(rolling) = &entries[0].value else {
            panic!("a window");
        };
        assert_eq!(rolling.used_percent, Some("12.5".parse().unwrap()));
        assert_eq!(rolling.period_end_ms, Some(1_893_474_000_000));
        assert_eq!((rolling.used, rolling.limit), (None, None));
    }

    #[test]
    fn a_window_that_does_not_read_fails_the_whole_snapshot() {
        for (key, replacement) in [
            (
                "rolling",
                json!({"status": "unknown", "percent": 1,
                               "resetsAt": "2030-01-01T05:00:00Z"}),
            ),
            (
                "weekly",
                json!({"status": "ok", "percent": 101,
                              "resetsAt": "2030-01-01T05:00:00Z"}),
            ),
            (
                "monthly",
                json!({"status": "ok", "percent": 1, "resetsAt": "nonsense"}),
            ),
            (
                "monthly",
                json!({"status": "ok", "resetsAt": "2030-01-01T05:00:00Z"}),
            ),
        ] {
            let mut broken = payload();
            broken["usage"][key] = replacement;
            assert!(entries(&broken).is_err(), "{key}");
        }
        let mut missing = payload();
        missing["usage"].as_object_mut().unwrap().remove("weekly");
        assert!(entries(&missing).is_err());
        assert!(entries(&json!({})).is_err());
    }
}
