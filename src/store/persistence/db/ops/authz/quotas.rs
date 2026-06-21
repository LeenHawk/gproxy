//! Quota ops for the `db` backend. Unique per `(scope, scope_id)`.

use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::sea_query::Expr;
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::store::persistence::records::{Quota, QuotaInput, Scope};

use crate::store::persistence::db::entities::authz::quota;

fn to_record(m: quota::Model) -> anyhow::Result<Quota> {
    Ok(Quota {
        id: m.id,
        scope: Scope::parse(&m.scope)?,
        scope_id: m.scope_id,
        quota_total: m.quota_total.parse::<rust_decimal::Decimal>()?,
        cost_used: m.cost_used.parse::<rust_decimal::Decimal>()?,
        created_at: m.created_at,
        updated_at: m.updated_at,
    })
}

pub async fn get(
    conn: &DatabaseConnection,
    scope: Scope,
    scope_id: i64,
) -> anyhow::Result<Option<Quota>> {
    quota::Entity::find()
        .filter(quota::Column::Scope.eq(scope.as_str()))
        .filter(quota::Column::ScopeId.eq(scope_id))
        .one(conn)
        .await?
        .map(to_record)
        .transpose()
}

pub async fn upsert(conn: &DatabaseConnection, input: QuotaInput) -> anyhow::Result<Quota> {
    let now = crate::store::persistence::db::ops::now_secs();

    // Enforce uniqueness on (scope, scope_id): a row for this scope must be the
    // same record we are updating (if any).
    if let Some(existing) = quota::Entity::find()
        .filter(quota::Column::Scope.eq(input.scope.as_str()))
        .filter(quota::Column::ScopeId.eq(input.scope_id))
        .one(conn)
        .await?
        && Some(existing.id) != input.id
    {
        return Err(crate::store::persistence::ConflictError::new(format!(
            "quota already exists for scope {}:{}",
            input.scope.as_str(),
            input.scope_id
        ))
        .into());
    }

    // Race backstop: the (scope, scope_id) pre-check above can be beaten under
    // concurrency / multi-instance; the DB unique index then fires on the write,
    // which we map to the same ConflictError (→ 409) rather than a 500.
    let conflict_scope = input.scope.as_str().to_owned();
    let conflict_scope_id = input.scope_id;
    let conflict = |e: sea_orm::DbErr| {
        crate::store::persistence::db::ops::conflict_if_unique(e, || {
            format!("quota already exists for scope {conflict_scope}:{conflict_scope_id}")
        })
    };

    let model = match input.id {
        Some(id) => match quota::Entity::find_by_id(id).one(conn).await? {
            Some(existing) => {
                let mut am: quota::ActiveModel = existing.into();
                am.scope = Set(input.scope.as_str().to_owned());
                am.scope_id = Set(input.scope_id);
                am.quota_total = Set(input.quota_total.to_string());
                // cost_used is billing-owned (accumulated via add_cost). An admin
                // edit of an EXISTING quota must NOT clobber it — keep the stored
                // value (am.cost_used stays Set to `existing` from `.into()`).
                // Seeding/import (the insert branches below) still honors input.
                am.updated_at = Set(now);
                am.update(conn).await.map_err(conflict)?
            }
            None => {
                // Seeding an empty store from a pinned bundle: insert WITH the
                // explicit id (the unique (scope, scope_id) precheck above
                // already ensured no conflicting row exists).
                quota::ActiveModel {
                    id: Set(id),
                    scope: Set(input.scope.as_str().to_owned()),
                    scope_id: Set(input.scope_id),
                    quota_total: Set(input.quota_total.to_string()),
                    cost_used: Set(input.cost_used.to_string()),
                    created_at: Set(now),
                    updated_at: Set(now),
                }
                .insert(conn)
                .await
                .map_err(conflict)?
            }
        },
        None => quota::ActiveModel {
            id: NotSet,
            scope: Set(input.scope.as_str().to_owned()),
            scope_id: Set(input.scope_id),
            quota_total: Set(input.quota_total.to_string()),
            cost_used: Set(input.cost_used.to_string()),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(conn)
        .await
        .map_err(conflict)?,
    };

    to_record(model)
}

pub async fn delete(conn: &DatabaseConnection, id: i64) -> anyhow::Result<bool> {
    let res = quota::Entity::delete_by_id(id).exec(conn).await?;
    Ok(res.rows_affected > 0)
}

/// Atomically add `delta` to `cost_used` for the `(scope, scope_id)` row.
///
/// `cost_used` is a TEXT column holding the exact decimal string, so SQL `+`
/// cannot do the arithmetic. The read-add-write is guarded by a compare-and-
/// swap on the raw stored text (retried on contention) — a plain transaction
/// is NOT enough on Postgres/MySQL at READ COMMITTED, where two concurrent
/// SELECT-then-UPDATE transactions still lose one increment. No-op when the
/// row is absent (the request isn't cost-tracked).
pub async fn add_cost(
    conn: &DatabaseConnection,
    scope: Scope,
    scope_id: i64,
    delta: rust_decimal::Decimal,
) -> anyhow::Result<()> {
    const CAS_RETRIES: u32 = 5;
    let now = crate::store::persistence::db::ops::now_secs();
    for _ in 0..CAS_RETRIES {
        let Some(existing) = quota::Entity::find()
            .filter(quota::Column::Scope.eq(scope.as_str()))
            .filter(quota::Column::ScopeId.eq(scope_id))
            .one(conn)
            .await?
        else {
            return Ok(()); // no quota row → nothing to charge
        };
        let updated = existing.cost_used.parse::<rust_decimal::Decimal>()? + delta;
        let res = quota::Entity::update_many()
            .col_expr(quota::Column::CostUsed, Expr::value(updated.to_string()))
            .col_expr(quota::Column::UpdatedAt, Expr::value(now))
            .filter(quota::Column::Id.eq(existing.id))
            .filter(quota::Column::CostUsed.eq(existing.cost_used.clone()))
            .exec(conn)
            .await?;
        if res.rows_affected > 0 {
            return Ok(());
        }
    }
    anyhow::bail!(
        "quota add_cost: persistent write contention for {}:{scope_id}",
        scope.as_str()
    )
}

pub async fn delete_by_scope(
    conn: &DatabaseConnection,
    scope: Scope,
    scope_id: i64,
) -> anyhow::Result<()> {
    quota::Entity::delete_many()
        .filter(quota::Column::Scope.eq(scope.as_str()))
        .filter(quota::Column::ScopeId.eq(scope_id))
        .exec(conn)
        .await?;
    Ok(())
}
