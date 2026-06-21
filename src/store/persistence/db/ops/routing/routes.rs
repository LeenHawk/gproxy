//! Route ops for the `db` backend.

use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::store::persistence::records::{Route, RouteInput};

use crate::store::persistence::db::entities::routing::route;

fn to_record(m: route::Model) -> Route {
    Route {
        id: m.id,
        name: m.name,
        strategy: m.strategy,
        enabled: m.enabled,
        description: m.description,
        settings_json: m
            .settings_json
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
            .unwrap_or(None),
        created_at: m.created_at,
        updated_at: m.updated_at,
    }
}

pub async fn list(conn: &DatabaseConnection) -> anyhow::Result<Vec<Route>> {
    Ok(route::Entity::find()
        .all(conn)
        .await?
        .into_iter()
        .map(to_record)
        .collect())
}

pub async fn get(conn: &DatabaseConnection, id: i64) -> anyhow::Result<Option<Route>> {
    Ok(route::Entity::find_by_id(id)
        .one(conn)
        .await?
        .map(to_record))
}

pub async fn get_by_name(conn: &DatabaseConnection, name: &str) -> anyhow::Result<Option<Route>> {
    Ok(route::Entity::find()
        .filter(route::Column::Name.eq(name))
        .one(conn)
        .await?
        .map(to_record))
}

pub async fn upsert(conn: &DatabaseConnection, input: RouteInput) -> anyhow::Result<Route> {
    let now = crate::store::persistence::db::ops::now_secs();
    let settings = input
        .settings_json
        .as_ref()
        .map(serde_json::to_string)
        .transpose()?;
    let name = input.name.clone();
    let conflict = |e| {
        crate::store::persistence::db::ops::conflict_if_unique(e, || {
            format!("route name already exists: {name}")
        })
    };

    let model = match input.id {
        Some(id) => match route::Entity::find_by_id(id).one(conn).await? {
            Some(existing) => {
                let mut am: route::ActiveModel = existing.into();
                am.name = Set(input.name);
                am.strategy = Set(input.strategy);
                am.enabled = Set(input.enabled);
                am.description = Set(input.description);
                am.settings_json = Set(settings);
                am.updated_at = Set(now);
                am.update(conn).await.map_err(conflict)?
            }
            None => {
                // Seeding an empty store from a pinned bundle: insert WITH the
                // explicit id (matches the file backend's insert-with-id).
                route::ActiveModel {
                    id: Set(id),
                    name: Set(input.name),
                    strategy: Set(input.strategy),
                    enabled: Set(input.enabled),
                    description: Set(input.description),
                    settings_json: Set(settings),
                    created_at: Set(now),
                    updated_at: Set(now),
                }
                .insert(conn)
                .await
                .map_err(conflict)?
            }
        },
        None => route::ActiveModel {
            id: NotSet,
            name: Set(input.name),
            strategy: Set(input.strategy),
            enabled: Set(input.enabled),
            description: Set(input.description),
            settings_json: Set(settings),
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
    // cascade: members and aliases of this route.
    super::route_members::delete_by_route(conn, id).await?;
    super::aliases::delete_by_route(conn, id).await?;

    let res = route::Entity::delete_by_id(id).exec(conn).await?;
    Ok(res.rows_affected > 0)
}
