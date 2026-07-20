//! Alias ops for the `db` backend.

use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::store::persistence::records::{Alias, AliasInput};

use crate::store::persistence::db::entities::routing::alias;

fn to_record(m: alias::Model) -> Alias {
    Alias {
        id: m.id,
        provider: m.provider,
        alias: m.alias,
        target: m.target.unwrap_or_default(),
        sort_order: m.sort_order,
        enabled: m.enabled,
        created_at: m.created_at,
        updated_at: m.updated_at,
    }
}

pub async fn list(conn: &DatabaseConnection) -> anyhow::Result<Vec<Alias>> {
    Ok(alias::Entity::find()
        .all(conn)
        .await?
        .into_iter()
        .map(to_record)
        .collect())
}

pub async fn get_by_name(conn: &DatabaseConnection, value: &str) -> anyhow::Result<Option<Alias>> {
    Ok(alias::Entity::find()
        .filter(alias::Column::Alias.eq(value))
        .filter(alias::Column::Provider.eq("*"))
        .one(conn)
        .await?
        .map(to_record))
}

pub async fn upsert(conn: &DatabaseConnection, input: AliasInput) -> anyhow::Result<Alias> {
    let now = crate::store::persistence::db::ops::now_secs();
    let alias_name = input.alias.clone();
    let provider = input.provider;
    let target = input
        .target
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("alias target is required"))?;
    let conflict_msg = format!("alias already exists: {provider}/{alias_name}");
    let conflict =
        |e| crate::store::persistence::db::ops::conflict_if_unique(e, || conflict_msg.clone());

    let model = match input.id {
        Some(id) => match alias::Entity::find_by_id(id).one(conn).await? {
            Some(existing) => {
                let mut am: alias::ActiveModel = existing.into();
                am.provider = Set(provider);
                am.alias = Set(input.alias);
                am.target = Set(Some(target));
                am.sort_order = Set(input.sort_order);
                am.enabled = Set(input.enabled);
                am.updated_at = Set(now);
                am.update(conn).await.map_err(conflict)?
            }
            None => {
                // Seeding an empty store from a pinned bundle: insert WITH the
                // Preserve explicit IDs supplied by bundle imports.
                alias::ActiveModel {
                    id: Set(id),
                    provider: Set(provider),
                    alias: Set(input.alias),
                    target: Set(Some(target)),
                    sort_order: Set(input.sort_order),
                    enabled: Set(input.enabled),
                    created_at: Set(now),
                    updated_at: Set(now),
                }
                .insert(conn)
                .await
                .map_err(conflict)?
            }
        },
        None => alias::ActiveModel {
            id: NotSet,
            provider: Set(provider),
            alias: Set(input.alias),
            target: Set(Some(target)),
            sort_order: Set(input.sort_order),
            enabled: Set(input.enabled),
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
    let res = alias::Entity::delete_by_id(id).exec(conn).await?;
    Ok(res.rows_affected > 0)
}
