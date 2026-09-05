use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageFilter {
    pub from: i64,
    pub to: i64,
    pub user_key_id: Option<i64>,
    pub user_id: Option<i64>,
    pub provider_id: Option<i64>,
    pub credential_id: Option<i64>,
    pub model: Option<String>,
    pub request_id: Option<String>,
    pub operation: Option<String>,
    pub usage_source: Option<String>,
    pub ended: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UsageTotals {
    pub requests: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
    pub cost: Decimal,
    pub metrics: BTreeMap<String, Decimal>,
}

impl UsageTotals {
    pub fn add(&mut self, usage: &super::UsageInput) -> Result<(), crate::StoreError> {
        self.requests += 1;
        self.input_tokens += usage.input_tokens;
        self.output_tokens += usage.output_tokens;
        self.cached_input_tokens += usage.cached_input_tokens;
        self.cost += usage.cost;
        let metrics: BTreeMap<String, Decimal> = serde_json::from_value(usage.metrics.clone())
            .map_err(|error| crate::StoreError::InvalidData {
                field: "metrics_json",
                message: error.to_string(),
            })?;
        for (name, amount) in metrics {
            *self.metrics.entry(name).or_default() += amount;
        }
        Ok(())
    }

    pub fn total_tokens(&self) -> Decimal {
        Decimal::from(self.input_tokens)
            + Decimal::from(self.output_tokens)
            + [
                "cache_creation_5m_tokens",
                "cache_creation_30m_tokens",
                "cache_creation_1h_tokens",
            ]
            .iter()
            .filter_map(|key| self.metrics.get(*key))
            .copied()
            .sum::<Decimal>()
    }
}
