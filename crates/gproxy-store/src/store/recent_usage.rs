use crate::backend::Row;
use crate::query::usage;
use crate::records::RecentUsageRecord;
use crate::{Store, StoreError};

impl Store {
    pub async fn recent_usage_for_key(
        &self,
        user_key_id: i64,
        limit: u64,
    ) -> Result<Vec<RecentUsageRecord>, StoreError> {
        self.backend()
            .execute(usage::recent_for_key(user_key_id, limit)?)
            .await?
            .rows
            .into_iter()
            .map(parse)
            .collect()
    }
}

fn parse(row: Row) -> Result<RecentUsageRecord, StoreError> {
    Ok(RecentUsageRecord {
        request_id: row.text("request_id")?.to_owned(),
        at: row.i64("at")?,
        provider_id: row.i64("provider_id")?,
        operation: row.optional_text("operation")?.map(str::to_owned),
        upstream_model: row.text("upstream_model")?.to_owned(),
        input_tokens: super::usage::unsigned(row.i64("input_tokens")?, "input_tokens")?,
        output_tokens: super::usage::unsigned(row.i64("output_tokens")?, "output_tokens")?,
        cached_input_tokens: super::usage::unsigned(
            row.i64("cached_input_tokens")?,
            "cached_input_tokens",
        )?,
        cost: super::usage::decimal(row.text("cost")?, "cost")?,
        usage_source: row.text("usage_source")?.to_owned(),
        ended: row.text("ended")?.to_owned(),
        latency_ms: super::usage::unsigned(row.i64("latency_ms")?, "latency_ms")?,
    })
}
