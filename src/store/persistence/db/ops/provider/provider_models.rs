//! Provider-model ops for the `db` backend.

use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::store::persistence::records::{ProviderModel, ProviderModelInput};

use crate::store::persistence::db::entities::provider::provider_model;

fn to_record(m: provider_model::Model) -> anyhow::Result<ProviderModel> {
    Ok(ProviderModel {
        id: m.id,
        provider_id: m.provider_id,
        model_id: m.model_id,
        display_name: m.display_name,
        pricing_json: m
            .pricing_json
            .map(|s| serde_json::from_str(&s))
            .transpose()?,
        variants_json: m
            .variants_json
            .map(|s| serde_json::from_str(&s))
            .transpose()?,
        enabled: m.enabled,
        created_at: m.created_at,
        updated_at: m.updated_at,
    })
}

pub async fn list(
    conn: &DatabaseConnection,
    provider_id: i64,
) -> anyhow::Result<Vec<ProviderModel>> {
    provider_model::Entity::find()
        .filter(provider_model::Column::ProviderId.eq(provider_id))
        .all(conn)
        .await?
        .into_iter()
        .map(to_record)
        .collect()
}

pub async fn upsert(
    conn: &DatabaseConnection,
    input: ProviderModelInput,
) -> anyhow::Result<ProviderModel> {
    let now = crate::store::persistence::db::ops::now_secs();
    let pricing = input
        .pricing_json
        .map(|v| serde_json::to_string(&v))
        .transpose()?;
    let variants = input
        .variants_json
        .map(|v| serde_json::to_string(&v))
        .transpose()?;

    let model = match input.id {
        Some(id) => match provider_model::Entity::find_by_id(id).one(conn).await? {
            Some(existing) => {
                let mut am: provider_model::ActiveModel = existing.into();
                am.provider_id = Set(input.provider_id);
                am.model_id = Set(input.model_id);
                am.display_name = Set(input.display_name);
                am.pricing_json = Set(pricing);
                am.variants_json = Set(variants);
                am.enabled = Set(input.enabled);
                am.updated_at = Set(now);
                am.update(conn).await?
            }
            None => {
                // Seeding an empty store from a pinned bundle: insert WITH the
                // explicit id (matches the file backend's insert-with-id).
                provider_model::ActiveModel {
                    id: Set(id),
                    provider_id: Set(input.provider_id),
                    model_id: Set(input.model_id),
                    display_name: Set(input.display_name),
                    pricing_json: Set(pricing),
                    variants_json: Set(variants),
                    enabled: Set(input.enabled),
                    created_at: Set(now),
                    updated_at: Set(now),
                }
                .insert(conn)
                .await?
            }
        },
        None => {
            provider_model::ActiveModel {
                id: NotSet,
                provider_id: Set(input.provider_id),
                model_id: Set(input.model_id),
                display_name: Set(input.display_name),
                pricing_json: Set(pricing),
                variants_json: Set(variants),
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
    let res = provider_model::Entity::delete_by_id(id).exec(conn).await?;
    Ok(res.rows_affected > 0)
}

pub async fn delete_by_provider(conn: &DatabaseConnection, provider_id: i64) -> anyhow::Result<()> {
    provider_model::Entity::delete_many()
        .filter(provider_model::Column::ProviderId.eq(provider_id))
        .exec(conn)
        .await?;
    Ok(())
}
