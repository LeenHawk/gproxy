//! Route-member ops for the `db` backend.

use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::store::persistence::records::{RouteMember, RouteMemberInput};

use crate::store::persistence::db::entities::routing::route_member;

fn to_record(m: route_member::Model) -> RouteMember {
    RouteMember {
        id: m.id,
        route_id: m.route_id,
        provider_id: m.provider_id,
        upstream_model_id: m.upstream_model_id,
        weight: m.weight,
        tier: m.tier,
        enabled: m.enabled,
        created_at: m.created_at,
        updated_at: m.updated_at,
    }
}

pub async fn list(conn: &DatabaseConnection, route_id: i64) -> anyhow::Result<Vec<RouteMember>> {
    Ok(route_member::Entity::find()
        .filter(route_member::Column::RouteId.eq(route_id))
        .all(conn)
        .await?
        .into_iter()
        .map(to_record)
        .collect())
}

pub async fn upsert(
    conn: &DatabaseConnection,
    input: RouteMemberInput,
) -> anyhow::Result<RouteMember> {
    let now = crate::store::persistence::db::ops::now_secs();

    let model = match input.id {
        Some(id) => match route_member::Entity::find_by_id(id).one(conn).await? {
            Some(existing) => {
                let mut am: route_member::ActiveModel = existing.into();
                am.route_id = Set(input.route_id);
                am.provider_id = Set(input.provider_id);
                am.upstream_model_id = Set(input.upstream_model_id);
                am.weight = Set(input.weight);
                am.tier = Set(input.tier);
                am.enabled = Set(input.enabled);
                am.updated_at = Set(now);
                am.update(conn).await?
            }
            None => {
                // Seeding an empty store from a pinned bundle: insert WITH the
                // explicit id (matches the file backend's insert-with-id).
                route_member::ActiveModel {
                    id: Set(id),
                    route_id: Set(input.route_id),
                    provider_id: Set(input.provider_id),
                    upstream_model_id: Set(input.upstream_model_id),
                    weight: Set(input.weight),
                    tier: Set(input.tier),
                    enabled: Set(input.enabled),
                    created_at: Set(now),
                    updated_at: Set(now),
                }
                .insert(conn)
                .await?
            }
        },
        None => {
            route_member::ActiveModel {
                id: NotSet,
                route_id: Set(input.route_id),
                provider_id: Set(input.provider_id),
                upstream_model_id: Set(input.upstream_model_id),
                weight: Set(input.weight),
                tier: Set(input.tier),
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
    let res = route_member::Entity::delete_by_id(id).exec(conn).await?;
    Ok(res.rows_affected > 0)
}

pub async fn delete_by_route(conn: &DatabaseConnection, route_id: i64) -> anyhow::Result<()> {
    route_member::Entity::delete_many()
        .filter(route_member::Column::RouteId.eq(route_id))
        .exec(conn)
        .await?;
    Ok(())
}
