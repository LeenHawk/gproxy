//! Local, provider-independent credential usage views.
//!
//! Every credential gets these summaries, even when its channel has no live
//! upstream quota endpoint. Monetary values are the historical settled cost
//! stored on usage rows; they are never repriced at read time.

use rust_decimal::Decimal;
use serde::Serialize;

use crate::store::persistence::records::{CredentialUsageDaily, UsageModelSummary, UsageSummary};

const DAY_SECONDS: i64 = 86_400;

#[derive(Debug, Clone, Serialize)]
pub struct CredentialUsageTotals {
    pub requests: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub image_output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
    pub total_tokens: i64,
    pub cost_usd: String,
}

impl Default for CredentialUsageTotals {
    fn default() -> Self {
        Self {
            requests: 0,
            input_tokens: 0,
            output_tokens: 0,
            image_output_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            total_tokens: 0,
            cost_usd: "0".to_owned(),
        }
    }
}

impl CredentialUsageTotals {
    pub fn from_summary(summary: &UsageSummary) -> Self {
        Self::new(
            summary.requests,
            summary.input_tokens,
            summary.output_tokens,
            summary.image_output_tokens,
            summary.cache_read_tokens,
            summary.cache_creation_5m_tokens,
            summary.cache_creation_30m_tokens,
            summary.cache_creation_1h_tokens,
            summary.cost,
        )
    }

    pub fn from_model(summary: &UsageModelSummary) -> Self {
        Self::new(
            summary.requests,
            summary.input_tokens,
            summary.output_tokens,
            summary.image_output_tokens,
            summary.cache_read_tokens,
            summary.cache_creation_5m_tokens,
            summary.cache_creation_30m_tokens,
            summary.cache_creation_1h_tokens,
            summary.cost,
        )
    }

    pub fn from_daily(summary: &CredentialUsageDaily) -> Self {
        Self::new(
            summary.requests,
            summary.input_tokens,
            summary.output_tokens,
            summary.image_output_tokens,
            summary.cache_read_tokens,
            summary.cache_creation_5m_tokens,
            summary.cache_creation_30m_tokens,
            summary.cache_creation_1h_tokens,
            summary.cost,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        requests: i64,
        input_tokens: i64,
        output_tokens: i64,
        image_output_tokens: i64,
        cache_read_tokens: i64,
        cache_creation_5m_tokens: i64,
        cache_creation_30m_tokens: i64,
        cache_creation_1h_tokens: i64,
        cost: Decimal,
    ) -> Self {
        let cache_creation_tokens = cache_creation_5m_tokens
            .saturating_add(cache_creation_30m_tokens)
            .saturating_add(cache_creation_1h_tokens);
        let total_tokens = input_tokens
            .saturating_add(output_tokens)
            .saturating_add(image_output_tokens)
            .saturating_add(cache_read_tokens)
            .saturating_add(cache_creation_tokens);
        Self {
            requests,
            input_tokens,
            output_tokens,
            image_output_tokens,
            cache_read_tokens,
            cache_creation_tokens,
            total_tokens,
            cost_usd: cost.normalize().to_string(),
        }
    }

    pub fn add_assign(&mut self, other: &Self) {
        self.requests = self.requests.saturating_add(other.requests);
        self.input_tokens = self.input_tokens.saturating_add(other.input_tokens);
        self.output_tokens = self.output_tokens.saturating_add(other.output_tokens);
        self.image_output_tokens = self
            .image_output_tokens
            .saturating_add(other.image_output_tokens);
        self.cache_read_tokens = self
            .cache_read_tokens
            .saturating_add(other.cache_read_tokens);
        self.cache_creation_tokens = self
            .cache_creation_tokens
            .saturating_add(other.cache_creation_tokens);
        self.total_tokens = self.total_tokens.saturating_add(other.total_tokens);
        let left = self.cost_usd.parse::<Decimal>().unwrap_or_default();
        let right = other.cost_usd.parse::<Decimal>().unwrap_or_default();
        self.cost_usd = (left + right).normalize().to_string();
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CredentialUsageModelTotals {
    pub model: String,
    #[serde(flatten)]
    pub totals: CredentialUsageTotals,
}

impl CredentialUsageModelTotals {
    pub fn from_summary(summary: &UsageModelSummary) -> Self {
        Self {
            model: summary
                .model
                .clone()
                .filter(|model| !model.is_empty())
                .unwrap_or_else(|| "unknown".to_owned()),
            totals: CredentialUsageTotals::from_model(summary),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CredentialUsageDay {
    pub day_start: i64,
    pub totals: CredentialUsageTotals,
}

#[derive(Debug, Clone, Serialize)]
pub struct CredentialUsageSummaryView {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coverage_start: Option<i64>,
    pub lifetime: CredentialUsageTotals,
    pub last_7_days: Vec<CredentialUsageDay>,
    pub by_model: Vec<CredentialUsageModelTotals>,
}

/// Fold permanent per-model daily rows into the compact credential view. Empty
/// days are materialized so the Console always receives a stable seven-point
/// series, including today.
pub fn summarize_daily(rows: &[CredentialUsageDaily], now: i64) -> CredentialUsageSummaryView {
    use std::collections::BTreeMap;

    let today = now - now.rem_euclid(DAY_SECONDS);
    let seven_day_start = today - 6 * DAY_SECONDS;
    let coverage_start = rows.iter().map(|row| row.day_start).min();
    let mut lifetime = CredentialUsageTotals::default();
    let mut days: BTreeMap<i64, CredentialUsageTotals> = (0..7)
        .map(|offset| (seven_day_start + offset * DAY_SECONDS, Default::default()))
        .collect();
    let mut models: BTreeMap<String, CredentialUsageTotals> = BTreeMap::new();

    for row in rows {
        let totals = CredentialUsageTotals::from_daily(row);
        lifetime.add_assign(&totals);
        if let Some(day) = days.get_mut(&row.day_start) {
            day.add_assign(&totals);
        }
        let model = row
            .model
            .clone()
            .filter(|model| !model.is_empty())
            .unwrap_or_else(|| "unknown".to_owned());
        models.entry(model).or_default().add_assign(&totals);
    }

    let mut by_model: Vec<_> = models
        .into_iter()
        .map(|(model, totals)| CredentialUsageModelTotals { model, totals })
        .collect();
    by_model.sort_by(|left, right| {
        right
            .totals
            .total_tokens
            .cmp(&left.totals.total_tokens)
            .then_with(|| left.model.cmp(&right.model))
    });

    CredentialUsageSummaryView {
        coverage_start,
        lifetime,
        last_7_days: days
            .into_iter()
            .map(|(day_start, totals)| CredentialUsageDay { day_start, totals })
            .collect(),
        by_model,
    }
}
