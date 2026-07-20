//! 批量操作:实体枚举、结果汇总,以及对 `&dyn PersistenceBackend` 逐条编排的
//! 尽力而为删除/启停。逐条循环复用各实体已有的单条 `delete_*`/`set_enabled`,
//! 保住级联正确性;不是单事务,单条失败不阻塞其余。
use super::PersistenceBackend;

/// 支持批量操作的管理实体。`usage` 仅支持删除。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdminEntity {
    Providers,
    Credentials,
    Routes,
    RouteMembers,
    Aliases,
    ProviderModels,
    PriceRules,
    RoutingRules,
    RuleSets,
    Rules,
    ProviderRuleSets,
    Orgs,
    Teams,
    Users,
    UserKeys,
    Usage,
}

impl AdminEntity {
    /// 解析 `/admin/batch/{seg}` 的路径段。
    pub fn from_seg(s: &str) -> Option<Self> {
        Some(match s {
            "providers" => Self::Providers,
            "credentials" => Self::Credentials,
            "routes" => Self::Routes,
            "route-members" => Self::RouteMembers,
            "aliases" => Self::Aliases,
            "provider-models" => Self::ProviderModels,
            "price-rules" => Self::PriceRules,
            "routing-rules" => Self::RoutingRules,
            "rule-sets" => Self::RuleSets,
            "rules" => Self::Rules,
            "provider-rule-sets" => Self::ProviderRuleSets,
            "orgs" => Self::Orgs,
            "teams" => Self::Teams,
            "users" => Self::Users,
            "user-keys" => Self::UserKeys,
            "usage" => Self::Usage,
            _ => return None,
        })
    }

    /// usage 没有 enabled 字段。
    pub fn supports_enable(self) -> bool {
        !matches!(self, Self::Usage)
    }
}

#[derive(Debug, Default, serde::Serialize)]
pub struct BatchOutcome {
    pub affected: u64,
    pub errors: Vec<BatchError>,
}

#[derive(Debug, serde::Serialize)]
pub struct BatchError {
    pub id: i64,
    pub message: String,
}

impl BatchOutcome {
    fn record(&mut self, id: i64, res: anyhow::Result<bool>) {
        match res {
            Ok(true) => self.affected += 1,
            Ok(false) => self.errors.push(BatchError {
                id,
                message: "not found".into(),
            }),
            Err(e) => self.errors.push(BatchError {
                id,
                message: e.to_string(),
            }),
        }
    }
}

/// 逐条删除:复用各实体已有的 `delete_*`(含应用层级联)。
pub async fn run_batch_delete(
    be: &dyn PersistenceBackend,
    entity: AdminEntity,
    ids: &[i64],
) -> BatchOutcome {
    let mut out = BatchOutcome::default();
    for &id in ids {
        let res = match entity {
            AdminEntity::Providers => be.delete_provider(id).await,
            AdminEntity::Credentials => be.delete_credential(id).await,
            AdminEntity::Routes => be.delete_route(id).await,
            AdminEntity::RouteMembers => be.delete_route_member(id).await,
            AdminEntity::Aliases => be.delete_alias(id).await,
            AdminEntity::ProviderModels => be.delete_provider_model(id).await,
            AdminEntity::PriceRules => be.delete_price_rule(id).await,
            AdminEntity::RoutingRules => be.delete_routing_rule(id).await,
            AdminEntity::RuleSets => be.delete_rule_set(id).await,
            AdminEntity::Rules => be.delete_rule(id).await,
            AdminEntity::ProviderRuleSets => be.delete_provider_rule_set(id).await,
            AdminEntity::Orgs => be.delete_org(id).await,
            AdminEntity::Teams => be.delete_team(id).await,
            AdminEntity::Users => be.delete_user(id).await,
            AdminEntity::UserKeys => be.delete_user_key(id).await,
            AdminEntity::Usage => be.delete_usage(id).await,
        };
        out.record(id, res);
    }
    out
}

/// 逐条启停:定向更新 enabled + updated_at(各后端 `set_enabled` 实现)。
pub async fn run_batch_set_enabled(
    be: &dyn PersistenceBackend,
    entity: AdminEntity,
    ids: &[i64],
    enabled: bool,
) -> BatchOutcome {
    let mut out = BatchOutcome::default();
    for &id in ids {
        out.record(id, be.set_enabled(entity, id, enabled).await);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::persistence::records::OrgInput;
    use crate::store::persistence::{DbPersistence, PersistenceBackend};

    async fn be() -> (DbPersistence, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        (
            DbPersistence::connect("sqlite::memory:").await.unwrap(),
            dir,
        )
    }

    #[tokio::test]
    async fn batch_delete_is_best_effort() {
        let (be, _d) = be().await;
        let a = be
            .upsert_org(OrgInput {
                id: None,
                name: "a".into(),
                enabled: true,
                description: None,
            })
            .await
            .unwrap();
        let b = be
            .upsert_org(OrgInput {
                id: None,
                name: "b".into(),
                enabled: true,
                description: None,
            })
            .await
            .unwrap();
        // 一个真实 id + 一个不存在 id → 删一个,报告一个失败。
        let out = run_batch_delete(&be, AdminEntity::Orgs, &[a.id, 999_999, b.id]).await;
        assert_eq!(out.affected, 2);
        assert_eq!(out.errors.len(), 1);
        assert_eq!(out.errors[0].id, 999_999);
        assert!(be.list_orgs().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn set_enabled_flips_only_enabled() {
        let (be, _d) = be().await;
        let o = be
            .upsert_org(OrgInput {
                id: None,
                name: "x".into(),
                enabled: true,
                description: Some("d".into()),
            })
            .await
            .unwrap();
        let out = run_batch_set_enabled(&be, AdminEntity::Orgs, &[o.id], false).await;
        assert_eq!(out.affected, 1);
        let got = be.get_org(o.id).await.unwrap().unwrap();
        assert!(!got.enabled);
        assert_eq!(got.description.as_deref(), Some("d")); // 其它字段不动
        assert_eq!(got.name, "x");
    }
}
