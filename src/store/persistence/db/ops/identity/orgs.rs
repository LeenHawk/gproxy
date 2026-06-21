//! Org ops for the `db` backend. `name` is unique.

use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::store::persistence::records::{Org, OrgInput, Scope};

use crate::store::persistence::db::entities::identity::org;

fn to_record(m: org::Model) -> Org {
    Org {
        id: m.id,
        name: m.name,
        enabled: m.enabled,
        description: m.description,
        created_at: m.created_at,
        updated_at: m.updated_at,
    }
}

pub async fn list(conn: &DatabaseConnection) -> anyhow::Result<Vec<Org>> {
    Ok(org::Entity::find()
        .all(conn)
        .await?
        .into_iter()
        .map(to_record)
        .collect())
}

pub async fn get(conn: &DatabaseConnection, id: i64) -> anyhow::Result<Option<Org>> {
    Ok(org::Entity::find_by_id(id).one(conn).await?.map(to_record))
}

pub async fn get_by_name(conn: &DatabaseConnection, name: &str) -> anyhow::Result<Option<Org>> {
    Ok(org::Entity::find()
        .filter(org::Column::Name.eq(name))
        .one(conn)
        .await?
        .map(to_record))
}

pub async fn upsert(conn: &DatabaseConnection, input: OrgInput) -> anyhow::Result<Org> {
    let now = crate::store::persistence::db::ops::now_secs();
    let name = input.name.clone();
    let conflict = |e| {
        crate::store::persistence::db::ops::conflict_if_unique(e, || {
            format!("org name already exists: {name}")
        })
    };

    let model = match input.id {
        Some(id) => match org::Entity::find_by_id(id).one(conn).await? {
            Some(existing) => {
                let mut am: org::ActiveModel = existing.into();
                am.name = Set(input.name);
                am.enabled = Set(input.enabled);
                am.description = Set(input.description);
                am.updated_at = Set(now);
                am.update(conn).await.map_err(conflict)?
            }
            None => {
                // Seeding an empty store from a pinned bundle: insert WITH the
                // explicit id (matches the file backend's insert-with-id).
                org::ActiveModel {
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
        None => org::ActiveModel {
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
    // cascade: teams, users (which cascade user_keys), and scope-bound rows.
    super::teams::delete_by_org(conn, id).await?;
    super::users::delete_by_org(conn, id).await?;
    crate::store::persistence::db::ops::authz::route_permissions::delete_by_scope(
        conn,
        Scope::Org,
        id,
    )
    .await?;
    crate::store::persistence::db::ops::authz::rate_limits::delete_by_scope(conn, Scope::Org, id)
        .await?;
    crate::store::persistence::db::ops::authz::quotas::delete_by_scope(conn, Scope::Org, id)
        .await?;

    let res = org::Entity::delete_by_id(id).exec(conn).await?;
    Ok(res.rows_affected > 0)
}
