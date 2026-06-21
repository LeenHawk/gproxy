//! Alias ops for the `db` backend.

use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::store::persistence::records::{Alias, AliasInput};

use crate::store::persistence::db::entities::routing::alias;

fn to_record(m: alias::Model) -> Alias {
    Alias {
        id: m.id,
        alias: m.alias,
        route_id: m.route_id,
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
        .one(conn)
        .await?
        .map(to_record))
}

pub async fn upsert(conn: &DatabaseConnection, input: AliasInput) -> anyhow::Result<Alias> {
    let now = crate::store::persistence::db::ops::now_secs();
    let alias_name = input.alias.clone();
    let conflict = |e| {
        crate::store::persistence::db::ops::conflict_if_unique(e, || {
            format!("alias already exists: {alias_name}")
        })
    };

    let model = match input.id {
        Some(id) => match alias::Entity::find_by_id(id).one(conn).await? {
            Some(existing) => {
                let mut am: alias::ActiveModel = existing.into();
                am.alias = Set(input.alias);
                am.route_id = Set(input.route_id);
                am.updated_at = Set(now);
                am.update(conn).await.map_err(conflict)?
            }
            None => {
                // Seeding an empty store from a pinned bundle: insert WITH the
                // explicit id (matches the file backend's insert-with-id).
                alias::ActiveModel {
                    id: Set(id),
                    alias: Set(input.alias),
                    route_id: Set(input.route_id),
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
            alias: Set(input.alias),
            route_id: Set(input.route_id),
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

pub async fn delete_by_route(conn: &DatabaseConnection, route_id: i64) -> anyhow::Result<()> {
    alias::Entity::delete_many()
        .filter(alias::Column::RouteId.eq(route_id))
        .exec(conn)
        .await?;
    Ok(())
}
