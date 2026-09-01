use crate::backend::Row;
use crate::query::usage;
use crate::records::{
    UsageAggregateQuery, UsageAggregateRecord, UsageInput, UsageRecord, UsageWindow,
};
use crate::{Store, StoreError};
use rust_decimal::prelude::ToPrimitive as _;

impl Store {
    pub async fn usage_count(&self) -> Result<u64, StoreError> {
        let mut result = self.backend().execute(usage::usage_count()?).await?;
        let row = result
            .rows
            .pop()
            .ok_or_else(|| StoreError::Database("usage count row missing".into()))?;
        unsigned(row.i64("count")?, "usage count")
    }

    pub async fn usage_aggregate(
        &self,
        query: &UsageAggregateQuery,
    ) -> Result<Vec<UsageAggregateRecord>, StoreError> {
        const PAGE_SIZE: u64 = 5_000;
        const MAX_PAGES: usize = 20;
        let mut groups = std::collections::BTreeMap::<AggregateKey, UsageAggregateRecord>::new();
        let mut after_id = 0;
        for page in 0..MAX_PAGES {
            let page_limit = if page + 1 == MAX_PAGES {
                PAGE_SIZE + 1
            } else {
                PAGE_SIZE
            };
            let rows = self
                .backend()
                .execute(usage::aggregate(query, after_id, page_limit)?)
                .await?
                .rows;
            let row_count = rows.len();
            if page + 1 == MAX_PAGES && row_count > PAGE_SIZE as usize {
                return Err(StoreError::InvalidData {
                    field: "usage query",
                    message: "range exceeds 100000 rows; narrow the time range".into(),
                });
            }
            for row in rows {
                after_id = row.i64("id")?;
                accumulate(&mut groups, query.group_by, row)?;
            }
            if row_count < page_limit as usize {
                break;
            }
        }
        let mut values = groups.into_values().collect::<Vec<_>>();
        values.sort_by(|left, right| {
            right
                .cost
                .cmp(&left.cost)
                .then_with(|| left.provider_id.cmp(&right.provider_id))
                .then_with(|| left.model.cmp(&right.model))
                .then_with(|| left.user_id.cmp(&right.user_id))
                .then_with(|| left.user_key_id.cmp(&right.user_key_id))
        });
        values.truncate(500);
        Ok(values)
    }

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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum AggregateKey {
    Scalar(String),
    Dimensions(Option<i64>, Option<i64>, i64, String),
}

fn accumulate(
    groups: &mut std::collections::BTreeMap<AggregateKey, UsageAggregateRecord>,
    group_by: crate::records::UsageGroupBy,
    row: Row,
) -> Result<(), StoreError> {
    let user_key_id = row.optional_i64("user_key_id")?;
    let user_id = row.optional_i64("user_id")?;
    let provider_id = row.i64("provider_id")?;
    let model = row.text("upstream_model")?.to_owned();
    let group = match group_by {
        crate::records::UsageGroupBy::UserKey => required_group(user_key_id, "user_key_id")?,
        crate::records::UsageGroupBy::User => required_group(user_id, "user_id")?,
        crate::records::UsageGroupBy::Provider => provider_id.to_string(),
        crate::records::UsageGroupBy::Model | crate::records::UsageGroupBy::Dimensions => {
            model.clone()
        }
    };
    let key = match group_by {
        crate::records::UsageGroupBy::Dimensions => {
            AggregateKey::Dimensions(user_key_id, user_id, provider_id, model.clone())
        }
        _ => AggregateKey::Scalar(group.clone()),
    };
    let metrics = json(row.text("metrics_json")?, "metrics_json")?;
    let value = groups.entry(key).or_insert(UsageAggregateRecord {
        group,
        user_key_id,
        user_id,
        provider_id,
        model,
        requests: 0,
        input_tokens: 0,
        output_tokens: 0,
        cached_input_tokens: 0,
        cache_creation_5m_tokens: 0,
        cache_creation_30m_tokens: 0,
        cache_creation_1h_tokens: 0,
        cost: rust_decimal::Decimal::ZERO,
    });
    checked_add(&mut value.requests, 1, "requests")?;
    checked_add(
        &mut value.input_tokens,
        unsigned(row.i64("input_tokens")?, "input_tokens")?,
        "input_tokens",
    )?;
    checked_add(
        &mut value.output_tokens,
        unsigned(row.i64("output_tokens")?, "output_tokens")?,
        "output_tokens",
    )?;
    checked_add(
        &mut value.cached_input_tokens,
        unsigned(row.i64("cached_input_tokens")?, "cached_input_tokens")?,
        "cached_input_tokens",
    )?;
    for (target, name) in [
        (
            &mut value.cache_creation_5m_tokens,
            "cache_creation_5m_tokens",
        ),
        (
            &mut value.cache_creation_30m_tokens,
            "cache_creation_30m_tokens",
        ),
        (
            &mut value.cache_creation_1h_tokens,
            "cache_creation_1h_tokens",
        ),
    ] {
        checked_add(target, metric_tokens(&metrics, name)?, name)?;
    }
    value.cost += decimal(row.text("cost")?, "cost")?;
    Ok(())
}

fn required_group(value: Option<i64>, field: &'static str) -> Result<String, StoreError> {
    value
        .map(|value| value.to_string())
        .ok_or_else(|| StoreError::InvalidData {
            field,
            message: "usage group key is null".into(),
        })
}

fn metric_tokens(metrics: &serde_json::Value, field: &'static str) -> Result<u64, StoreError> {
    let Some(value) = metrics.get(field) else {
        return Ok(0);
    };
    let value = match value {
        serde_json::Value::Number(value) => value.to_string(),
        serde_json::Value::String(value) => value.clone(),
        _ => {
            return Err(StoreError::InvalidData {
                field,
                message: "usage token metric must be a number".into(),
            });
        }
    };
    let value = value
        .parse::<rust_decimal::Decimal>()
        .map_err(|error| invalid(field, error))?;
    if !value.fract().is_zero() {
        return Err(StoreError::InvalidData {
            field,
            message: "usage token metric must be an integer".into(),
        });
    }
    value.to_u64().ok_or_else(|| StoreError::InvalidData {
        field,
        message: "usage token metric is outside the u64 range".into(),
    })
}

fn checked_add(target: &mut u64, value: u64, field: &'static str) -> Result<(), StoreError> {
    *target = target
        .checked_add(value)
        .ok_or_else(|| StoreError::InvalidData {
            field,
            message: "aggregate exceeds u64".into(),
        })?;
    Ok(())
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

pub(super) fn unsigned(value: i64, field: &'static str) -> Result<u64, StoreError> {
    u64::try_from(value).map_err(|error| invalid(field, error))
}

pub(super) fn decimal(
    value: &str,
    field: &'static str,
) -> Result<rust_decimal::Decimal, StoreError> {
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
