use std::collections::BTreeSet;

use bytes::Bytes;
use gproxy_store::Store;
use gproxy_store::records::{PriceRateInput, PriceRuleInput, RecordBatch};
use http::{Response, StatusCode};

use crate::dto::{
    ApplyDefaultModelPricesRequest, ApplyDefaultModelPricesResponse, DefaultModelPricingDto,
};
use crate::handlers::util;
use crate::{AdminError, State, response};

const RULE_BATCH_SIZE: usize = 200;
const RATE_BATCH_SIZE: usize = 500;

struct Pending<'a> {
    source: &'a DefaultModelPricingDto,
    provider_id: Option<i64>,
    model_pattern: String,
    priority: i64,
}

pub(crate) async fn seed_global(store: &Store) -> Result<usize, AdminError> {
    let pending = super::catalog::catalog()?
        .models
        .iter()
        .filter_map(|model| model.pricing.as_ref())
        .map(|source| Pending {
            source,
            provider_id: None,
            model_pattern: source.model_pattern.clone(),
            priority: source.priority,
        })
        .collect();
    insert(store, pending).await
}

pub(crate) async fn apply(state: &impl State, body: &Bytes) -> Result<Response<Bytes>, AdminError> {
    let request: ApplyDefaultModelPricesRequest = util::parse(body)?;
    if request.model_ids.is_empty() {
        return Err(AdminError::BadRequest(
            "default model price ids must not be empty".into(),
        ));
    }
    if let Some(provider_id) = request.provider_id {
        super::super::control::validators::provider(state, provider_id).await?;
    }
    let selected = request.model_ids.iter().collect::<BTreeSet<_>>();
    if selected.len() != request.model_ids.len()
        || selected.iter().any(|model| model.trim().is_empty())
    {
        return Err(AdminError::BadRequest(
            "default model price ids must be unique and nonblank".into(),
        ));
    }
    let snapshot = state.store().control_snapshot().await?;
    let mut existing = snapshot
        .price_rules
        .iter()
        .filter(|rule| rule.provider_id == request.provider_id)
        .map(|rule| rule.model_pattern.clone())
        .collect::<BTreeSet<_>>();
    let mut skipped = 0;
    let mut unmatched = 0;
    let mut pending = Vec::new();
    for model in selected {
        if existing.contains(model.as_str()) {
            skipped += 1;
            continue;
        }
        let source = if request.provider_id.is_none() {
            super::catalog::model(model).and_then(|model| model.pricing.as_ref())
        } else {
            super::catalog::price(model)
        };
        if let Some(source) = source {
            let (model_pattern, priority) = if request.provider_id.is_none() {
                (source.model_pattern.clone(), source.priority)
            } else {
                (model.clone(), 0)
            };
            if !existing.insert(model_pattern.clone()) {
                skipped += 1;
                continue;
            }
            pending.push(Pending {
                source,
                provider_id: request.provider_id,
                model_pattern,
                priority,
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
        &ApplyDefaultModelPricesResponse {
            created,
            skipped,
            unmatched,
        },
    )
}

pub(crate) fn embedded_global_rule_ids(
    rules: &[gproxy_store::records::PriceRuleRecord],
    rates: &[gproxy_store::records::PriceRateRecord],
) -> BTreeSet<i64> {
    rules
        .iter()
        .filter(|rule| rule.provider_id.is_none() && rule.enabled)
        .filter(|rule| matches_embedded(rule, rates))
        .map(|rule| rule.id)
        .collect()
}

fn matches_embedded(
    rule: &gproxy_store::records::PriceRuleRecord,
    rates: &[gproxy_store::records::PriceRateRecord],
) -> bool {
    let Some(source) = super::catalog::catalog().ok().and_then(|catalog| {
        catalog.models.iter().find_map(|model| {
            model
                .pricing
                .as_ref()
                .filter(|pricing| pricing.model_pattern == rule.model_pattern)
        })
    }) else {
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

fn rate_inputs(source: &DefaultModelPricingDto, rule_id: i64) -> Vec<PriceRateInput> {
    source
        .rates
        .iter()
        .map(|rate| PriceRateInput {
            rule_id,
            metric: rate.metric.clone(),
            unit_size: rate.unit_size,
            price: rate
                .price
                .parse()
                .expect("embedded model catalog was validated"),
            conditions: None,
            priority: rate.priority,
        })
        .collect()
}
