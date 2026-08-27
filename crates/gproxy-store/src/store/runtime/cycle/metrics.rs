use std::collections::BTreeMap;

use rust_decimal::Decimal;
use serde_json::Value;

use crate::query::runtime;
use crate::records::{CredentialQuotaCycleModelRecord, CredentialQuotaObservation};
use crate::{Store, StoreError};

pub(super) struct Collected {
    pub totals: Value,
    pub models: Vec<CredentialQuotaCycleModelRecord>,
}

pub(super) async fn collect(
    store: &Store,
    input: &CredentialQuotaObservation,
) -> Result<Collected, StoreError> {
    collect_range(
        store,
        input.credential_id,
        input.period_start,
        input.observed_at,
    )
    .await
}

pub(super) async fn collect_range(
    store: &Store,
    credential_id: i64,
    period_start: Option<i64>,
    observed_at: i64,
) -> Result<Collected, StoreError> {
    let Some(period_start) = period_start else {
        return Ok(Collected {
            totals: Value::Object(serde_json::Map::new()),
            models: Vec::new(),
        });
    };
    let rows = store
        .backend()
        .execute(runtime::select_credential_cycle_usage(
            credential_id,
            period_start,
            observed_at,
        )?)
        .await?
        .rows;
    let mut totals = empty_totals();
    let mut models = BTreeMap::<String, BTreeMap<String, Decimal>>::new();
    for row in rows {
        let model = row.text("upstream_model")?.to_owned();
        let model_totals = models.entry(model).or_insert_with(empty_totals);
        add_row(&row, &mut totals)?;
        add_row(&row, model_totals)?;
    }
    let totals = serde_json::to_value(totals).map_err(|error| invalid("metrics_json", error))?;
    let models = models
        .into_iter()
        .map(|(model, metrics)| {
            Ok(CredentialQuotaCycleModelRecord {
                model,
                metrics: serde_json::to_value(metrics)
                    .map_err(|error| invalid("metrics_json", error))?,
            })
        })
        .collect::<Result<Vec<_>, StoreError>>()?;
    Ok(Collected { totals, models })
}

fn empty_totals() -> BTreeMap<String, Decimal> {
    BTreeMap::from([
        ("requests".to_owned(), Decimal::ZERO),
        ("input_tokens".to_owned(), Decimal::ZERO),
        ("output_tokens".to_owned(), Decimal::ZERO),
        ("cached_input_tokens".to_owned(), Decimal::ZERO),
        ("cost".to_owned(), Decimal::ZERO),
    ])
}

fn add_row(
    row: &crate::backend::Row,
    totals: &mut BTreeMap<String, Decimal>,
) -> Result<(), StoreError> {
    add(totals, "requests", Decimal::ONE);
    for field in ["input_tokens", "output_tokens", "cached_input_tokens"] {
        add(totals, field, Decimal::from(row.i64(field)?));
    }
    let cost = row
        .text("cost")?
        .parse::<Decimal>()
        .map_err(|error| invalid("cost", error))?;
    add(totals, "cost", cost);
    let metrics = serde_json::from_str::<serde_json::Map<String, Value>>(row.text("metrics_json")?)
        .map_err(|error| invalid("metrics_json", error))?;
    for (metric, value) in metrics {
        let amount = serde_json::from_value::<Decimal>(value)
            .map_err(|error| invalid("metrics_json", error))?;
        add(totals, &metric, amount);
    }
    Ok(())
}

fn add(totals: &mut BTreeMap<String, Decimal>, metric: &str, amount: Decimal) {
    *totals.entry(metric.to_owned()).or_default() += amount;
}

fn invalid(field: &'static str, error: impl std::fmt::Display) -> StoreError {
    StoreError::InvalidData {
        field,
        message: error.to_string(),
    }
}
