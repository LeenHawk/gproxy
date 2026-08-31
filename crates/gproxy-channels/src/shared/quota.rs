//! Helpers shared by the channels' quota-probe parsers.

use rust_decimal::Decimal;
use serde_json::Value;

pub(crate) fn iso_to_unix(value: &str) -> Option<i64> {
    time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
        .ok()
        .map(|stamp| stamp.unix_timestamp())
}

/// Lowercase, collapse every non-alphanumeric run into one `_`, trim.
pub(crate) fn slug(value: &str, fallback: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            output.push(character.to_ascii_lowercase());
        } else if !output.ends_with('_') {
            output.push('_');
        }
    }
    let output = output.trim_matches('_');
    if output.is_empty() {
        fallback.to_owned()
    } else {
        output.to_owned()
    }
}

/// Quota counters arrive as JSON numbers or numeric strings interchangeably.
pub(crate) fn decimal(value: &Value) -> Option<Decimal> {
    match value {
        Value::Number(number) => number
            .as_f64()
            .and_then(|value| Decimal::try_from(value).ok()),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

pub(crate) fn percent_used(used: Decimal, limit: Decimal) -> Option<Decimal> {
    if limit <= Decimal::ZERO {
        return None;
    }
    used.checked_div(limit)?
        .checked_mul(Decimal::ONE_HUNDRED)
        .map(|value| value.clamp(Decimal::ZERO, Decimal::ONE_HUNDRED).round_dp(4))
}

/// Code Assist quota buckets report the fraction LEFT in [0, 1]. Derived
/// percents round to 4 decimals: the f64 arithmetic leaves artifacts like
/// 19.999999999999996 that are noise, not wire facts.
pub(crate) fn remaining_fraction_to_used_percent(fraction: f64) -> Option<Decimal> {
    if !fraction.is_finite() {
        return None;
    }
    Decimal::try_from(((1.0 - fraction) * 100.0).clamp(0.0, 100.0))
        .ok()
        .map(|value| value.round_dp(4))
}
