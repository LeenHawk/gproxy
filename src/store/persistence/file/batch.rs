//! 批量原语(file 后端):定向 set_enabled + usage 单条删除。
use crate::store::persistence::batch::AdminEntity;
use crate::store::persistence::file::table;
use crate::store::persistence::records::{
    Alias, Credential, Org, PriceRule, Provider, ProviderModel, ProviderRuleSet, Route,
    RouteMember, RoutingRule, Rule, RuleSet, Team, Usage, User, UserKey,
};
use std::path::Path;

/// 加载实体表 → 命中 id 改 enabled+updated_at → 回写;返回是否命中。
macro_rules! set_enabled_arm {
    ($root:expr, $id:expr, $enabled:expr, $now:expr, $m:path, $Rec:ty) => {{
        use $m as m;
        let file = m::path($root);
        let mut t = table::load::<$Rec>(&file).await?;
        let mut hit = false;
        for r in t.rows.iter_mut() {
            if r.id == $id {
                r.enabled = $enabled;
                r.updated_at = $now;
                hit = true;
                break;
            }
        }
        if hit {
            table::store(&file, &t).await?;
        }
        Ok(hit)
    }};
}

pub async fn set_enabled(
    root: &Path,
    entity: AdminEntity,
    id: i64,
    enabled: bool,
) -> anyhow::Result<bool> {
    let now = table::now_secs();
    use crate::store::persistence::file as f;
    match entity {
        AdminEntity::Providers => {
            set_enabled_arm!(root, id, enabled, now, f::provider::providers, Provider)
        }
        AdminEntity::Credentials => {
            set_enabled_arm!(root, id, enabled, now, f::provider::credentials, Credential)
        }
        AdminEntity::ProviderModels => {
            set_enabled_arm!(
                root,
                id,
                enabled,
                now,
                f::provider::provider_models,
                ProviderModel
            )
        }
        AdminEntity::PriceRules => {
            set_enabled_arm!(root, id, enabled, now, f::pricing::price_rules, PriceRule)
        }
        AdminEntity::Routes => {
            set_enabled_arm!(root, id, enabled, now, f::routing::routes, Route)
        }
        AdminEntity::RouteMembers => {
            set_enabled_arm!(
                root,
                id,
                enabled,
                now,
                f::routing::route_members,
                RouteMember
            )
        }
        AdminEntity::Aliases => {
            set_enabled_arm!(root, id, enabled, now, f::routing::aliases, Alias)
        }
        AdminEntity::RoutingRules => {
            set_enabled_arm!(
                root,
                id,
                enabled,
                now,
                f::transform::routing_rules,
                RoutingRule
            )
        }
        AdminEntity::RuleSets => {
            set_enabled_arm!(root, id, enabled, now, f::transform::rule_sets, RuleSet)
        }
        AdminEntity::Rules => {
            set_enabled_arm!(root, id, enabled, now, f::transform::rules, Rule)
        }
        AdminEntity::ProviderRuleSets => {
            set_enabled_arm!(
                root,
                id,
                enabled,
                now,
                f::transform::provider_rule_sets,
                ProviderRuleSet
            )
        }
        AdminEntity::Orgs => {
            set_enabled_arm!(root, id, enabled, now, f::identity::orgs, Org)
        }
        AdminEntity::Teams => {
            set_enabled_arm!(root, id, enabled, now, f::identity::teams, Team)
        }
        AdminEntity::Users => {
            set_enabled_arm!(root, id, enabled, now, f::identity::users, User)
        }
        AdminEntity::UserKeys => {
            set_enabled_arm!(root, id, enabled, now, f::identity::user_keys, UserKey)
        }
        AdminEntity::Usage => anyhow::bail!("usage has no enabled field"),
    }
}

pub async fn delete_usage(root: &Path, id: i64) -> anyhow::Result<bool> {
    use crate::store::persistence::file::usage::usages;
    let file = usages::path(root);
    let mut t = table::load::<Usage>(&file).await?;
    let before = t.rows.len();
    t.rows.retain(|r| r.id != id);
    let removed = before != t.rows.len();
    if removed {
        table::store(&file, &t).await?;
    }
    Ok(removed)
}
