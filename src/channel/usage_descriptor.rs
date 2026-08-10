//! Helpers shared by built-in usage-window descriptor implementations.

use crate::channel::{
    UsageWindow, UsageWindowBoundaryConfidence, UsageWindowBoundarySource, UsageWindowDescriptor,
};

pub(crate) fn iso_to_unix(value: &str) -> Option<i64> {
    // Keep this module available to edge/wasm builds without pulling the
    // native-only `time` dependency into the channel contract.
    let (date, time) = value.trim().split_once(['T', 't'])?;
    let mut date_parts = date.split('-');
    let year = date_parts.next()?.parse::<i64>().ok()?;
    let month = date_parts.next()?.parse::<u32>().ok()?;
    let day = date_parts.next()?.parse::<u32>().ok()?;
    if date_parts.next().is_some() || !(1..=12).contains(&month) {
        return None;
    }
    let max_day = days_in_month(year, month);
    if day == 0 || day > max_day {
        return None;
    }

    let (clock, offset_seconds) =
        if let Some(clock) = time.strip_suffix('Z').or_else(|| time.strip_suffix('z')) {
            (clock, 0_i64)
        } else {
            let split = time
                .char_indices()
                .skip(1)
                .filter_map(|(index, ch)| matches!(ch, '+' | '-').then_some(index))
                .last()?;
            let (clock, offset) = time.split_at(split);
            let sign = if offset.starts_with('+') { 1 } else { -1 };
            let mut parts = offset[1..].split(':');
            let hours = parts.next()?.parse::<i64>().ok()?;
            let minutes = parts.next()?.parse::<i64>().ok()?;
            if parts.next().is_some() || hours > 23 || minutes > 59 {
                return None;
            }
            (clock, sign * (hours * 3600 + minutes * 60))
        };
    let mut clock_parts = clock.split(':');
    let hour = clock_parts.next()?.parse::<i64>().ok()?;
    let minute = clock_parts.next()?.parse::<i64>().ok()?;
    let second = clock_parts.next()?.split('.').next()?.parse::<i64>().ok()?;
    if clock_parts.next().is_some() || hour > 23 || minute > 59 || second > 59 {
        return None;
    }

    let days = days_from_civil(year, month, day);
    Some(
        days.saturating_mul(86_400)
            .saturating_add(hour * 3600 + minute * 60 + second)
            .saturating_sub(offset_seconds),
    )
}

fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        _ => 0,
    }
}

// Howard Hinnant's civil-date conversion, offset to the Unix epoch.
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let shifted_month = i64::from(month) + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * shifted_month + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

pub(crate) fn reset_unix(window: &UsageWindow) -> Option<i64> {
    window
        .resets_at_unix
        .or_else(|| window.resets_at.as_deref().and_then(iso_to_unix))
}

pub(crate) fn with_known_duration(
    descriptor: UsageWindowDescriptor,
    window: &UsageWindow,
    seconds: i64,
) -> UsageWindowDescriptor {
    let Some(reset) = reset_unix(window) else {
        return descriptor;
    };
    descriptor.period_start(
        reset.saturating_sub(seconds),
        UsageWindowBoundarySource::KnownWindow,
        UsageWindowBoundaryConfidence::Derived,
    )
}
