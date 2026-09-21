//! The Copilot seat's metered features.
//!
//! `GET {github_api}/copilot_internal/user`, presented with the GitHub token
//! rather than the Copilot one, answers a `quota_snapshots` object with one
//! entry per metered feature — `chat`, `completions`,
//! `premium_interactions` — each carrying an `entitlement`, a `remaining`
//! count and a `percent_remaining`. `unlimited: true` means the feature is
//! not metered at all on this plan and is reported as nothing rather than as
//! a full window. `quota_reset_date` is a bare `YYYY-MM-DD`, not a timestamp
//! (v3 `copilotcli/quota.rs`).
//!
//! No `QuotaModel`: which of the three features a seat meters is a plan fact
//! the reply carries and nothing on the credential states it, so a declared
//! dimension would be a guess. The readings arrive under the feature names.

use super::auth;
use super::config::{CopilotCliConfig, ID};
use crate::channel::{
    CredentialContext, OperationFuture, QuotaAllowance, QuotaEntry, QuotaResetBehavior, QuotaScope,
    QuotaSnapshot, QuotaSubject, QuotaValue,
};
use crate::channels::shared::compatible::ability::{require_success, send};
use crate::channels::shared::compatible::http::invalid_response;
use http::Method;
use rust_decimal::Decimal;
use serde::Deserialize;

/// The source every Copilot feature window is observed under.
pub const QUOTA_SOURCE: &str = "copilot_quota";

#[derive(Deserialize)]
struct CopilotUser {
    #[serde(default)]
    quota_reset_date: Option<String>,
    #[serde(default)]
    quota_snapshots: Option<QuotaSnapshots>,
}

#[derive(Deserialize, Default)]
struct QuotaSnapshots {
    #[serde(default)]
    chat: Option<QuotaDetail>,
    #[serde(default)]
    completions: Option<QuotaDetail>,
    #[serde(default)]
    premium_interactions: Option<QuotaDetail>,
}

#[derive(Deserialize)]
struct QuotaDetail {
    #[serde(default)]
    entitlement: Option<f64>,
    #[serde(default)]
    remaining: Option<f64>,
    #[serde(default)]
    percent_remaining: Option<f64>,
    #[serde(default)]
    unlimited: Option<bool>,
}

/// `YYYY-MM-DD` as milliseconds at midnight UTC. An ISO-8601 timestamp is
/// accepted too, in case the field ever grows one.
fn reset_to_ms(value: &str) -> Option<i64> {
    let value = value.trim();
    if let Ok(stamp) =
        time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
    {
        return Some(stamp.unix_timestamp() * 1_000);
    }
    let mut parts = value.split('-');
    let year = parts.next()?.parse().ok()?;
    let month: u8 = parts.next()?.parse().ok()?;
    let day = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    time::Date::from_calendar_date(year, time::Month::try_from(month).ok()?, day)
        .ok()
        .map(|date| date.midnight().assume_utc().unix_timestamp() * 1_000)
}

fn decimal(value: f64) -> Option<Decimal> {
    Decimal::try_from(value).ok()
}

