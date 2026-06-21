//! Rule-set ops for the `db` backend. Unique `name`.

use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::store::persistence::records::{RuleSet, RuleSetInput};

use crate::store::persistence::db::entities::transform::rule_set;

fn to_record(m: rule_set::Model) -> RuleSet {
    RuleSet {
        id: m.id,
        name: m.name,
        enabled: m.enabled,
        description: m.description,
        created_at: m.created_at,
        updated_at: m.updated_at,
    }
}

pub async fn list(conn: &DatabaseConnection) -> anyhow::Result<Vec<RuleSet>> {
    Ok(rule_set::Entity::find()
        .all(conn)
        .await?
        .into_iter()
        .map(to_record)
        .collect())
}

pub async fn get(conn: &DatabaseConnection, id: i64) -> anyhow::Result<Option<RuleSet>> {
    Ok(rule_set::Entity::find_by_id(id)
        .one(conn)
        .await?
        .map(to_record))
}

pub async fn get_by_name(conn: &DatabaseConnection, name: &str) -> anyhow::Result<Option<RuleSet>> {
    Ok(rule_set::Entity::find()
        .filter(rule_set::Column::Name.eq(name))
        .one(conn)
        .await?
        .map(to_record))
}

pub async fn upsert(conn: &DatabaseConnection, input: RuleSetInput) -> anyhow::Result<RuleSet> {
    let now = crate::store::persistence::db::ops::now_secs();
    let name = input.name.clone();
    let conflict = |e| {
        crate::store::persistence::db::ops::conflict_if_unique(e, || {
            format!("rule set name already exists: {name}")
        })
    };

    // Enforce uniqueness on `name`.
    if let Some(existing) = rule_set::Entity::find()
        .filter(rule_set::Column::Name.eq(input.name.clone()))
        .one(conn)
        .await?
        && Some(existing.id) != input.id
    {
        return Err(crate::store::persistence::ConflictError::new(format!(
            "rule set name already exists: {}",
            input.name
        ))
        .into());
    }

    let model = match input.id {
        Some(id) => match rule_set::Entity::find_by_id(id).one(conn).await? {
            Some(existing) => {
                let mut am: rule_set::ActiveModel = existing.into();
                am.name = Set(input.name);
                am.enabled = Set(input.enabled);
                am.description = Set(input.description);
                am.updated_at = Set(now);
                am.update(conn).await.map_err(conflict)?
            }
            None => {
                // Seeding an empty store from a pinned bundle: insert WITH the
                // explicit id (the unique `name` precheck above already ensured
                // no conflicting row exists).
                rule_set::ActiveModel {
                    id: Set(id),
                    name: Set(input.name),
                    enabled: Set(input.enabled),
                    description: Set(input.description),
                    created_at: Set(now),
                    updated_at: Set(now),
                }
                .insert(conn)
                .await
                .map_err(conflict)?
            }
        },
        None => rule_set::ActiveModel {
            id: NotSet,
            name: Set(input.name),
            enabled: Set(input.enabled),
            description: Set(input.description),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(conn)
        .await
        .map_err(conflict)?,
    };

    Ok(to_record(model))
}

pub async fn delete(conn: &DatabaseConnection, id: i64) -> anyhow::Result<bool> {
    // cascade: this set's rules and its provider attachments (not the providers).
    super::rules::delete_by_rule_set(conn, id).await?;
    super::provider_rule_sets::delete_by_rule_set(conn, id).await?;

    let res = rule_set::Entity::delete_by_id(id).exec(conn).await?;
    Ok(res.rows_affected > 0)
}
