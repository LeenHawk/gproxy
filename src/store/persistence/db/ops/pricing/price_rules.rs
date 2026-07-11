//! Pricing rule ops for the `db` backend.

use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{ActiveModelTrait, DatabaseConnection, EntityTrait};

use crate::store::persistence::db::entities::pricing::price_rule;
use crate::store::persistence::records::{PriceRule, PriceRuleInput};

fn to_record(m: price_rule::Model) -> anyhow::Result<PriceRule> {
    Ok(PriceRule {
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
        image_price: m.image_price.parse()?,
        enabled: m.enabled,
        created_at: m.created_at,
        updated_at: m.updated_at,
    })
}

pub async fn list(conn: &DatabaseConnection) -> anyhow::Result<Vec<PriceRule>> {
    price_rule::Entity::find()
        .all(conn)
        .await?
        .into_iter()
        .map(to_record)
        .collect()
}

pub async fn upsert(conn: &DatabaseConnection, input: PriceRuleInput) -> anyhow::Result<PriceRule> {
    let now = crate::store::persistence::db::ops::now_secs();
    let input_price = input.input_price.to_string();
    let output_price = input.output_price.to_string();
    let cache_read_price = input.cache_read_price.to_string();
    let cache_creation_5m_price = input.cache_creation_5m_price.to_string();
    let cache_creation_30m_price = input.cache_creation_30m_price.to_string();
    let cache_creation_1h_price = input.cache_creation_1h_price.to_string();
    let image_price = input.image_price.to_string();

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
                am.image_price = Set(image_price);
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
                    image_price: Set(image_price),
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
                image_price: Set(image_price),
                enabled: Set(input.enabled),
                created_at: Set(now),
                updated_at: Set(now),
            }
            .insert(conn)
            .await?
        }
    };

    to_record(model)
}

pub async fn delete(conn: &DatabaseConnection, id: i64) -> anyhow::Result<bool> {
    let res = price_rule::Entity::delete_by_id(id).exec(conn).await?;
    Ok(res.rows_affected > 0)
}
