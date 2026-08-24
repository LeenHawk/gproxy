use std::collections::BTreeMap;

use rust_decimal::Decimal;
use serde_json::Value;

use crate::query::runtime;
use crate::records::CredentialQuotaObservation;
use crate::{Store, StoreError};

pub(super) async fn collect(
    store: &Store,
    input: &CredentialQuotaObservation,
) -> Result<Value, StoreError> {
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
) -> Result<Value, StoreError> {
    let Some(period_start) = period_start else {
        return Ok(Value::Object(serde_json::Map::new()));
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
    let mut totals = BTreeMap::from([
        ("requests".to_owned(), Decimal::ZERO),
        ("input_tokens".to_owned(), Decimal::ZERO),
        ("output_tokens".to_owned(), Decimal::ZERO),
        ("cached_input_tokens".to_owned(), Decimal::ZERO),
        ("cost".to_owned(), Decimal::ZERO),
    ]);
    for row in rows {
        add(&mut totals, "requests", Decimal::ONE);
        for field in ["input_tokens", "output_tokens", "cached_input_tokens"] {
            add(&mut totals, field, Decimal::from(row.i64(field)?));
        }
        let cost = row
            .text("cost")?
            .parse::<Decimal>()
            .map_err(|error| invalid("cost", error))?;
        add(&mut totals, "cost", cost);
        let metrics =
            serde_json::from_str::<serde_json::Map<String, Value>>(row.text("metrics_json")?)
                .map_err(|error| invalid("metrics_json", error))?;
        for (metric, value) in metrics {
            let amount = serde_json::from_value::<Decimal>(value)
                .map_err(|error| invalid("metrics_json", error))?;
            add(&mut totals, &metric, amount);
        }
    }
    serde_json::to_value(totals).map_err(|error| invalid("metrics_json", error))
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
