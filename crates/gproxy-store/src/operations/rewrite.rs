use crate::{
    Repository, Result, StoreError,
    entity::upstream::{provider_rewrite_rule_set, rewrite_rule, rewrite_rule_set},
    error::invalid,
    repository::models,
};
use gproxy_seaorm::{BatchConnectionTrait, BatchStatement, SelectProjection};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect, QueryTrait,
};

pub struct RuleSetReplacement {
    pub id: String,
    pub rules: Vec<rewrite_rule::ActiveModel>,
}
#[derive(Clone, Debug)]
pub struct RuleSetData {
    pub entity: rewrite_rule_set::Model,
    pub rules: Vec<rewrite_rule::Model>,
}
#[derive(Clone, Debug)]
pub struct ProviderRules {
    pub provider_id: String,
    pub attachments: Vec<provider_rewrite_rule_set::Model>,
    pub rule_sets: Vec<RuleSetData>,
}
impl<C: BatchConnectionTrait> Repository<'_, C, rewrite_rule_set::Entity> {
    /// Entire replacement is one SQL transaction. A missing set with no new
    /// rules yields None; inserting into a missing set fails its FK and rolls back.
    pub async fn replace_rules_many(
        &self,
        replacements: Vec<RuleSetReplacement>,
    ) -> Result<Vec<Option<RuleSetData>>> {
        let backend = self.db.get_database_backend();
        let mut batch = Vec::new();
        let mut ids = Vec::new();
        for replacement in replacements {
            ids.push(replacement.id.clone());
            batch.push(BatchStatement::Execute(
                rewrite_rule::Entity::delete_many()
                    .filter(rewrite_rule::Column::RuleSetId.eq(&replacement.id))
                    .build(backend),
            ));
            for rule in replacement.rules {
                if rule.get(rewrite_rule::Column::RuleSetId).into_value()
                    != Some(replacement.id.clone().into())
                {
                    return Err(invalid(
                        "replacement rule must belong to the selected rule set",
                    ));
                }
                crate::repository::active_key::<rewrite_rule::Entity>(&rule)?;
                batch.push(BatchStatement::Execute(
                    rewrite_rule::Entity::insert(rule).build(backend),
                ));
            }
        }
        let writes = batch.len();
        for id in &ids {
            batch.push(BatchStatement::Query(
                rewrite_rule_set::Entity::find_by_id(id.clone()).batch_query(backend)?,
            ));
            batch.push(BatchStatement::Query(
                rewrite_rule::Entity::find()
                    .filter(rewrite_rule::Column::RuleSetId.eq(id))
                    .order_by_asc(rewrite_rule::Column::SortOrder)
                    .order_by_asc(rewrite_rule::Column::Id)
                    .batch_query(backend)?,
            ));
        }
        let mut results = self.db.batch(&batch).await?.into_iter().skip(writes);
        ids.into_iter()
            .map(|_| {
                let set = models::<rewrite_rule_set::Model>(
                    results.next().ok_or(StoreError::UnexpectedResult)?,
                )?
                .into_iter()
                .next();
                let rules = models::<rewrite_rule::Model>(
                    results.next().ok_or(StoreError::UnexpectedResult)?,
                )?;
                Ok(set.map(|entity| RuleSetData { entity, rules }))
            })
            .collect()
    }
    /// Only enabled attachments/sets/rules, ordered by attachment order then rule
    /// order. One coherent batch; no regex compilation or payload rewriting here.
    pub async fn for_providers(&self, ids: &[String]) -> Result<Vec<ProviderRules>> {
        let backend = self.db.get_database_backend();
        let mut batch = Vec::new();
        for id in ids {
            let attachments = provider_rewrite_rule_set::Entity::find()
                .filter(provider_rewrite_rule_set::Column::ProviderId.eq(id))
                .filter(provider_rewrite_rule_set::Column::Enabled.eq(true));
            let sets = rewrite_rule_set::Entity::find()
                .filter(rewrite_rule_set::Column::Enabled.eq(true))
                .filter(
                    rewrite_rule_set::Column::Id.in_subquery(
                        attachments
                            .clone()
                            .select_only()
                            .column(provider_rewrite_rule_set::Column::RuleSetId)
                            .into_query(),
                    ),
                );
            let rules = rewrite_rule::Entity::find()
                .filter(rewrite_rule::Column::Enabled.eq(true))
                .filter(
                    rewrite_rule::Column::RuleSetId.in_subquery(
                        sets.clone()
                            .select_only()
                            .column(rewrite_rule_set::Column::Id)
                            .into_query(),
                    ),
                );
            batch.push(
                attachments
                    .order_by_asc(provider_rewrite_rule_set::Column::SortOrder)
                    .order_by_asc(provider_rewrite_rule_set::Column::Id)
                    .batch_query(backend)?,
            );
            batch.push(sets.batch_query(backend)?);
            batch.push(
                rules
                    .order_by_asc(rewrite_rule::Column::SortOrder)
                    .order_by_asc(rewrite_rule::Column::Id)
                    .batch_query(backend)?,
            );
        }
        let mut sets = self.db.query_batch(&batch).await?.into_iter();
        ids.iter()
            .map(|id| {
                use sea_orm::FromQueryResult;
                let mut attachments = sets
                    .next()
                    .ok_or(StoreError::UnexpectedResult)?
                    .iter()
                    .map(|r| provider_rewrite_rule_set::Model::from_query_result(r, ""))
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                let rulesets = sets
                    .next()
                    .ok_or(StoreError::UnexpectedResult)?
                    .iter()
                    .map(|r| rewrite_rule_set::Model::from_query_result(r, ""))
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                let rules = sets
                    .next()
                    .ok_or(StoreError::UnexpectedResult)?
                    .iter()
                    .map(|r| rewrite_rule::Model::from_query_result(r, ""))
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                attachments.retain(|a| rulesets.iter().any(|s| s.id == a.rule_set_id));
                let rule_sets = attachments
                    .iter()
                    .filter_map(|a| rulesets.iter().find(|s| s.id == a.rule_set_id))
                    .map(|s| RuleSetData {
                        entity: s.clone(),
                        rules: rules
                            .iter()
                            .filter(|r| r.rule_set_id == s.id)
                            .cloned()
                            .collect(),
                    })
                    .collect();
                Ok(ProviderRules {
                    provider_id: id.clone(),
                    attachments,
                    rule_sets,
                })
            })
            .collect()
    }
}
