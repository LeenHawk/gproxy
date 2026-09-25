use super::{Scope, crud, rewrite::Rewrite};
use crate::{SdkResult, dto::RuleSetDto};
use gproxy_seaorm::{BatchConnectionTrait, BatchStatement};
use gproxy_store::entity::upstream::{
    provider_rewrite_rule_set as binding, rewrite_rule_set as set,
};
use sea_orm::{DbBackend, EntityTrait, QueryTrait, Set, sea_query::OnConflict};

pub(super) fn default_statements(
    provider_id: &str,
    name: &str,
    backend: DbBackend,
    ignore_existing: bool,
) -> Vec<BatchStatement> {
    let id = format!("gproxy:provider-default:{provider_id}");
    let now = crate::rt::now_ms();
    let mut set = set::Entity::insert(set::ActiveModel {
        id: Set(id.clone()),
        name: Set(name.into()),
        description: Set(None),
        enabled: Set(true),
        created_at_ms: Set(now),
        updated_at_ms: Set(now),
    });
    let mut binding = binding::Entity::insert(binding::ActiveModel {
        id: Set(id.clone()),
        provider_id: Set(provider_id.into()),
        rule_set_id: Set(id),
        sort_order: Set(0),
        enabled: Set(true),
        created_at_ms: Set(now),
        updated_at_ms: Set(now),
    });
    if ignore_existing {
        set = set.on_conflict(OnConflict::column(set::Column::Id).do_nothing().to_owned());
        binding = binding.on_conflict(
            OnConflict::columns([binding::Column::ProviderId, binding::Column::RuleSetId])
                .do_nothing()
                .to_owned(),
        );
    }
    vec![
        BatchStatement::Execute(set.build(backend)),
        BatchStatement::Execute(binding.build(backend)),
    ]
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Rewrite<'_, C> {
    /// Backfill older providers without replacing their rules or enablement.
    pub async fn ensure_default_set(&self, provider_id: &str) -> SdkResult<RuleSetDto> {
        let provider = crud::row::<C, super::providers::Providers<'_, C>>(
            &super::providers::Providers::new(self.writer),
            provider_id,
        )
        .await?;
        self.writer
            .commit(
                default_statements(provider_id, &provider.name, self.writer.backend(), true),
                &[Scope::Rewrite],
            )
            .await?;
        self.sets()
            .get(&format!("gproxy:provider-default:{provider_id}"))
            .await
    }
}
