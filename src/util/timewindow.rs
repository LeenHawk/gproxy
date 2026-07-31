//! UTC calendar-window keys and exact decimal accumulator rollover.

use rust_decimal::Decimal;

pub const fn day_key(unix: i64) -> i64 {
    unix.div_euclid(86_400)
}

/// Monday-start week key. Epoch day 0 was Thursday, so 1970-01-05 is week 1.
pub const fn week_key(unix: i64) -> i64 {
    (day_key(unix) + 3).div_euclid(7)
}

pub const fn month_key(unix: i64) -> i64 {
    let (year, month) = civil_from_days(day_key(unix));
    year * 12 + month - 1
}

/// Howard Hinnant's civil-from-days algorithm, with day zero at 1970-01-01.
const fn civil_from_days(days: i64) -> (i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let month = mp + if mp < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }
    (year, month)
}

/// Roll an accumulator to `current_key`, then apply `delta`.
pub(crate) fn accumulate(
    anchor: i64,
    used_raw: &str,
    current_key: i64,
    delta: Decimal,
) -> anyhow::Result<(i64, Decimal)> {
    let used = if anchor == current_key {
        used_raw.parse::<Decimal>()?
    } else {
        Decimal::ZERO
    };
    Ok((current_key, used + delta))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_window_boundaries() {
        assert_eq!(day_key(-1), -1);
        assert_eq!(day_key(0), 0);
        assert_eq!(day_key(86_400), 1);
        assert_eq!(week_key(4 * 86_400), 1); // 1970-01-05, Monday
        assert_eq!(week_key(3 * 86_400), 0);
        assert_eq!(month_key(0), 1970 * 12); // 1970-01-01
        assert_eq!(month_key(59 * 86_400), 1970 * 12 + 2); // 1970-03-01
        assert_eq!(month_key(11_016 * 86_400), 2000 * 12 + 1); // 2000-02-29
        assert_eq!(month_key(11_017 * 86_400), 2000 * 12 + 2); // 2000-03-01
    }
}
