//! Route-permission ops for the `db` backend.

use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::store::persistence::records::{RoutePermission, RoutePermissionInput, Scope};

use crate::store::persistence::db::entities::authz::route_permission;

fn to_record(m: route_permission::Model) -> anyhow::Result<RoutePermission> {
    Ok(RoutePermission {
        id: m.id,
        scope: Scope::parse(&m.scope)?,
        scope_id: m.scope_id,
        route_pattern: m.route_pattern,
        created_at: m.created_at,
        updated_at: m.updated_at,
    })
}

pub async fn list(
    conn: &DatabaseConnection,
    scope: Scope,
    scope_id: i64,
) -> anyhow::Result<Vec<RoutePermission>> {
    route_permission::Entity::find()
        .filter(route_permission::Column::Scope.eq(scope.as_str()))
        .filter(route_permission::Column::ScopeId.eq(scope_id))
        .all(conn)
        .await?
        .into_iter()
        .map(to_record)
        .collect()
}

pub async fn upsert(
    conn: &DatabaseConnection,
    input: RoutePermissionInput,
) -> anyhow::Result<RoutePermission> {
    let now = crate::store::persistence::db::ops::now_secs();

    let model = match input.id {
        Some(id) => match route_permission::Entity::find_by_id(id).one(conn).await? {
            Some(existing) => {
                let mut am: route_permission::ActiveModel = existing.into();
                am.scope = Set(input.scope.as_str().to_owned());
                am.scope_id = Set(input.scope_id);
                am.route_pattern = Set(input.route_pattern);
                am.updated_at = Set(now);
                am.update(conn).await?
            }
            None => {
                // Seeding an empty store from a pinned bundle: insert WITH the
                // explicit id (matches the file backend's insert-with-id).
                route_permission::ActiveModel {
                    id: Set(id),
                    scope: Set(input.scope.as_str().to_owned()),
                    scope_id: Set(input.scope_id),
                    route_pattern: Set(input.route_pattern),
                    created_at: Set(now),
                    updated_at: Set(now),
                }
                .insert(conn)
                .await?
            }
        },
        None => {
            route_permission::ActiveModel {
                id: NotSet,
                scope: Set(input.scope.as_str().to_owned()),
                scope_id: Set(input.scope_id),
                route_pattern: Set(input.route_pattern),
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
    let res = route_permission::Entity::delete_by_id(id)
        .exec(conn)
        .await?;
    Ok(res.rows_affected > 0)
}

pub async fn delete_by_scope(
    conn: &DatabaseConnection,
    scope: Scope,
    scope_id: i64,
) -> anyhow::Result<()> {
    route_permission::Entity::delete_many()
        .filter(route_permission::Column::Scope.eq(scope.as_str()))
        .filter(route_permission::Column::ScopeId.eq(scope_id))
        .exec(conn)
        .await?;
    Ok(())
}
