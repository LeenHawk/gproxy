mod period;

use rust_decimal::Decimal;

use self::period::period;
use crate::query::runtime;
use crate::records::{QuotaWindowKind, QuotaWindowRecord};
use crate::{Store, StoreError};

impl Store {
    pub async fn add_quota_cost(
        &self,
        request_id: &str,
        window_id: i64,
        delta: Decimal,
    ) -> Result<QuotaWindowRecord, StoreError> {
        const RETRIES: usize = 8;
        for _ in 0..RETRIES {
            if self.quota_settlement_exists(request_id, window_id).await? {
                return self
                    .quota_window(window_id)
                    .await?
                    .ok_or_else(|| StoreError::Database("quota window vanished".into()));
            }
            let existing = self
                .quota_window(window_id)
                .await?
                .ok_or_else(|| StoreError::Database("quota window vanished".into()))?;
            let cost_used = existing.cost_used + delta;
            let result = self
                .backend()
                .batch(vec![
                    runtime::update_quota_window_cost(window_id, existing.cost_used, cost_used)?,
                    runtime::insert_quota_settlement(request_id, window_id, delta)?,
                ])
                .await;
            match result {
                Ok(results)
                    if results
                        .first()
                        .is_some_and(|result| result.affected_rows == 1)
                        && results
                            .get(1)
                            .is_some_and(|result| result.affected_rows == 1) =>
                {
                    return Ok(QuotaWindowRecord {
                        cost_used,
                        ..existing
                    });
                }
                Ok(_) => {}
                Err(error) if unique_conflict(&error) => {}
                Err(error) => return Err(error),
            }
        }
        Err(StoreError::Database(
            "quota window update remained contended".into(),
        ))
    }

    async fn quota_settlement_exists(
        &self,
        request_id: &str,
        window_id: i64,
    ) -> Result<bool, StoreError> {
        Ok(!self
            .backend()
            .execute(runtime::read_quota_settlement(request_id, window_id)?)
            .await?
            .rows
            .is_empty())
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

fn unique_conflict(error: &StoreError) -> bool {
    matches!(error, StoreError::Database(message) if message.to_ascii_lowercase().contains("unique"))
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
