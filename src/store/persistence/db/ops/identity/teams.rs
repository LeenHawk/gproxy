//! Team ops for the `db` backend. Unique per `(org_id, name)`.

use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::store::persistence::records::{Scope, Team, TeamInput};

use crate::store::persistence::db::entities::identity::team;

fn to_record(m: team::Model) -> Team {
    Team {
        id: m.id,
        org_id: m.org_id,
        name: m.name,
        enabled: m.enabled,
        created_at: m.created_at,
        updated_at: m.updated_at,
    }
}

pub async fn list(conn: &DatabaseConnection, org_id: i64) -> anyhow::Result<Vec<Team>> {
    Ok(team::Entity::find()
        .filter(team::Column::OrgId.eq(org_id))
        .all(conn)
        .await?
        .into_iter()
        .map(to_record)
        .collect())
}

pub async fn get(conn: &DatabaseConnection, id: i64) -> anyhow::Result<Option<Team>> {
    Ok(team::Entity::find_by_id(id).one(conn).await?.map(to_record))
}

pub async fn upsert(conn: &DatabaseConnection, input: TeamInput) -> anyhow::Result<Team> {
    let now = crate::store::persistence::db::ops::now_secs();

    // Enforce uniqueness on (org_id, name).
    if let Some(existing) = team::Entity::find()
        .filter(team::Column::OrgId.eq(input.org_id))
        .filter(team::Column::Name.eq(input.name.clone()))
        .one(conn)
        .await?
        && Some(existing.id) != input.id
    {
        return Err(crate::store::persistence::ConflictError::new(format!(
            "team name already exists in org {}: {}",
            input.org_id, input.name
        ))
        .into());
    }

    // Race backstop: the (org_id, name) pre-check above can be beaten under
    // concurrency / multi-instance; the DB unique index then fires on the write,
    // which we map to the same ConflictError (→ 409) rather than a 500.
    let conflict_org = input.org_id;
    let conflict_name = input.name.clone();
    let conflict = |e: sea_orm::DbErr| {
        crate::store::persistence::db::ops::conflict_if_unique(e, || {
            format!("team name already exists in org {conflict_org}: {conflict_name}")
        })
    };

    let model = match input.id {
        Some(id) => match team::Entity::find_by_id(id).one(conn).await? {
            Some(existing) => {
                let mut am: team::ActiveModel = existing.into();
                am.org_id = Set(input.org_id);
                am.name = Set(input.name);
                am.enabled = Set(input.enabled);
                am.updated_at = Set(now);
                am.update(conn).await.map_err(conflict)?
            }
            None => {
                // Seeding an empty store from a pinned bundle: insert WITH the
                // explicit id (the unique (org_id, name) precheck above already
                // ensured no conflicting row exists).
                team::ActiveModel {
                    id: Set(id),
                    org_id: Set(input.org_id),
                    name: Set(input.name),
                    enabled: Set(input.enabled),
                    created_at: Set(now),
                    updated_at: Set(now),
                }
                .insert(conn)
                .await
                .map_err(conflict)?
            }
        },
        None => team::ActiveModel {
            id: NotSet,
            org_id: Set(input.org_id),
            name: Set(input.name),
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
    // cascade: detach members and drop scope-bound rows for this team.
    super::users::clear_team(conn, id).await?;
    crate::store::persistence::db::ops::authz::route_permissions::delete_by_scope(
        conn,
        Scope::Team,
        id,
    )
    .await?;
    crate::store::persistence::db::ops::authz::rate_limits::delete_by_scope(conn, Scope::Team, id)
        .await?;
    crate::store::persistence::db::ops::authz::quotas::delete_by_scope(conn, Scope::Team, id)
        .await?;

    let res = team::Entity::delete_by_id(id).exec(conn).await?;
    Ok(res.rows_affected > 0)
}

pub async fn delete_by_org(conn: &DatabaseConnection, org_id: i64) -> anyhow::Result<()> {
    let teams = team::Entity::find()
        .filter(team::Column::OrgId.eq(org_id))
        .all(conn)
        .await?;
    for t in teams {
        super::users::clear_team(conn, t.id).await?;
        crate::store::persistence::db::ops::authz::route_permissions::delete_by_scope(
            conn,
            Scope::Team,
            t.id,
        )
        .await?;
        crate::store::persistence::db::ops::authz::rate_limits::delete_by_scope(
            conn,
            Scope::Team,
            t.id,
        )
        .await?;
        crate::store::persistence::db::ops::authz::quotas::delete_by_scope(conn, Scope::Team, t.id)
            .await?;
    }
    team::Entity::delete_many()
        .filter(team::Column::OrgId.eq(org_id))
        .exec(conn)
        .await?;
    Ok(())
}
