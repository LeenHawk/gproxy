use crate::query::runtime;
use crate::records::{CaptureInput, QuotaWindowRecord, RequestLogInput};
use crate::{Store, StoreError};

impl Store {
    pub async fn add_quota_usage(
        &self,
        quota_id: i64,
        window_start: i64,
        delta: i64,
    ) -> Result<QuotaWindowRecord, StoreError> {
        let results = self
            .backend()
            .batch(vec![
                runtime::add_quota_window(quota_id, window_start, delta)?,
                runtime::read_quota_window(quota_id, window_start)?,
            ])
            .await?;
        quota_window(
            results
                .into_iter()
                .nth(1)
                .ok_or_else(|| StoreError::Database("quota window result missing".into()))?,
        )
    }

    pub async fn quota_window(
        &self,
        quota_id: i64,
        window_start: i64,
    ) -> Result<Option<QuotaWindowRecord>, StoreError> {
        let result = self
            .backend()
            .execute(runtime::read_quota_window(quota_id, window_start)?)
            .await?;
        result
            .rows
            .into_iter()
            .next()
            .map(parse_quota_window)
            .transpose()
    }

    pub async fn quota_windows(&self) -> Result<Vec<QuotaWindowRecord>, StoreError> {
        self.backend()
            .execute(runtime::select_quota_windows()?)
            .await?
            .rows
            .into_iter()
            .map(parse_quota_window)
            .collect()
    }

    pub async fn begin_request_log(&self, input: &RequestLogInput) -> Result<(), StoreError> {
        self.backend()
            .execute(runtime::begin_request_log(input)?)
            .await?;
        Ok(())
    }

    pub async fn record_capture(&self, input: &CaptureInput) -> Result<(), StoreError> {
        self.backend()
            .batch(vec![
                runtime::finish_request_log(&input.request_id, input.response_status, None)?,
                runtime::insert_capture(input)?,
            ])
            .await?;
        Ok(())
    }
}

fn quota_window(result: crate::backend::QueryResult) -> Result<QuotaWindowRecord, StoreError> {
    result
        .rows
        .into_iter()
        .next()
        .ok_or_else(|| StoreError::Database("quota window row missing".into()))
        .and_then(parse_quota_window)
}

fn parse_quota_window(row: crate::backend::Row) -> Result<QuotaWindowRecord, StoreError> {
    Ok(QuotaWindowRecord {
        quota_id: row.i64("quota_id")?,
        window_start: row.i64("window_start")?,
        used_tokens: u64::try_from(row.i64("used_tokens")?).map_err(|error| {
            StoreError::InvalidData {
                field: "used_tokens",
                message: error.to_string(),
            }
        })?,
    })
}
