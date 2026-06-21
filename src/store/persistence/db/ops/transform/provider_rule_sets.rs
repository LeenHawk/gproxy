//! Provider ↔ rule-set attachment ops for the `db` backend.

use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::store::persistence::records::{ProviderRuleSet, ProviderRuleSetInput};

use crate::store::persistence::db::entities::transform::provider_rule_set;

fn to_record(m: provider_rule_set::Model) -> ProviderRuleSet {
    ProviderRuleSet {
        id: m.id,
        provider_id: m.provider_id,
        rule_set_id: m.rule_set_id,
        sort_order: m.sort_order,
        enabled: m.enabled,
        created_at: m.created_at,
        updated_at: m.updated_at,
    }
}

pub async fn list(
    conn: &DatabaseConnection,
    provider_id: i64,
) -> anyhow::Result<Vec<ProviderRuleSet>> {
    Ok(provider_rule_set::Entity::find()
        .filter(provider_rule_set::Column::ProviderId.eq(provider_id))
        .all(conn)
        .await?
        .into_iter()
        .map(to_record)
        .collect())
}

pub async fn upsert(
    conn: &DatabaseConnection,
    input: ProviderRuleSetInput,
) -> anyhow::Result<ProviderRuleSet> {
    let now = crate::store::persistence::db::ops::now_secs();

    let model = match input.id {
        Some(id) => match provider_rule_set::Entity::find_by_id(id).one(conn).await? {
            Some(existing) => {
                let mut am: provider_rule_set::ActiveModel = existing.into();
                am.provider_id = Set(input.provider_id);
                am.rule_set_id = Set(input.rule_set_id);
                am.sort_order = Set(input.sort_order);
                am.enabled = Set(input.enabled);
                am.updated_at = Set(now);
                am.update(conn).await?
            }
            None => {
                // Seeding an empty store from a pinned bundle: insert WITH the
                // explicit id (matches the file backend's insert-with-id).
                provider_rule_set::ActiveModel {
                    id: Set(id),
                    provider_id: Set(input.provider_id),
                    rule_set_id: Set(input.rule_set_id),
                    sort_order: Set(input.sort_order),
                    enabled: Set(input.enabled),
                    created_at: Set(now),
                    updated_at: Set(now),
                }
                .insert(conn)
                .await?
            }
        },
        None => {
            provider_rule_set::ActiveModel {
                id: NotSet,
                provider_id: Set(input.provider_id),
                rule_set_id: Set(input.rule_set_id),
                sort_order: Set(input.sort_order),
                enabled: Set(input.enabled),
                created_at: Set(now),
                updated_at: Set(now),
            }
            .insert(conn)
            .await?
        }
    };

    Ok(to_record(model))
}

pub async fn delete(conn: &DatabaseConnection, id: i64) -> anyhow::Result<bool> {
    let res = provider_rule_set::Entity::delete_by_id(id)
        .exec(conn)
        .await?;
    Ok(res.rows_affected > 0)
}

pub async fn delete_by_provider(conn: &DatabaseConnection, provider_id: i64) -> anyhow::Result<()> {
    provider_rule_set::Entity::delete_many()
        .filter(provider_rule_set::Column::ProviderId.eq(provider_id))
        .exec(conn)
        .await?;
    Ok(())
}

pub async fn delete_by_rule_set(conn: &DatabaseConnection, rule_set_id: i64) -> anyhow::Result<()> {
    provider_rule_set::Entity::delete_many()
        .filter(provider_rule_set::Column::RuleSetId.eq(rule_set_id))
        .exec(conn)
        .await?;
    Ok(())
}
