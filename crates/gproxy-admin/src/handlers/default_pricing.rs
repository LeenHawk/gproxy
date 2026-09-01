use std::collections::BTreeSet;
use std::sync::LazyLock;

use bytes::Bytes;
use gproxy_store::Store;
use gproxy_store::records::{PriceRateInput, PriceRuleInput, RecordBatch};
use http::{Response, StatusCode};
use rust_decimal::Decimal;

use crate::dto::{
    ApplyDefaultPricesRequest, ApplyDefaultPricesResponse, DefaultPriceCatalogDto,
    DefaultPriceRuleDto,
};
use crate::handlers::util;
use crate::{AdminError, State, response};

const CATALOG_JSON: &[u8] = include_bytes!("../../assets/openrouter-price-catalog.json");
const RULE_BATCH_SIZE: usize = 200;
const RATE_BATCH_SIZE: usize = 500;
static CATALOG: LazyLock<Result<DefaultPriceCatalogDto, String>> =
    LazyLock::new(|| parse_catalog().map_err(|error| error.to_string()));

struct Pending<'a> {
    source: &'a DefaultPriceRuleDto,
    provider_id: Option<i64>,
    model_pattern: String,
    priority: i64,
}

pub(super) fn list() -> Result<Response<Bytes>, AdminError> {
    response::json(StatusCode::OK, catalog()?)
}

pub(crate) async fn seed_global(store: &Store) -> Result<usize, AdminError> {
    let pending = catalog()?
        .price_rules
        .iter()
        .map(|source| Pending {
            source,
            provider_id: None,
            model_pattern: source.model_pattern.clone(),
            priority: source.priority,
        })
        .collect();
    insert(store, pending).await
}

pub(super) async fn apply(state: &impl State, body: &Bytes) -> Result<Response<Bytes>, AdminError> {
    let request: ApplyDefaultPricesRequest = util::parse(body)?;
    if request.model_ids.is_empty() {
        return Err(AdminError::BadRequest(
            "default price model_ids must not be empty".into(),
        ));
    }
    super::control::validators::provider(state, request.provider_id).await?;
    let selected = request.model_ids.iter().collect::<BTreeSet<_>>();
    if selected.len() != request.model_ids.len()
        || selected.iter().any(|model| model.trim().is_empty())
    {
        return Err(AdminError::BadRequest(
            "default price model_ids must be unique and nonblank".into(),
        ));
    }

    let snapshot = state.store().control_snapshot().await?;
    let existing = snapshot
        .price_rules
        .iter()
        .filter(|rule| rule.provider_id == Some(request.provider_id))
        .map(|rule| rule.model_pattern.as_str())
        .collect::<BTreeSet<_>>();
    let mut skipped = 0;
    let mut unmatched = 0;
    let mut pending = Vec::new();
    for model in selected {
        if existing.contains(model.as_str()) {
            skipped += 1;
        } else if let Some(source) = match_model(model) {
            pending.push(Pending {
                source,
                provider_id: Some(request.provider_id),
                model_pattern: model.clone(),
                priority: 0,
            });
        } else {
            unmatched += 1;
        }
    }
    let created = insert(state.store(), pending).await?;
    if created > 0 {
        state.reload().await?;
    }
    response::json(
        if created > 0 {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
        &ApplyDefaultPricesResponse {
            created,
            skipped,
            unmatched,
        },
    )
}

fn match_model(model: &str) -> Option<&'static DefaultPriceRuleDto> {
    let model = model.to_ascii_lowercase();
    catalog()
        .ok()?
        .price_rules
        .iter()
        .filter_map(|rule| {
            let needle = rule.model_pattern.strip_prefix('*')?.strip_suffix('*')?;
            model
                .contains(&needle.to_ascii_lowercase())
                .then_some((needle.len(), rule))
        })
        .fold(None, |best, candidate| match best {
            Some((length, _)) if length >= candidate.0 => best,
            _ => Some(candidate),
        })
        .map(|(_, rule)| rule)
}

pub(crate) fn embedded_global_rule_ids(
    rules: &[gproxy_store::records::PriceRuleRecord],
    rates: &[gproxy_store::records::PriceRateRecord],
) -> BTreeSet<i64> {
    let Ok(catalog) = catalog() else {
        return BTreeSet::new();
    };
    rules
        .iter()
        .filter(|rule| rule.provider_id.is_none() && rule.enabled)
        .filter(|rule| {
            let Some(source) = catalog
                .price_rules
                .iter()
                .find(|source| source.model_pattern == rule.model_pattern)
            else {
                return false;
            };
            if rule.priority != source.priority || rule.tiers != source.tiers {
                return false;
            }
            let actual = rates
                .iter()
                .filter(|rate| rate.rule_id == rule.id)
                .collect::<Vec<_>>();
            actual.len() == source.rates.len()
                && source.rates.iter().all(|expected| {
                    actual.iter().any(|rate| {
                        rate.metric == expected.metric
                            && rate.unit_size == expected.unit_size
                            && rate.price.to_string() == expected.price
                            && rate.conditions.is_none()
                            && rate.priority == expected.priority
                    })
                })
        })
        .map(|rule| rule.id)
        .collect()
}

