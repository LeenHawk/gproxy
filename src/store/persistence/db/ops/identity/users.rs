//! User ops for the `db` backend. `name` is unique.

use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::store::persistence::records::{Scope, User, UserInput};

use crate::store::persistence::db::entities::identity::user;

fn to_record(m: user::Model) -> User {
    User {
        id: m.id,
        name: m.name,
        org_id: m.org_id,
        team_id: m.team_id,
        password: m.password,
        enabled: m.enabled,
        is_admin: m.is_admin,
        created_at: m.created_at,
        updated_at: m.updated_at,
    }
}

pub async fn list(conn: &DatabaseConnection) -> anyhow::Result<Vec<User>> {
    Ok(user::Entity::find()
        .all(conn)
        .await?
        .into_iter()
        .map(to_record)
        .collect())
}

pub async fn get(conn: &DatabaseConnection, id: i64) -> anyhow::Result<Option<User>> {
    Ok(user::Entity::find_by_id(id).one(conn).await?.map(to_record))
}

pub async fn get_by_name(conn: &DatabaseConnection, name: &str) -> anyhow::Result<Option<User>> {
    Ok(user::Entity::find()
        .filter(user::Column::Name.eq(name))
        .one(conn)
        .await?
        .map(to_record))
}

pub async fn upsert(conn: &DatabaseConnection, input: UserInput) -> anyhow::Result<User> {
    let now = crate::store::persistence::db::ops::now_secs();
    let name = input.name.clone();
    let conflict = |e| {
        crate::store::persistence::db::ops::conflict_if_unique(e, || {
            format!("user name already exists: {name}")
        })
    };

    let model = match input.id {
        Some(id) => match user::Entity::find_by_id(id).one(conn).await? {
            Some(existing) => {
                let mut am: user::ActiveModel = existing.into();
                am.name = Set(input.name);
                am.org_id = Set(input.org_id);
                am.team_id = Set(input.team_id);
                am.password = Set(input.password);
                am.enabled = Set(input.enabled);
                am.is_admin = Set(input.is_admin);
                am.updated_at = Set(now);
                am.update(conn).await.map_err(conflict)?
            }
            None => {
                // Seeding an empty store from a pinned bundle: insert WITH the
                // explicit id (matches the file backend's insert-with-id).
                user::ActiveModel {
                    id: Set(id),
                    name: Set(input.name),
                    org_id: Set(input.org_id),
                    team_id: Set(input.team_id),
                    password: Set(input.password),
                    enabled: Set(input.enabled),
                    is_admin: Set(input.is_admin),
                    created_at: Set(now),
                    updated_at: Set(now),
                }
                .insert(conn)
                .await
                .map_err(conflict)?
            }
        },
        None => user::ActiveModel {
            id: NotSet,
            name: Set(input.name),
            org_id: Set(input.org_id),
            team_id: Set(input.team_id),
            password: Set(input.password),
            enabled: Set(input.enabled),
            is_admin: Set(input.is_admin),
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
    // cascade: keys and scope-bound permissions / rate limits / quotas.
    super::user_keys::delete_by_user(conn, id).await?;
    crate::store::persistence::db::ops::authz::route_permissions::delete_by_scope(
        conn,
        Scope::User,
        id,
    )
    .await?;
    crate::store::persistence::db::ops::authz::rate_limits::delete_by_scope(conn, Scope::User, id)
        .await?;
    crate::store::persistence::db::ops::authz::quotas::delete_by_scope(conn, Scope::User, id)
        .await?;

    let res = user::Entity::delete_by_id(id).exec(conn).await?;
    Ok(res.rows_affected > 0)
}

pub async fn delete_by_org(conn: &DatabaseConnection, org_id: i64) -> anyhow::Result<()> {
    // cascade each user's keys + scope rows before bulk-removing the users.
    let users = user::Entity::find()
        .filter(user::Column::OrgId.eq(org_id))
        .all(conn)
        .await?;
    for u in users {
        super::user_keys::delete_by_user(conn, u.id).await?;
        crate::store::persistence::db::ops::authz::route_permissions::delete_by_scope(
            conn,
            Scope::User,
            u.id,
        )
        .await?;
        crate::store::persistence::db::ops::authz::rate_limits::delete_by_scope(
            conn,
            Scope::User,
            u.id,
        )
        .await?;
        crate::store::persistence::db::ops::authz::quotas::delete_by_scope(conn, Scope::User, u.id)
            .await?;
    }
    user::Entity::delete_many()
        .filter(user::Column::OrgId.eq(org_id))
        .exec(conn)
        .await?;
    Ok(())
}

pub async fn clear_team(conn: &DatabaseConnection, team_id: i64) -> anyhow::Result<()> {
    let users = user::Entity::find()
        .filter(user::Column::TeamId.eq(team_id))
        .all(conn)
        .await?;
    for u in users {
        let mut am: user::ActiveModel = u.into();
        am.team_id = Set(None);
        am.update(conn).await?;
    }
    Ok(())
}
