use crate::records::QuotaWindowKind;

pub(super) fn period(kind: QuotaWindowKind, now: i64) -> (i64, Option<i64>) {
    const DAY: i64 = 86_400;
    match kind {
        QuotaWindowKind::Total => (0, None),
        QuotaWindowKind::Daily => aligned(now, DAY),
        QuotaWindowKind::Weekly => {
            let day = now.div_euclid(DAY);
            let start = (day - (day + 3).rem_euclid(7)) * DAY;
            (start, Some(start + 7 * DAY))
        }
        QuotaWindowKind::Monthly => month_period(now),
        QuotaWindowKind::FiveHour => anchored(now, 5 * 3_600),
        QuotaWindowKind::SevenDay => anchored(now, 7 * DAY),
    }
}

fn aligned(now: i64, seconds: i64) -> (i64, Option<i64>) {
    let start = now - now.rem_euclid(seconds);
    (start, Some(start + seconds))
}

fn anchored(now: i64, seconds: i64) -> (i64, Option<i64>) {
    (now, Some(now.saturating_add(seconds)))
}

fn month_period(now: i64) -> (i64, Option<i64>) {
    const DAY: i64 = 86_400;
    let (year, month) = civil_month(now.div_euclid(DAY));
    let start = days_from_civil(year, month, 1) * DAY;
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    (start, Some(days_from_civil(next_year, next_month, 1) * DAY))
}

// Proleptic Gregorian conversion keeps calendar windows dependency-free.
fn civil_month(days: i64) -> (i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year, month)
}

fn days_from_civil(mut year: i64, month: i64, day: i64) -> i64 {
    year -= i64::from(month <= 2);
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_prime = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month_prime + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}
