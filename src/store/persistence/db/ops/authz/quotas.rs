//! Quota ops for the `db` backend. Unique per `(scope, scope_id)`.

use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::sea_query::Expr;
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::store::persistence::records::{Quota, QuotaInput, Scope};
use crate::util::timewindow;

use crate::store::persistence::db::entities::authz::quota;

fn to_record(m: quota::Model) -> anyhow::Result<Quota> {
    Ok(Quota {
        id: m.id,
        scope: Scope::parse(&m.scope)?,
        scope_id: m.scope_id,
        quota_total: m.quota_total.parse::<rust_decimal::Decimal>()?,
        quota_daily: m.quota_daily.map(|v| v.parse()).transpose()?,
        quota_weekly: m.quota_weekly.map(|v| v.parse()).transpose()?,
        quota_monthly: m.quota_monthly.map(|v| v.parse()).transpose()?,
        quota_5h: m.quota_5h.map(|v| v.parse()).transpose()?,
        quota_7d: m.quota_7d.map(|v| v.parse()).transpose()?,
        cost_used: m.cost_used.parse::<rust_decimal::Decimal>()?,
        day_used: m.day_used.parse()?,
        day_anchor: m.day_anchor,
        week_used: m.week_used.parse()?,
        week_anchor: m.week_anchor,
        month_used: m.month_used.parse()?,
        month_anchor: m.month_anchor,
        five_hour_used: m.five_hour_used.parse()?,
        five_hour_anchor: m.five_hour_anchor,
        seven_day_used: m.seven_day_used.parse()?,
        seven_day_anchor: m.seven_day_anchor,
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

pub async fn list_all(conn: &DatabaseConnection) -> anyhow::Result<Vec<Quota>> {
    quota::Entity::find()
        .all(conn)
        .await?
        .into_iter()
        .map(to_record)
        .collect()
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
                am.quota_daily = Set(input.quota_daily.map(|v| v.to_string()));
                am.quota_weekly = Set(input.quota_weekly.map(|v| v.to_string()));
                am.quota_monthly = Set(input.quota_monthly.map(|v| v.to_string()));
                am.quota_5h = Set(input.quota_5h.map(|v| v.to_string()));
                am.quota_7d = Set(input.quota_7d.map(|v| v.to_string()));
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
                    quota_daily: Set(input.quota_daily.map(|v| v.to_string())),
                    quota_weekly: Set(input.quota_weekly.map(|v| v.to_string())),
                    quota_monthly: Set(input.quota_monthly.map(|v| v.to_string())),
                    quota_5h: Set(input.quota_5h.map(|v| v.to_string())),
                    quota_7d: Set(input.quota_7d.map(|v| v.to_string())),
                    cost_used: Set(input.cost_used.to_string()),
                    day_used: Set("0".to_owned()),
                    day_anchor: Set(0),
                    week_used: Set("0".to_owned()),
                    week_anchor: Set(0),
                    month_used: Set("0".to_owned()),
                    month_anchor: Set(0),
                    five_hour_used: Set("0".to_owned()),
                    five_hour_anchor: Set(0),
                    seven_day_used: Set("0".to_owned()),
                    seven_day_anchor: Set(0),
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
            quota_daily: Set(input.quota_daily.map(|v| v.to_string())),
            quota_weekly: Set(input.quota_weekly.map(|v| v.to_string())),
            quota_monthly: Set(input.quota_monthly.map(|v| v.to_string())),
            quota_5h: Set(input.quota_5h.map(|v| v.to_string())),
            quota_7d: Set(input.quota_7d.map(|v| v.to_string())),
            cost_used: Set(input.cost_used.to_string()),
            day_used: Set("0".to_owned()),
            day_anchor: Set(0),
            week_used: Set("0".to_owned()),
            week_anchor: Set(0),
            month_used: Set("0".to_owned()),
            month_anchor: Set(0),
            five_hour_used: Set("0".to_owned()),
            five_hour_anchor: Set(0),
            seven_day_used: Set("0".to_owned()),
            seven_day_anchor: Set(0),
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
    let day_key = timewindow::day_key(now);
    let week_key = timewindow::week_key(now);
    let month_key = timewindow::month_key(now);
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
        let (day_anchor, day_used) =
            timewindow::accumulate(existing.day_anchor, &existing.day_used, day_key, delta)?;
        let (week_anchor, week_used) =
            timewindow::accumulate(existing.week_anchor, &existing.week_used, week_key, delta)?;
        let (month_anchor, month_used) = timewindow::accumulate(
            existing.month_anchor,
            &existing.month_used,
            month_key,
            delta,
        )?;
        let (five_hour_anchor, five_hour_used) = timewindow::accumulate_anchored(
            existing.five_hour_anchor,
            &existing.five_hour_used,
            now,
            timewindow::FIVE_HOURS_SECS,
            delta,
        )?;
        let (seven_day_anchor, seven_day_used) = timewindow::accumulate_anchored(
            existing.seven_day_anchor,
            &existing.seven_day_used,
            now,
            timewindow::SEVEN_DAYS_SECS,
            delta,
        )?;
        let res = quota::Entity::update_many()
            .col_expr(quota::Column::CostUsed, Expr::value(updated.to_string()))
            .col_expr(quota::Column::DayUsed, Expr::value(day_used.to_string()))
            .col_expr(quota::Column::DayAnchor, Expr::value(day_anchor))
            .col_expr(quota::Column::WeekUsed, Expr::value(week_used.to_string()))
            .col_expr(quota::Column::WeekAnchor, Expr::value(week_anchor))
            .col_expr(
                quota::Column::MonthUsed,
                Expr::value(month_used.to_string()),
            )
            .col_expr(quota::Column::MonthAnchor, Expr::value(month_anchor))
            .col_expr(
                quota::Column::FiveHourUsed,
                Expr::value(five_hour_used.to_string()),
            )
            .col_expr(quota::Column::FiveHourAnchor, Expr::value(five_hour_anchor))
            .col_expr(
                quota::Column::SevenDayUsed,
                Expr::value(seven_day_used.to_string()),
            )
            .col_expr(quota::Column::SevenDayAnchor, Expr::value(seven_day_anchor))
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
