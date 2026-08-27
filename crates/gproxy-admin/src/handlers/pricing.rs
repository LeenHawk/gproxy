use bytes::Bytes;
use gproxy_store::records::{PriceRateInput, PriceRuleInput};
use http::{Response, StatusCode};

use crate::dto::{PriceRateDto, PriceRateWriteRequest, PriceRuleDto, PriceRuleWriteRequest};
use crate::handlers::util;
use crate::route::Entity;
use crate::{AdminError, State, response};

pub(super) async fn list(
    state: &impl State,
    entity: Entity,
) -> Result<Response<Bytes>, AdminError> {
    let snapshot = state.store().control_snapshot().await?;
    match entity {
        Entity::PriceRules => response::json(
            StatusCode::OK,
            &snapshot
                .price_rules
                .iter()
                .map(|value| PriceRuleDto {
                    id: value.id,
                    provider_id: value.provider_id,
                    model_pattern: value.model_pattern.clone(),
                    tiers: value.tiers.clone(),
                    priority: value.priority,
                    enabled: value.enabled,
                })
                .collect::<Vec<_>>(),
        ),
        Entity::PriceRates => response::json(
            StatusCode::OK,
            &snapshot
                .price_rates
                .iter()
                .map(|value| PriceRateDto {
                    id: value.id,
                    rule_id: value.rule_id,
                    metric: value.metric.clone(),
                    unit_size: value.unit_size,
                    price: value.price.normalize().to_string(),
                    conditions: value.conditions.clone(),
                    priority: value.priority,
                })
                .collect::<Vec<_>>(),
        ),
        _ => Err(AdminError::NotFound),
    }
}

pub(super) async fn create(
    state: &impl State,
    entity: Entity,
    body: &Bytes,
) -> Result<Response<Bytes>, AdminError> {
    let id = match entity {
        Entity::PriceRules => {
            let input = rule(util::parse(body)?)?;
            super::control::validators::price_rule(state, input.provider_id).await?;
            state.store().insert_price_rule(&input).await?
        }
        Entity::PriceRates => {
            let input = rate(util::parse(body)?)?;
            super::control::validators::price_rate(state, input.rule_id).await?;
            state.store().insert_price_rate(&input).await?
        }
        _ => return Err(AdminError::NotFound),
    };
    util::created(state, id).await
}

pub(super) async fn update(
    state: &impl State,
    entity: Entity,
    id: i64,
    body: &Bytes,
) -> Result<Response<Bytes>, AdminError> {
    let applied = match entity {
        Entity::PriceRules => {
            let input = rule(util::parse(body)?)?;
            super::control::validators::price_rule(state, input.provider_id).await?;
            state.store().update_price_rule(id, &input).await?
        }
        Entity::PriceRates => {
            let input = rate(util::parse(body)?)?;
            super::control::validators::price_rate(state, input.rule_id).await?;
            state.store().update_price_rate(id, &input).await?
        }
        _ => return Err(AdminError::NotFound),
    };
    util::updated(state, applied).await
}

pub(super) async fn delete(
    state: &impl State,
    entity: Entity,
    id: i64,
) -> Result<Response<Bytes>, AdminError> {
    let applied = match entity {
        Entity::PriceRules => state.store().delete_price_rule(id).await?,
        Entity::PriceRates => state.store().delete_price_rate(id).await?,
        _ => return Err(AdminError::NotFound),
    };
    util::updated(state, applied).await
}

fn rule(request: PriceRuleWriteRequest) -> Result<PriceRuleInput, AdminError> {
    if request.model_pattern.trim().is_empty() {
        return Err(AdminError::BadRequest(
            "price rule model_pattern must not be blank".into(),
        ));
    }
    Ok(PriceRuleInput {
        provider_id: request.provider_id,
        model_pattern: request.model_pattern,
        tiers: request.tiers,
        priority: request.priority,
        enabled: request.enabled,
    })
}

fn rate(request: PriceRateWriteRequest) -> Result<PriceRateInput, AdminError> {
    if request.metric.trim().is_empty() || request.unit_size == 0 {
        return Err(AdminError::BadRequest(
            "price rate metric must not be blank and unit_size must be positive".into(),
        ));
    }
    let price = request
        .price
        .parse::<rust_decimal::Decimal>()
        .map_err(|_| AdminError::BadRequest("price must be a decimal".into()))?;
    if price < rust_decimal::Decimal::ZERO {
        return Err(AdminError::BadRequest("price must not be negative".into()));
    }
    Ok(PriceRateInput {
        rule_id: request.rule_id,
        metric: request.metric,
        unit_size: request.unit_size,
        price,
        conditions: request.conditions,
        priority: request.priority,
    })
}