fn entries(payload: &CopilotUser) -> Vec<QuotaEntry> {
    let period_end_ms = payload.quota_reset_date.as_deref().and_then(reset_to_ms);
    let snapshots = payload.quota_snapshots.as_ref();
    [
        ("chat", snapshots.and_then(|s| s.chat.as_ref())),
        (
            "completions",
            snapshots.and_then(|s| s.completions.as_ref()),
        ),
        (
            "premium_interactions",
            snapshots.and_then(|s| s.premium_interactions.as_ref()),
        ),
    ]
    .into_iter()
    .filter_map(|(feature, detail)| {
        let detail = detail?;
        // An unmetered feature has no window to report.
        if detail.unlimited == Some(true) {
            return None;
        }
        let limit = detail.entitlement.and_then(decimal);
        let remaining = detail.remaining.and_then(decimal);
        Some(QuotaEntry {
            id: feature.to_owned(),
            source_id: QUOTA_SOURCE.into(),
            label: Some(feature.replace('_', " ")),
            subject: QuotaSubject::Account,
            // Premium interactions are charged on every model; the other two
            // name a Copilot feature rather than a model family.
            model_scope: if feature == "premium_interactions" {
                QuotaScope::All
            } else {
                QuotaScope::Unknown
            },
            value: QuotaValue::Window(QuotaAllowance {
                used: limit
                    .zip(remaining)
                    .map(|(limit, remaining)| (limit - remaining).max(Decimal::ZERO)),
                limit,
                remaining,
                used_percent: detail
                    .percent_remaining
                    .map(|percent| (100.0 - percent).clamp(0.0, 100.0))
                    .and_then(decimal),
                unlimited: detail.unlimited,
                unit: None,
                period_start_ms: None,
                period_end_ms,
                reset_behavior: QuotaResetBehavior::Periodic,
            }),
        })
    })
    .collect()
}

impl crate::channel::QuotaQuery for super::CopilotCli {
    fn query<'a>(&'a self, context: CredentialContext<'a>) -> OperationFuture<'a, QuotaSnapshot> {
        Box::pin(async move {
            let config = CopilotCliConfig::from_view(context.provider)?;
            // The account surface is GitHub's, so it takes the GitHub token;
            // the Copilot bearer is only good against the inference origin.
            let github = auth::github_token(&context.credential)?;
            let (status, _, body) = send(
                context.client,
                Method::GET,
                &format!("{}/copilot_internal/user", config.github_api_url()),
                auth::github_headers(&config, github)?,
                None,
            )
            .await?;
            let body = require_success(status, body)?;
            let payload: CopilotUser = serde_json::from_slice(&body)
                .map_err(|error| invalid_response(format!("{ID} account: {error}")))?;
            Ok(QuotaSnapshot {
                // The host stamps receipt; the payload carries no clock.
                observed_at_ms: 0,
                entries: entries(&payload),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_metered_feature_becomes_a_window_and_an_unlimited_one_becomes_nothing() {
        let body = br#"{
          "copilot_plan": "pro",
          "quota_reset_date": "2026-07-01",
          "quota_snapshots": {
            "chat": {"entitlement": 0, "remaining": 0, "percent_remaining": 100,
                     "unlimited": true},
            "completions": {"entitlement": 0, "remaining": 0, "percent_remaining": 100,
                            "unlimited": true},
            "premium_interactions": {"entitlement": 300, "remaining": 270,
                                     "percent_remaining": 90, "unlimited": false}
          }
        }"#;
        let payload: CopilotUser = serde_json::from_slice(body).unwrap();
        let entries = entries(&payload);
        assert_eq!(entries.len(), 1, "unlimited features report nothing");
        assert_eq!(entries[0].id, "premium_interactions");
        assert_eq!(entries[0].source_id, QUOTA_SOURCE);
        assert_eq!(entries[0].model_scope, QuotaScope::All);
        let QuotaValue::Window(window) = &entries[0].value else {
            panic!("a window");
        };
        assert_eq!(window.used, Some(Decimal::from(30)));
        assert_eq!(window.limit, Some(Decimal::from(300)));
        assert_eq!(window.remaining, Some(Decimal::from(270)));
        assert_eq!(window.used_percent, Some(Decimal::from(10)));
        // A bare date is midnight UTC.
        assert_eq!(window.period_end_ms, Some(1_782_864_000_000));
    }

    #[test]
    fn a_bare_date_and_a_timestamp_both_resolve_and_nonsense_does_not() {
        assert_eq!(reset_to_ms("2026-07-01"), Some(1_782_864_000_000));
        assert_eq!(reset_to_ms("2026-07-01T00:00:00Z"), Some(1_782_864_000_000));
        assert_eq!(reset_to_ms("2026-07-01-02"), None);
        assert_eq!(reset_to_ms("soon"), None);
        assert_eq!(reset_to_ms("2026-13-01"), None);
    }
}
