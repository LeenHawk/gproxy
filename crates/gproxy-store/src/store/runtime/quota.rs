use rust_decimal::Decimal;

use crate::query::runtime;
use crate::records::{QuotaWindowKind, QuotaWindowRecord};
use crate::{Store, StoreError};

impl Store {
    pub async fn add_quota_cost(
        &self,
        window_id: i64,
        delta: Decimal,
    ) -> Result<QuotaWindowRecord, StoreError> {
        const RETRIES: usize = 8;
        for _ in 0..RETRIES {
            let existing = self
                .quota_window(window_id)
                .await?
                .ok_or_else(|| StoreError::Database("quota window vanished".into()))?;
            let cost_used = existing.cost_used + delta;
            let updated = self
                .backend()
                .execute(runtime::update_quota_window_cost(
                    window_id,
                    existing.cost_used,
                    cost_used,
                )?)
                .await?;
            if updated.affected_rows == 1 {
                return Ok(QuotaWindowRecord {
                    cost_used,
                    ..existing
                });
            }
        }
        Err(StoreError::Database(
            "quota window update remained contended".into(),
        ))
    }

    pub async fn quota_window(
        &self,
        window_id: i64,
    ) -> Result<Option<QuotaWindowRecord>, StoreError> {
        let result = self
            .backend()
            .execute(runtime::read_quota_window(window_id)?)
            .await?;
        result.rows.into_iter().next().map(parse).transpose()
    }

    pub async fn ensure_quota_window(
        &self,
        quota_id: i64,
        kind: QuotaWindowKind,
        now: i64,
    ) -> Result<QuotaWindowRecord, StoreError> {
        const RETRIES: usize = 8;
        for _ in 0..RETRIES {
            if let Some(current) = self.active_quota_window(quota_id, kind).await? {
                if current.reset_at.is_none_or(|reset| reset > now) {
                    return Ok(current);
                }
                self.backend()
                    .execute(runtime::close_quota_window(current.id)?)
                    .await?;
                continue;
            }
            let (start, reset_at) = period(kind, now);
            self.backend()
                .execute(runtime::insert_quota_window(
                    quota_id,
                    kind.as_str(),
                    start,
                    reset_at,
                )?)
                .await?;
        }
        Err(StoreError::Database(
            "quota window rollover remained contended".into(),
        ))
    }

    pub async fn quota_windows(&self) -> Result<Vec<QuotaWindowRecord>, StoreError> {
        self.backend()
            .execute(runtime::select_quota_windows()?)
            .await?
            .rows
            .into_iter()
            .map(parse)
            .collect()
    }

    async fn active_quota_window(
        &self,
        quota_id: i64,
        kind: QuotaWindowKind,
    ) -> Result<Option<QuotaWindowRecord>, StoreError> {
        let result = self
            .backend()
            .execute(runtime::read_active_quota_window(quota_id, kind.as_str())?)
            .await?;
        result.rows.into_iter().next().map(parse).transpose()
    }
}

fn parse(row: crate::backend::Row) -> Result<QuotaWindowRecord, StoreError> {
    let raw_kind = row.text("window_kind")?;
    Ok(QuotaWindowRecord {
        id: row.i64("id")?,
        quota_id: row.i64("quota_id")?,
        window_kind: QuotaWindowKind::from_name(raw_kind).ok_or_else(|| {
            StoreError::InvalidData {
                field: "window_kind",
                message: format!("unknown quota window kind `{raw_kind}`"),
            }
        })?,
        window_start: row.i64("window_start")?,
        reset_at: row.optional_i64("reset_at")?,
        cost_used: row.text("cost_used")?.parse::<Decimal>().map_err(|error| {
            StoreError::InvalidData {
                field: "cost_used",
                message: error.to_string(),
            }
        })?,
    })
}

fn period(kind: QuotaWindowKind, now: i64) -> (i64, Option<i64>) {
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
