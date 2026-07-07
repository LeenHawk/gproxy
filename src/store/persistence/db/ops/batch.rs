//! 批量原语(SeaORM):定向 set_enabled + usage 单条删除。
use sea_orm::sea_query::Expr;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use crate::store::persistence::batch::AdminEntity;
use crate::store::persistence::db::entities as e;
use crate::store::persistence::db::ops::now_secs;

/// 一条 `UPDATE {table} SET enabled=?, updated_at=? WHERE id=?`。
macro_rules! set_enabled_arm {
    ($conn:expr, $id:expr, $enabled:expr, $now:expr, $m:path) => {{
        use $m as m;
        let res = m::Entity::update_many()
            .col_expr(m::Column::Enabled, Expr::value($enabled))
            .col_expr(m::Column::UpdatedAt, Expr::value($now))
            .filter(m::Column::Id.eq($id))
            .exec($conn)
            .await?;
        Ok(res.rows_affected > 0)
    }};
}

pub async fn set_enabled(
    conn: &DatabaseConnection,
    entity: AdminEntity,
    id: i64,
    enabled: bool,
) -> anyhow::Result<bool> {
    let now = now_secs();
    match entity {
        AdminEntity::Providers => {
            set_enabled_arm!(conn, id, enabled, now, e::provider::provider)
        }
        AdminEntity::Credentials => {
            set_enabled_arm!(conn, id, enabled, now, e::provider::credential)
        }
        AdminEntity::ProviderModels => {
            set_enabled_arm!(conn, id, enabled, now, e::provider::provider_model)
        }
        AdminEntity::PriceRules => {
            set_enabled_arm!(conn, id, enabled, now, e::pricing::price_rule)
        }
        AdminEntity::Routes => {
            set_enabled_arm!(conn, id, enabled, now, e::routing::route)
        }
        AdminEntity::RouteMembers => {
            set_enabled_arm!(conn, id, enabled, now, e::routing::route_member)
        }
        AdminEntity::Aliases => {
            set_enabled_arm!(conn, id, enabled, now, e::routing::alias)
        }
        AdminEntity::RoutingRules => {
            set_enabled_arm!(conn, id, enabled, now, e::transform::routing_rule)
        }
        AdminEntity::RuleSets => {
            set_enabled_arm!(conn, id, enabled, now, e::transform::rule_set)
        }
        AdminEntity::Rules => {
            set_enabled_arm!(conn, id, enabled, now, e::transform::rule)
        }
        AdminEntity::ProviderRuleSets => {
            set_enabled_arm!(conn, id, enabled, now, e::transform::provider_rule_set)
        }
        AdminEntity::Orgs => {
            set_enabled_arm!(conn, id, enabled, now, e::identity::org)
        }
        AdminEntity::Teams => {
            set_enabled_arm!(conn, id, enabled, now, e::identity::team)
        }
        AdminEntity::Users => {
            set_enabled_arm!(conn, id, enabled, now, e::identity::user)
        }
        AdminEntity::UserKeys => {
            set_enabled_arm!(conn, id, enabled, now, e::identity::user_key)
        }
        AdminEntity::Usage => anyhow::bail!("usage has no enabled field"),
    }
}

pub async fn delete_usage(conn: &DatabaseConnection, id: i64) -> anyhow::Result<bool> {
    let res = e::usage::usage::Entity::delete_many()
        .filter(e::usage::usage::Column::Id.eq(id))
        .exec(conn)
        .await?;
    Ok(res.rows_affected > 0)
}
