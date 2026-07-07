//! 批量原语(libsql/Hrana):定向 set_enabled + usage 单条删除(手写 SQL)。

use crate::store::libsql::{LibsqlClient, arg_integer};
use crate::store::persistence::batch::AdminEntity;
use crate::store::persistence::libsql::util::{arg_bool, exec, now_secs};

/// 实体 → 表名。usage 无 enabled,返回 None。
fn enabled_table(entity: AdminEntity) -> Option<&'static str> {
    Some(match entity {
        AdminEntity::Providers => "providers",
        AdminEntity::Credentials => "credentials",
        AdminEntity::ProviderModels => "provider_models",
        AdminEntity::PriceRules => "price_rules",
        AdminEntity::Routes => "routes",
        AdminEntity::RouteMembers => "route_members",
        AdminEntity::Aliases => "aliases",
        AdminEntity::RoutingRules => "routing_rules",
        AdminEntity::RuleSets => "rule_sets",
        AdminEntity::Rules => "rules",
        AdminEntity::ProviderRuleSets => "provider_rule_sets",
        AdminEntity::Orgs => "orgs",
        AdminEntity::Teams => "teams",
        AdminEntity::Users => "users",
        AdminEntity::UserKeys => "user_keys",
        AdminEntity::Usage => return None,
    })
}

/// 定向 UPDATE {table} SET enabled = ?, updated_at = ? WHERE id = ?
/// 表名来自白名单,非用户输入,无注入风险。
pub async fn set_enabled(
    client: &LibsqlClient,
    entity: AdminEntity,
    id: i64,
    enabled: bool,
) -> anyhow::Result<bool> {
    let table = match enabled_table(entity) {
        Some(t) => t,
        None => anyhow::bail!("usage has no enabled field"),
    };
    let now = now_secs();
    let sql = format!("UPDATE {table} SET enabled = ?, updated_at = ? WHERE id = ?");
    let n = exec(
        client,
        &sql,
        &[arg_bool(enabled), arg_integer(now), arg_integer(id)],
    )
    .await?;
    Ok(n > 0)
}

/// DELETE FROM usages WHERE id = ?
pub async fn delete_usage(client: &LibsqlClient, id: i64) -> anyhow::Result<bool> {
    let n = exec(
        client,
        "DELETE FROM usages WHERE id = ?",
        &[arg_integer(id)],
    )
    .await?;
    Ok(n > 0)
}
