use crate::backend::Row;
use crate::query::usage;
use crate::records::{UsageInput, UsageRecord, UsageWindow};
use crate::{Store, StoreError};

impl Store {
    pub async fn record_usage(&self, input: &UsageInput) -> Result<bool, StoreError> {
        let results = self
            .backend()
            .batch(vec![
                usage::insert_usage(input)?,
                usage::accumulate_hourly(input)?,
            ])
            .await?;
        Ok(results
            .first()
            .is_some_and(|result| result.affected_rows == 1))
    }

    pub async fn usage_by_request(
        &self,
        request_id: &str,
    ) -> Result<Option<UsageRecord>, StoreError> {
        let result = self
            .backend()
            .execute(usage::usage_by_request(request_id)?)
            .await?;
        result.rows.into_iter().next().map(parse_usage).transpose()
    }

    pub async fn usage_window(
        &self,
        user_id: i64,
        provider_id: i64,
        since: i64,
    ) -> Result<UsageWindow, StoreError> {
        let mut result = self
            .backend()
            .execute(usage::aggregate_for_caller(user_id, provider_id, since)?)
            .await?;
        let row = result
            .rows
            .pop()
            .ok_or_else(|| StoreError::Database("usage aggregate row missing".into()))?;
        Ok(UsageWindow {
            cost: decimal(row.text("cost")?, "cost")?,
            input_tokens: unsigned(row.i64("input_tokens")?, "input_tokens")?,
            output_tokens: unsigned(row.i64("output_tokens")?, "output_tokens")?,
        })
    }
}

fn parse_usage(row: Row) -> Result<UsageRecord, StoreError> {
    Ok(UsageRecord {
        id: row.i64("id")?,
        usage: UsageInput {
            request_id: row.text("request_id")?.to_owned(),
            at: row.i64("at")?,
            provider_id: row.i64("provider_id")?,
            credential_id: row.i64("credential_id")?,
            organization_id: row.optional_i64("organization_id")?,
            team_id: row.optional_i64("team_id")?,
            user_id: row.optional_i64("user_id")?,
            user_key_id: row.optional_i64("user_key_id")?,
            operation: row.optional_text("operation")?.map(str::to_owned),
            upstream_model: row.text("upstream_model")?.to_owned(),
            input_tokens: unsigned(row.i64("input_tokens")?, "input_tokens")?,
            output_tokens: unsigned(row.i64("output_tokens")?, "output_tokens")?,
            cached_input_tokens: unsigned(row.i64("cached_input_tokens")?, "cached_input_tokens")?,
            metrics: json(row.text("metrics_json")?, "metrics_json")?,
            dimensions: json(row.text("dimensions_json")?, "dimensions_json")?,
            cost: decimal(row.text("cost")?, "cost")?,
            usage_source: row.text("usage_source")?.to_owned(),
            ended: row.text("ended")?.to_owned(),
            latency_ms: unsigned(row.i64("latency_ms")?, "latency_ms")?,
        },
    })
}

fn unsigned(value: i64, field: &'static str) -> Result<u64, StoreError> {
    u64::try_from(value).map_err(|error| invalid(field, error))
}

fn decimal(value: &str, field: &'static str) -> Result<rust_decimal::Decimal, StoreError> {
    value.parse().map_err(|error| invalid(field, error))
}

fn json(value: &str, field: &'static str) -> Result<serde_json::Value, StoreError> {
    serde_json::from_str(value).map_err(|error| invalid(field, error))
}

fn invalid(field: &'static str, error: impl std::fmt::Display) -> StoreError {
    StoreError::InvalidData {
        field,
        message: error.to_string(),
    }
}