async fn insert(store: &Store, pending: Vec<Pending<'_>>) -> Result<usize, AdminError> {
    if pending.is_empty() {
        return Ok(0);
    }
    let mut inserted = Vec::with_capacity(pending.len());
    for chunk in pending.chunks(RULE_BATCH_SIZE) {
        let rules = chunk
            .iter()
            .map(|entry| PriceRuleInput {
                provider_id: entry.provider_id,
                model_pattern: entry.model_pattern.clone(),
                tiers: entry.source.tiers.clone(),
                priority: entry.priority,
                enabled: true,
            })
            .collect();
        match store
            .insert_record_batch(RecordBatch::PriceRules(rules))
            .await
        {
            Ok(ids) => inserted.extend(chunk.iter().zip(ids)),
            Err(error) => {
                cleanup(store, &inserted).await;
                return Err(error.into());
            }
        }
    }
    let rates = inserted
        .iter()
        .flat_map(|(entry, id)| rate_inputs(entry.source, *id))
        .collect::<Vec<_>>();
    for chunk in rates.chunks(RATE_BATCH_SIZE) {
        if let Err(error) = store
            .insert_record_batch(RecordBatch::PriceRates(chunk.to_vec()))
            .await
        {
            cleanup(store, &inserted).await;
            return Err(error.into());
        }
    }
    Ok(inserted.len())
}

async fn cleanup(store: &Store, inserted: &[(&Pending<'_>, i64)]) {
    for (_, id) in inserted {
        let _ = store.delete_price_rule(*id).await;
    }
}

fn catalog() -> Result<&'static DefaultPriceCatalogDto, AdminError> {
    CATALOG
        .as_ref()
        .map_err(|error| AdminError::Internal(error.clone()))
}

fn parse_catalog() -> Result<DefaultPriceCatalogDto, AdminError> {
    let catalog: DefaultPriceCatalogDto = serde_json::from_slice(CATALOG_JSON)
        .map_err(|error| AdminError::Internal(format!("default price catalog: {error}")))?;
    if catalog.schema_version != 1
        || catalog.source.included_models != catalog.price_rules.len()
        || catalog.source.catalog != "openrouter"
    {
        return Err(AdminError::Internal(
            "default price catalog metadata is invalid".into(),
        ));
    }
    let mut model_ids = BTreeSet::new();
    let mut patterns = BTreeSet::new();
    for rule in &catalog.price_rules {
        let match_text = rule
            .model_pattern
            .strip_prefix('*')
            .and_then(|value| value.strip_suffix('*'));
        if rule.model_id.trim().is_empty()
            || match_text.is_none_or(str::is_empty)
            || rule.rates.is_empty()
            || !model_ids.insert(rule.model_id.as_str())
            || !patterns.insert(rule.model_pattern.as_str())
        {
            return Err(AdminError::Internal(
                "default price catalog contains an invalid rule".into(),
            ));
        }
        gproxy_store::records::parse_price_tiers(rule.tiers.as_ref())?;
        for rate in &rule.rates {
            let price = rate.price.parse::<Decimal>().map_err(|_| {
                AdminError::Internal("default price catalog contains an invalid price".into())
            })?;
            if rate.metric.trim().is_empty() || rate.unit_size == 0 || price < Decimal::ZERO {
                return Err(AdminError::Internal(
                    "default price catalog contains an invalid rate".into(),
                ));
            }
        }
    }
    Ok(catalog)
}

fn rate_inputs(rule: &DefaultPriceRuleDto, rule_id: i64) -> Vec<PriceRateInput> {
    rule.rates
        .iter()
        .map(|rate| PriceRateInput {
            rule_id,
            metric: rate.metric.clone(),
            unit_size: rate.unit_size,
            price: rate
                .price
                .parse()
                .expect("embedded price catalog was validated"),
            conditions: None,
            priority: rate.priority,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn embedded_catalog_is_valid_and_prefers_the_longest_match() {
        let catalog = super::catalog().expect("embedded catalog");
        assert_eq!(catalog.price_rules.len(), 493);
        let matched = super::match_model("OPENAI/GPT-5.6-SOL-PRO:BATCH").expect("match");
        assert_eq!(matched.model_id, "openai/gpt-5.6-sol-pro:batch");
    }
}
