//! Usage-rollup ops for the `db` backend (accumulate by dimension bucket).

use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::sea_query::{Expr, ExprTrait};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Select,
};

use crate::store::persistence::records::{UsageRollup, UsageRollupInput};

use crate::store::persistence::db::entities::usage::usage_rollup;
use crate::store::persistence::db::ops::is_unique_violation;

fn to_record(m: usage_rollup::Model) -> anyhow::Result<UsageRollup> {
    Ok(UsageRollup {
        id: m.id,
        granularity: m.granularity,
        bucket_start: m.bucket_start,
        provider_id: m.provider_id,
        org_id: m.org_id,
        team_id: m.team_id,
        user_id: m.user_id,
        route_name: m.route_name,
        model: m.model,
        requests: m.requests,
        input_tokens: m.input_tokens,
        output_tokens: m.output_tokens,
        cost: m.cost.parse::<rust_decimal::Decimal>()?,
        created_at: m.created_at,
        updated_at: m.updated_at,
    })
}

/// Select the bucket matching ALL of `input`'s dimensions (incl. None).
fn find_bucket(input: &UsageRollupInput) -> Select<usage_rollup::Entity> {
    usage_rollup::Entity::find()
        .filter(usage_rollup::Column::Granularity.eq(input.granularity.clone()))
        .filter(usage_rollup::Column::BucketStart.eq(input.bucket_start))
        .filter(match input.provider_id {
            Some(v) => usage_rollup::Column::ProviderId.eq(v),
            None => usage_rollup::Column::ProviderId.is_null(),
        })
        .filter(match input.org_id {
            Some(v) => usage_rollup::Column::OrgId.eq(v),
            None => usage_rollup::Column::OrgId.is_null(),
        })
        .filter(match input.team_id {
            Some(v) => usage_rollup::Column::TeamId.eq(v),
            None => usage_rollup::Column::TeamId.is_null(),
        })
        .filter(match input.user_id {
            Some(v) => usage_rollup::Column::UserId.eq(v),
            None => usage_rollup::Column::UserId.is_null(),
        })
        .filter(match input.route_name.clone() {
            Some(v) => usage_rollup::Column::RouteName.eq(v),
            None => usage_rollup::Column::RouteName.is_null(),
        })
        .filter(match input.model.clone() {
            Some(v) => usage_rollup::Column::Model.eq(v),
            None => usage_rollup::Column::Model.is_null(),
        })
}

/// Accumulate `input` into its dimension bucket. The integer columns add IN
/// SQL (atomic regardless of concurrent writers — a plain read-add-write loses
/// increments across instances sharing one database); `cost` is a TEXT decimal,
/// so it rides a compare-and-swap on the raw stored text, retried on
/// contention. A failed CAS applies nothing (single guarded UPDATE), so a
/// retry never double-counts the integer columns. First-insert races collide
/// on the `uq_usage_rollups_dims` unique index — the loser retries into the
/// accumulate path.
pub async fn add(
    conn: &DatabaseConnection,
    input: UsageRollupInput,
) -> anyhow::Result<UsageRollup> {
    const CAS_RETRIES: u32 = 5;
    let now = crate::store::persistence::db::ops::now_secs();

    for _ in 0..CAS_RETRIES {
        let Some(existing) = find_bucket(&input).one(conn).await? else {
            let insert = usage_rollup::ActiveModel {
                id: NotSet,
                granularity: Set(input.granularity.clone()),
                bucket_start: Set(input.bucket_start),
                provider_id: Set(input.provider_id),
                org_id: Set(input.org_id),
                team_id: Set(input.team_id),
                user_id: Set(input.user_id),
                route_name: Set(input.route_name.clone()),
                model: Set(input.model.clone()),
                requests: Set(input.requests),
                input_tokens: Set(input.input_tokens),
                output_tokens: Set(input.output_tokens),
                cost: Set(input.cost.to_string()),
                created_at: Set(now),
                updated_at: Set(now),
            }
            .insert(conn)
            .await;
            match insert {
                Ok(model) => return to_record(model),
                // Concurrent first insert won the bucket: re-find → CAS update.
                Err(e) if is_unique_violation(&e) => continue,
                Err(e) => return Err(e.into()),
            }
        };

        let cost = existing.cost.parse::<rust_decimal::Decimal>()? + input.cost;
        let res = usage_rollup::Entity::update_many()
            .col_expr(
                usage_rollup::Column::Requests,
                Expr::col(usage_rollup::Column::Requests).add(input.requests),
            )
            .col_expr(
                usage_rollup::Column::InputTokens,
                Expr::col(usage_rollup::Column::InputTokens).add(input.input_tokens),
            )
            .col_expr(
                usage_rollup::Column::OutputTokens,
                Expr::col(usage_rollup::Column::OutputTokens).add(input.output_tokens),
            )
            .col_expr(usage_rollup::Column::Cost, Expr::value(cost.to_string()))
            .col_expr(usage_rollup::Column::UpdatedAt, Expr::value(now))
            .filter(usage_rollup::Column::Id.eq(existing.id))
            .filter(usage_rollup::Column::Cost.eq(existing.cost.clone()))
            .exec(conn)
            .await?;
        if res.rows_affected > 0 {
            let model = usage_rollup::Entity::find_by_id(existing.id)
                .one(conn)
                .await?
                .ok_or_else(|| anyhow::anyhow!("rollup bucket vanished after update"))?;
            return to_record(model);
        }
    }
    anyhow::bail!("usage_rollup add: persistent write contention")
}

pub async fn list(
    conn: &DatabaseConnection,
    granularity: &str,
    from: i64,
    to: i64,
    user_id: Option<i64>,
) -> anyhow::Result<Vec<UsageRollup>> {
    let mut sel = usage_rollup::Entity::find()
        .filter(usage_rollup::Column::Granularity.eq(granularity))
        .filter(usage_rollup::Column::BucketStart.gte(from))
        .filter(usage_rollup::Column::BucketStart.lte(to));
    if let Some(v) = user_id {
        sel = sel.filter(usage_rollup::Column::UserId.eq(v));
    }
    sel.order_by_asc(usage_rollup::Column::BucketStart)
        .all(conn)
        .await?
        .into_iter()
        .map(to_record)
        .collect()
}
