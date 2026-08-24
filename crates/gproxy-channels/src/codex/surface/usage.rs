use gproxy_channel_api::{ChannelError, QuotaWindow, SurfaceServices};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde_json::{Map, Value, json};

use super::helpers::{plan_type, unix_now};

const PRIMARY_SECS: i64 = 18_000;
const SECONDARY_SECS: i64 = 604_800;

pub(super) async fn profile(services: &SurfaceServices<'_>) -> Result<Value, ChannelError> {
    let usage = services.usage.window(0).await.map_err(prepare_error)?;
    Ok(json!({
        "stats": {
            "lifetime_tokens": usage.input_tokens.saturating_add(usage.output_tokens),
            "peak_daily_tokens": null,
            "longest_running_turn_sec": null,
            "current_streak_days": null,
            "longest_streak_days": null,
            "daily_usage_buckets": null
        }
    }))
}

pub(super) async fn usage(services: &SurfaceServices<'_>) -> Result<Value, ChannelError> {
    let local = services.usage.window(0).await.map_err(prepare_error)?;
    let windows = services
        .usage
        .quota_windows()
        .await
        .map_err(prepare_error)?;
    let now = unix_now();
    let primary = select_window(&windows, &["primary", "5h", "five-hour"], PRIMARY_SECS);
    let secondary = select_window(&windows, &["secondary", "7d", "seven-day"], SECONDARY_SECS);
    let reached = primary
        .iter()
        .chain(secondary.iter())
        .any(|(_, percent)| percent.is_some_and(|value| value >= Decimal::ONE_HUNDRED));
    let complete = primary
        .as_ref()
        .is_some_and(|(_, percent)| percent.is_some())
        && secondary
            .as_ref()
            .is_some_and(|(_, percent)| percent.is_some());

    let mut rate_limit = Map::new();
    if let Some((window, percent)) = primary {
        rate_limit.insert(
            "primary_window".into(),
            window_payload(window, percent, now),
        );
    }
    if let Some((window, percent)) = secondary {
        rate_limit.insert(
            "secondary_window".into(),
            window_payload(window, percent, now),
        );
    }
    if reached || complete {
        rate_limit.insert("allowed".into(), Value::Bool(!reached));
        rate_limit.insert("limit_reached".into(), Value::Bool(reached));
    }

    let mut response = Map::from_iter([
        (
            "plan_type".into(),
            Value::String(plan_type(services.provider.settings).into()),
        ),
        (
            "rate_limit_reset_credits".into(),
            json!({"available_count":0}),
        ),
        (
            "local_usage".into(),
            json!({
                "cost": local.cost,
                "input_tokens": local.input_tokens,
                "output_tokens": local.output_tokens
            }),
        ),
    ]);
    if !rate_limit.is_empty() {
        response.insert("rate_limit".into(), Value::Object(rate_limit));
    }
    if reached {
        response.insert(
            "rate_limit_reached_type".into(),
            json!({"type":"rate_limit_reached"}),
        );
    }
    Ok(Value::Object(response))
}

fn select_window<'a>(
    windows: &'a [QuotaWindow],
    keys: &[&str],
    duration: i64,
) -> Option<(&'a QuotaWindow, Option<Decimal>)> {
    windows
        .iter()
        .filter_map(|window| {
            let key_matches = keys.iter().any(|key| window.key.eq_ignore_ascii_case(key));
            let duration_matches = window
                .period_start
                .zip(window.reset_at)
                .is_some_and(|(start, reset)| reset.saturating_sub(start) == duration);
            (key_matches || duration_matches).then(|| (window, window_percent(window)))
        })
        .max_by_key(|(_, percent)| *percent)
}

fn window_percent(window: &QuotaWindow) -> Option<Decimal> {
    window
        .used_percent
        .or_else(|| {
            let used = window.upstream_used?;
            let limit = window.upstream_limit?;
            (limit > Decimal::ZERO).then(|| used / limit * Decimal::ONE_HUNDRED)
        })
        .map(|percent| percent.clamp(Decimal::ZERO, Decimal::ONE_HUNDRED))
}

fn window_payload(window: &QuotaWindow, percent: Option<Decimal>, now: i64) -> Value {
    let mut payload = Map::new();
    if let Some(percent) = percent {
        let used_percent = percent
            .round()
            .to_i64()
            .expect("bounded quota percentage fits in i64");
        payload.insert("used_percent".into(), Value::from(used_percent));
    }
    if let Some(reset_at) = window.reset_at {
        payload.insert("reset_at".into(), Value::from(reset_at));
        payload.insert(
            "reset_after_seconds".into(),
            Value::from(reset_at.saturating_sub(now).max(0)),
        );
        if let Some(start) = window.period_start {
            payload.insert(
                "limit_window_seconds".into(),
                Value::from(reset_at.saturating_sub(start)),
            );
        }
    }
    Value::Object(payload)
}

fn prepare_error(error: impl std::fmt::Display) -> ChannelError {
    ChannelError::Prepare(error.to_string())
}
