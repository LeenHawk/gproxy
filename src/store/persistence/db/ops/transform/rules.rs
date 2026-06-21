//! Rule ops for the `db` backend. `config_json` / `filter_operation_keys`
//! round-trip through serialized JSON text.

use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::store::persistence::records::{Rule, RuleInput};

use crate::store::persistence::db::entities::transform::rule;

fn to_record(m: rule::Model) -> anyhow::Result<Rule> {
    Ok(Rule {
        id: m.id,
        rule_set_id: m.rule_set_id,
        kind: m.kind,
        config_json: serde_json::from_str(&m.config_json)?,
        filter_model_pattern: m.filter_model_pattern,
        filter_operation_keys: m
            .filter_operation_keys
            .map(|s| serde_json::from_str(&s))
            .transpose()?,
        sort_order: m.sort_order,
        enabled: m.enabled,
        created_at: m.created_at,
        updated_at: m.updated_at,
    })
}

pub async fn list(conn: &DatabaseConnection, rule_set_id: i64) -> anyhow::Result<Vec<Rule>> {
    rule::Entity::find()
        .filter(rule::Column::RuleSetId.eq(rule_set_id))
        .all(conn)
        .await?
        .into_iter()
        .map(to_record)
        .collect()
}

pub async fn get(conn: &DatabaseConnection, id: i64) -> anyhow::Result<Option<Rule>> {
    rule::Entity::find_by_id(id)
        .one(conn)
        .await?
        .map(to_record)
        .transpose()
}

pub async fn upsert(conn: &DatabaseConnection, input: RuleInput) -> anyhow::Result<Rule> {
    let now = crate::store::persistence::db::ops::now_secs();
    let config = serde_json::to_string(&input.config_json)?;
    let filter_keys = input
        .filter_operation_keys
        .map(|v| serde_json::to_string(&v))
        .transpose()?;

    let model = match input.id {
        Some(id) => match rule::Entity::find_by_id(id).one(conn).await? {
            Some(existing) => {
                let mut am: rule::ActiveModel = existing.into();
                am.rule_set_id = Set(input.rule_set_id);
                am.kind = Set(input.kind);
                am.config_json = Set(config);
                am.filter_model_pattern = Set(input.filter_model_pattern);
                am.filter_operation_keys = Set(filter_keys);
                am.sort_order = Set(input.sort_order);
                am.enabled = Set(input.enabled);
                am.updated_at = Set(now);
                am.update(conn).await?
            }
            None => {
                // Seeding an empty store from a pinned bundle: insert WITH the
                // explicit id (matches the file backend's insert-with-id).
                rule::ActiveModel {
                    id: Set(id),
                    rule_set_id: Set(input.rule_set_id),
                    kind: Set(input.kind),
                    config_json: Set(config),
                    filter_model_pattern: Set(input.filter_model_pattern),
                    filter_operation_keys: Set(filter_keys),
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
            rule::ActiveModel {
                id: NotSet,
                rule_set_id: Set(input.rule_set_id),
                kind: Set(input.kind),
                config_json: Set(config),
                filter_model_pattern: Set(input.filter_model_pattern),
                filter_operation_keys: Set(filter_keys),
                sort_order: Set(input.sort_order),
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
    let res = rule::Entity::delete_by_id(id).exec(conn).await?;
    Ok(res.rows_affected > 0)
}

pub async fn delete_by_rule_set(conn: &DatabaseConnection, rule_set_id: i64) -> anyhow::Result<()> {
    rule::Entity::delete_many()
        .filter(rule::Column::RuleSetId.eq(rule_set_id))
        .exec(conn)
        .await?;
    Ok(())
}
