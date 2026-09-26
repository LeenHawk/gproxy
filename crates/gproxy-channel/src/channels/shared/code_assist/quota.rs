//! Turning the Code Assist quota payloads into `QuotaEntry`s.
//!
//! Both Gemini CLI tools report the fraction of a per-model bucket that is
//! **left** (`remainingFraction`, 0..1) and an ISO-8601 `resetTime`, which
//! maps onto a periodic window whose used percentage is `(1 - fraction)*100`
//! (v3 `shared/quota.rs` plus each channel's `quota.rs`).

use crate::channel::{
    QuotaAllowance, QuotaEntry, QuotaResetBehavior, QuotaScope, QuotaSubject, QuotaValue,
};
use rust_decimal::Decimal;
use serde_json::Value;

/// A number that may arrive as a JSON number or as a numeric string.
pub(crate) fn decimal(value: &Value) -> Option<Decimal> {
    match value {
        Value::Number(number) => number.as_i64().map(Decimal::from).or_else(|| {
            number
                .as_f64()
                .and_then(|value| Decimal::try_from(value).ok())
        }),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

pub(crate) fn iso_to_ms(value: &str) -> Option<i64> {
    time::OffsetDateTime::parse(value.trim(), &time::format_description::well_known::Rfc3339)
        .ok()
        .map(|stamp| stamp.unix_timestamp().saturating_mul(1000))
}

/// `remainingFraction` is the share still available; the host stores use.
pub(crate) fn used_percent(remaining_fraction: f64) -> Option<Decimal> {
    if !remaining_fraction.is_finite() {
        return None;
    }
    Decimal::try_from((1.0 - remaining_fraction.clamp(0.0, 1.0)) * 100.0)
        .ok()
        .map(|percent| percent.round_dp(4))
}

/// Keep `-`, `_` and `.` so model ids stay readable in a window key (v3
/// `geminicli/quota.rs::component`).
pub(crate) fn component(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for character in value.chars() {
        if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
            output.push(character.to_ascii_lowercase());
        } else if !output.ends_with('_') {
            output.push('_');
        }
    }
    let trimmed = output.trim_matches('_');
    if trimmed.is_empty() {
        "unknown".into()
    } else {
        trimmed.into()
    }
}

/// One reported bucket as a periodic window. A bucket that reports real
/// amounts is recorded in its own units; one that only reports a fraction is
/// recorded on the 0..100 percent scale, as `claudecode`'s windows are.
pub(crate) fn window(
    id: String,
    label: Option<String>,
    scope: QuotaScope,
    used_percent: Option<Decimal>,
    amounts: Option<(Decimal, Decimal)>,
    period_end_ms: Option<i64>,
) -> QuotaEntry {
    let (used, limit, remaining, unit) = match amounts {
        Some((spent, limit)) => (
            Some(spent),
            Some(limit),
            Some((limit - spent).max(Decimal::ZERO)),
            None,
        ),
        None => (
            used_percent,
            Some(Decimal::ONE_HUNDRED),
            used_percent.map(|used| Decimal::ONE_HUNDRED.saturating_sub(used).max(Decimal::ZERO)),
            Some("percent".to_owned()),
        ),
    };
    QuotaEntry {
        source_id: id.clone(),
        id,
        label,
        subject: QuotaSubject::Account,
        model_scope: scope,
        value: QuotaValue::Window(QuotaAllowance {
            used,
            limit,
            remaining,
            used_percent,
            unlimited: None,
            unit,
            period_start_ms: None,
            period_end_ms,
            reset_behavior: QuotaResetBehavior::Periodic,
        }),
    }
}
