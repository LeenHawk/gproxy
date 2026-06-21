//! Routing-rule ops for the `db` backend.

use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::store::persistence::records::{RoutingRule, RoutingRuleInput};

use crate::store::persistence::db::entities::transform::routing_rule;

fn to_record(m: routing_rule::Model) -> RoutingRule {
    RoutingRule {
        id: m.id,
        provider_id: m.provider_id,
        operation: m.operation,
        kind: m.kind,
        implementation: m.implementation,
        dest_operation: m.dest_operation,
        dest_kind: m.dest_kind,
        sort_order: m.sort_order,
        enabled: m.enabled,
        created_at: m.created_at,
        updated_at: m.updated_at,
    }
}

pub async fn list(conn: &DatabaseConnection, provider_id: i64) -> anyhow::Result<Vec<RoutingRule>> {
    Ok(routing_rule::Entity::find()
        .filter(routing_rule::Column::ProviderId.eq(provider_id))
        .all(conn)
        .await?
        .into_iter()
        .map(to_record)
        .collect())
}

pub async fn get(conn: &DatabaseConnection, id: i64) -> anyhow::Result<Option<RoutingRule>> {
    Ok(routing_rule::Entity::find_by_id(id)
        .one(conn)
        .await?
        .map(to_record))
}

pub async fn upsert(
    conn: &DatabaseConnection,
    input: RoutingRuleInput,
) -> anyhow::Result<RoutingRule> {
    let now = crate::store::persistence::db::ops::now_secs();

    if let Some(existing) = routing_rule::Entity::find()
        .filter(routing_rule::Column::ProviderId.eq(input.provider_id))
        .filter(routing_rule::Column::Operation.eq(input.operation.clone()))
        .filter(routing_rule::Column::Kind.eq(input.kind.clone()))
        .one(conn)
        .await?
        && Some(existing.id) != input.id
    {
        return Err(crate::store::persistence::ConflictError::new(format!(
            "routing rule already exists for provider {} ({}, {})",
            input.provider_id, input.operation, input.kind
        ))
        .into());
    }

    // Race backstop: the (provider_id, operation, kind) pre-check above can be
    // beaten under concurrency / multi-instance; the DB unique index then fires
    // on the write, which we map to the same ConflictError (→ 409) not a 500.
    let conflict_pid = input.provider_id;
    let conflict_op = input.operation.clone();
    let conflict_kind = input.kind.clone();
    let conflict = |e: sea_orm::DbErr| {
        crate::store::persistence::db::ops::conflict_if_unique(e, || {
            format!(
                "routing rule already exists for provider {conflict_pid} ({conflict_op}, {conflict_kind})"
            )
        })
    };

    let model = match input.id {
        Some(id) => match routing_rule::Entity::find_by_id(id).one(conn).await? {
            Some(existing) => {
                let mut am: routing_rule::ActiveModel = existing.into();
                am.provider_id = Set(input.provider_id);
                am.operation = Set(input.operation);
                am.kind = Set(input.kind);
                am.implementation = Set(input.implementation);
                am.dest_operation = Set(input.dest_operation);
                am.dest_kind = Set(input.dest_kind);
                am.sort_order = Set(input.sort_order);
                am.enabled = Set(input.enabled);
                am.updated_at = Set(now);
                am.update(conn).await.map_err(conflict)?
            }
            None => {
                // Seeding an empty store from a pinned bundle: insert WITH the
                // explicit id (the unique (provider_id, operation, kind)
                // precheck above already ensured no conflicting row exists).
                routing_rule::ActiveModel {
                    id: Set(id),
                    provider_id: Set(input.provider_id),
                    operation: Set(input.operation),
                    kind: Set(input.kind),
                    implementation: Set(input.implementation),
                    dest_operation: Set(input.dest_operation),
                    dest_kind: Set(input.dest_kind),
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
        None => routing_rule::ActiveModel {
            id: NotSet,
            provider_id: Set(input.provider_id),
            operation: Set(input.operation),
            kind: Set(input.kind),
            implementation: Set(input.implementation),
            dest_operation: Set(input.dest_operation),
            dest_kind: Set(input.dest_kind),
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
    let res = routing_rule::Entity::delete_by_id(id).exec(conn).await?;
    Ok(res.rows_affected > 0)
}

pub async fn delete_by_provider(conn: &DatabaseConnection, provider_id: i64) -> anyhow::Result<()> {
    routing_rule::Entity::delete_many()
        .filter(routing_rule::Column::ProviderId.eq(provider_id))
        .exec(conn)
        .await?;
    Ok(())
}
