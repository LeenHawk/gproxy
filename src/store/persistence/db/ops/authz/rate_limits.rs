//! Rate-limit ops for the `db` backend.

use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::store::persistence::records::{RateLimit, RateLimitInput, Scope};

use crate::store::persistence::db::entities::authz::rate_limit;

fn to_record(m: rate_limit::Model) -> anyhow::Result<RateLimit> {
    Ok(RateLimit {
        id: m.id,
        scope: Scope::parse(&m.scope)?,
        scope_id: m.scope_id,
        route_pattern: m.route_pattern,
        rpm: m.rpm,
        rpd: m.rpd,
        total_tokens: m.total_tokens,
        created_at: m.created_at,
        updated_at: m.updated_at,
    })
}

pub async fn list(
    conn: &DatabaseConnection,
    scope: Scope,
    scope_id: i64,
) -> anyhow::Result<Vec<RateLimit>> {
    rate_limit::Entity::find()
        .filter(rate_limit::Column::Scope.eq(scope.as_str()))
        .filter(rate_limit::Column::ScopeId.eq(scope_id))
        .all(conn)
        .await?
        .into_iter()
        .map(to_record)
        .collect()
}

pub async fn upsert(conn: &DatabaseConnection, input: RateLimitInput) -> anyhow::Result<RateLimit> {
    let now = crate::store::persistence::db::ops::now_secs();

    let model = match input.id {
        Some(id) => match rate_limit::Entity::find_by_id(id).one(conn).await? {
            Some(existing) => {
                let mut am: rate_limit::ActiveModel = existing.into();
                am.scope = Set(input.scope.as_str().to_owned());
                am.scope_id = Set(input.scope_id);
                am.route_pattern = Set(input.route_pattern);
                am.rpm = Set(input.rpm);
                am.rpd = Set(input.rpd);
                am.total_tokens = Set(input.total_tokens);
                am.updated_at = Set(now);
                am.update(conn).await?
            }
            None => {
                // Seeding an empty store from a pinned bundle: insert WITH the
                // explicit id (matches the file backend's insert-with-id).
                rate_limit::ActiveModel {
                    id: Set(id),
                    scope: Set(input.scope.as_str().to_owned()),
                    scope_id: Set(input.scope_id),
                    route_pattern: Set(input.route_pattern),
                    rpm: Set(input.rpm),
                    rpd: Set(input.rpd),
                    total_tokens: Set(input.total_tokens),
                    created_at: Set(now),
                    updated_at: Set(now),
                }
                .insert(conn)
                .await?
            }
        },
        None => {
            rate_limit::ActiveModel {
                id: NotSet,
                scope: Set(input.scope.as_str().to_owned()),
                scope_id: Set(input.scope_id),
                route_pattern: Set(input.route_pattern),
                rpm: Set(input.rpm),
                rpd: Set(input.rpd),
                total_tokens: Set(input.total_tokens),
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
    let res = rate_limit::Entity::delete_by_id(id).exec(conn).await?;
    Ok(res.rows_affected > 0)
}

pub async fn delete_by_scope(
    conn: &DatabaseConnection,
    scope: Scope,
    scope_id: i64,
) -> anyhow::Result<()> {
    rate_limit::Entity::delete_many()
        .filter(rate_limit::Column::Scope.eq(scope.as_str()))
        .filter(rate_limit::Column::ScopeId.eq(scope_id))
        .exec(conn)
        .await?;
    Ok(())
}
