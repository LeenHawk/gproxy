//! Pricing rule ops for the `db` backend.

use std::collections::HashMap;

use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder,
};

use crate::store::persistence::db::entities::pricing::{price_rule, price_rule_rate};
use crate::store::persistence::records::{PriceRate, PriceRule, PriceRuleInput};

fn to_record(m: price_rule::Model, rates: Vec<PriceRate>) -> anyhow::Result<PriceRule> {
    let mut record = PriceRule {
        id: m.id,
        provider_id: m.provider_id,
        match_type: m.match_type,
        model_match: m.model_match,
        input_price: m.input_price.parse()?,
        output_price: m.output_price.parse()?,
        cache_read_price: m.cache_read_price.parse()?,
        cache_creation_5m_price: m.cache_creation_5m_price.parse()?,
        cache_creation_30m_price: m.cache_creation_30m_price.parse()?,
        cache_creation_1h_price: m.cache_creation_1h_price.parse()?,
        image_output_price: m.image_output_price.parse()?,
        pricing_tiers_json: m
            .pricing_tiers_json
            .map(|value| serde_json::from_str(&value))
            .transpose()?,
        rates,
        enabled: m.enabled,
        created_at: m.created_at,
        updated_at: m.updated_at,
    };
    record.apply_rate_projections();
    Ok(record)
}

pub async fn list(conn: &DatabaseConnection) -> anyhow::Result<Vec<PriceRule>> {
    let mut rates: HashMap<i64, Vec<PriceRate>> = HashMap::new();
    for rate in price_rule_rate::Entity::find()
        .order_by_asc(price_rule_rate::Column::SortOrder)
        .order_by_asc(price_rule_rate::Column::Id)
        .all(conn)
        .await?
    {
        rates
            .entry(rate.price_rule_id)
            .or_default()
            .push(PriceRate {
                metric: rate.metric,
                unit: rate.unit,
                unit_size: u64::try_from(rate.unit_size).unwrap_or(1),
                price_usd: rate.price_usd.parse()?,
                conditions_json: rate
                    .conditions_json
                    .map(|value| serde_json::from_str(&value))
                    .transpose()?,
                sort_order: rate.sort_order,
            });
    }
    price_rule::Entity::find()
        .all(conn)
        .await?
        .into_iter()
        .map(|rule| {
            let rule_rates = rates.remove(&rule.id).unwrap_or_default();
            to_record(rule, rule_rates)
        })
        .collect()
}

pub async fn upsert(conn: &DatabaseConnection, input: PriceRuleInput) -> anyhow::Result<PriceRule> {
    input.validate_rates()?;
    let rates = input.effective_rates();
    let now = crate::store::persistence::db::ops::now_secs();
    let input_price = input.input_price.to_string();
    let output_price = input.output_price.to_string();
    let cache_read_price = input.cache_read_price.to_string();
    let cache_creation_5m_price = input.cache_creation_5m_price.to_string();
    let cache_creation_30m_price = input.cache_creation_30m_price.to_string();
    let cache_creation_1h_price = input.cache_creation_1h_price.to_string();
    let image_output_price = input.image_output_price.to_string();
    let pricing_tiers_json = input
        .pricing_tiers_json
        .as_ref()
        .map(serde_json::to_string)
        .transpose()?;

    let model = match input.id {
        Some(id) => match price_rule::Entity::find_by_id(id).one(conn).await? {
            Some(existing) => {
                let mut am: price_rule::ActiveModel = existing.into();
                am.provider_id = Set(input.provider_id);
                am.match_type = Set(input.match_type);
                am.model_match = Set(input.model_match);
                am.input_price = Set(input_price);
                am.output_price = Set(output_price);
                am.cache_read_price = Set(cache_read_price);
                am.cache_creation_5m_price = Set(cache_creation_5m_price);
                am.cache_creation_30m_price = Set(cache_creation_30m_price);
                am.cache_creation_1h_price = Set(cache_creation_1h_price);
                am.image_output_price = Set(image_output_price);
                am.pricing_tiers_json = Set(pricing_tiers_json);
                am.enabled = Set(input.enabled);
                am.updated_at = Set(now);
                am.update(conn).await?
            }
            None => {
                price_rule::ActiveModel {
                    id: Set(id),
                    provider_id: Set(input.provider_id),
                    match_type: Set(input.match_type),
                    model_match: Set(input.model_match),
                    input_price: Set(input_price),
                    output_price: Set(output_price),
                    cache_read_price: Set(cache_read_price),
                    cache_creation_5m_price: Set(cache_creation_5m_price),
                    cache_creation_30m_price: Set(cache_creation_30m_price),
                    cache_creation_1h_price: Set(cache_creation_1h_price),
                    image_output_price: Set(image_output_price),
                    pricing_tiers_json: Set(pricing_tiers_json),
                    enabled: Set(input.enabled),
                    created_at: Set(now),
                    updated_at: Set(now),
                }
                .insert(conn)
                .await?
            }
        },
        None => {
            price_rule::ActiveModel {
                id: NotSet,
                provider_id: Set(input.provider_id),
                match_type: Set(input.match_type),
                model_match: Set(input.model_match),
                input_price: Set(input_price),
                output_price: Set(output_price),
                cache_read_price: Set(cache_read_price),
                cache_creation_5m_price: Set(cache_creation_5m_price),
                cache_creation_30m_price: Set(cache_creation_30m_price),
                cache_creation_1h_price: Set(cache_creation_1h_price),
                image_output_price: Set(image_output_price),
                pricing_tiers_json: Set(pricing_tiers_json),
                enabled: Set(input.enabled),
                created_at: Set(now),
                updated_at: Set(now),
            }
            .insert(conn)
            .await?
        }
    };

    price_rule_rate::Entity::delete_many()
        .filter(price_rule_rate::Column::PriceRuleId.eq(model.id))
        .exec(conn)
        .await?;
    for rate in &rates {
        price_rule_rate::ActiveModel {
            id: NotSet,
            price_rule_id: Set(model.id),
            metric: Set(rate.metric.clone()),
            unit: Set(rate.unit.clone()),
            unit_size: Set(i64::try_from(rate.unit_size).unwrap_or(i64::MAX)),
            price_usd: Set(rate.price_usd.to_string()),
            conditions_json: Set(rate
                .conditions_json
                .as_ref()
                .map(serde_json::to_string)
                .transpose()?),
            sort_order: Set(rate.sort_order),
        }
        .insert(conn)
        .await?;
    }
    to_record(model, rates)
}

pub async fn delete(conn: &DatabaseConnection, id: i64) -> anyhow::Result<bool> {
    price_rule_rate::Entity::delete_many()
        .filter(price_rule_rate::Column::PriceRuleId.eq(id))
        .exec(conn)
        .await?;
    let res = price_rule::Entity::delete_by_id(id).exec(conn).await?;
    Ok(res.rows_affected > 0)
}
