//! Payload rewriting: reusable rule sets, their ordered rules, and the
//! providers they are attached to.
//!
//! Every rule is compiled with core's own compiler before it is stored. A rule
//! that does not compile is skipped at assembly with a log line nobody reads;
//! refusing it here is the difference between a typo you are told about and a
//! rewrite that silently never happens.

use std::sync::Arc;

use gproxy_seaorm::{BatchConnectionTrait, BatchStatement, SelectProjection};
use gproxy_store::{
    Repository,
    entity::upstream::{provider_rewrite_rule_set, rewrite_rule, rewrite_rule_set},
    operations::rewrite::RuleSetReplacement,
};
use sea_orm::{ColumnTrait, Condition, EntityTrait, QueryFilter, QueryOrder, Select, Set};

use super::{
    Scope, Writer,
    crud::{self, Shape},
};
use crate::{
    SdkError, SdkResult,
    dto::{
        BatchItem, ListQuery, Page, ProviderRuleSetDto, ProviderRuleSetPatch, ProviderRuleSetWrite,
        RewriteRuleDto, RewriteRulePatch, RewriteRuleWrite, RuleSetDto, RuleSetPatch, RuleSetWrite,
    },
};

const PHASES: [&str; 3] = ["request", "response", "both"];
const TARGETS: [&str; 3] = ["body", "header", "query"];

pub struct Rewrite<'a, C> {
    pub(super) writer: Writer<'a, C>,
}

impl<'a, C> Rewrite<'a, C> {
    pub(crate) fn new(writer: Writer<'a, C>) -> Self {
        Self { writer }
    }
    pub fn sets(&self) -> RuleSets<'a, C> {
        RuleSets {
            writer: self.writer,
        }
    }
    pub fn rules(&self) -> RewriteRules<'a, C> {
        RewriteRules {
            writer: self.writer,
        }
    }
    pub fn bindings(&self) -> ProviderRuleSets<'a, C> {
        ProviderRuleSets {
            writer: self.writer,
        }
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Rewrite<'_, C> {
    /// Replace a set's rules wholesale: one delete and the new inserts, in the
    /// same commit as the revision bump. This is what an editor saves, and it
    /// is why reordering rules never leaves a half-ordered set behind.
    pub async fn replace_rules(
        &self,
        set_id: &str,
        rules: Vec<RewriteRuleWrite>,
    ) -> SdkResult<Vec<RewriteRuleDto>> {
        crud::require_rows(
            self.writer.store().rewrite_rule_sets(),
            "rewrite rule set",
            std::slice::from_ref(&set_id.to_owned()),
        )
        .await?;
        let now = crate::rt::now_ms();
        let mut models = Vec::with_capacity(rules.len());
        for (index, write) in rules.into_iter().enumerate() {
            let mut model = rule_model(set_id, write, now)?;
            // The order a caller sends is the order it means, unless it said
            // otherwise: an editor's list has no sort field of its own.
            if model.sort_order == 0 {
                model.sort_order = i64::try_from(index).unwrap_or(0);
            }
            validate_rule(&model)?;
            models.push(active_rule(model));
        }
        let mut statements = self
            .writer
            .store()
            .rewrite_rule_sets()
            .replace_rules_statements(vec![RuleSetReplacement {
                id: set_id.to_owned(),
                rules: models,
            }])?;
        let writes = statements.len();
        statements.push(BatchStatement::Query(
            rewrite_rule::Entity::find()
                .filter(rewrite_rule::Column::RuleSetId.eq(set_id))
                .order_by_asc(rewrite_rule::Column::SortOrder)
                .order_by_asc(rewrite_rule::Column::Id)
                .batch_query(self.writer.backend())?,
        ));
        let (_, mut results) = self
            .writer
            .commit_results(statements, &[Scope::Rewrite])
            .await?;
        let read = results
            .drain(writes..)
            .next()
            .ok_or(gproxy_store::StoreError::UnexpectedResult)?;
        Ok(crud::rows::<rewrite_rule::Model>(read)?
            .into_iter()
            .map(RewriteRuleDto::from)
            .collect())
    }
}

/// Core's compiler is the authority on what a rule may say: phase and target
/// combinations, header names, JSON paths, the regexes and every filter.
fn validate_rule(model: &rewrite_rule::Model) -> SdkResult<()> {
    if model.action == "replace" {
        crud::text(&model.pattern, "pattern")?;
    }
    gproxy_core::rewrite::compile_rule(Arc::new(model.clone()))
        .map(|_| ())
        .map_err(|error| SdkError::invalid(format!("rewrite rule is not usable: {error}")))
}

/// A complete candidate row from a write, so the compiler sees exactly what
/// would be stored.
fn rule_model(
    rule_set_id: &str,
    write: RewriteRuleWrite,
    now_ms: i64,
) -> SdkResult<rewrite_rule::Model> {
    let target: rewrite_rule::RewriteTarget = crud::enumerated(
        write.target.as_deref().unwrap_or("body"),
        "target",
        &TARGETS,
    )?;
    let phase = write.phase.unwrap_or_else(|| "request".to_owned());
    let phase = crud::text(&phase, "phase")?.to_ascii_lowercase();
    if !PHASES.contains(&phase.as_str()) {
        return Err(SdkError::invalid(format!(
            "phase must be one of {}",
            PHASES.join(", ")
        )));
    }
    let action = write.action.unwrap_or_else(|| "replace".into());
    let pattern = if action == "replace" {
        crud::text(&write.pattern, "pattern")?
    } else {
        write.pattern
    };
    Ok(rewrite_rule::Model {
        id: crud::id_or_new(write.id.as_deref()),
        rule_set_id: rule_set_id.to_owned(),
        phase,
        action,
        target,
        target_name: crud::optional_text(write.target_name),
        paths: write.paths,
        pattern,
        replacement: write.replacement,
        filter_operation_keys: write.filter_operation_keys,
        filter_model_pattern: crud::optional_text(write.filter_model_pattern),
        filter_header_pattern: crud::optional_text(write.filter_header_pattern),
        filter_event_pattern: crud::optional_text(write.filter_event_pattern),
        filter_body: write.filter_body,
        filter_header: write.filter_header,
        sort_order: write.sort_order.unwrap_or(0),
        enabled: write.enabled.unwrap_or(true),
        created_at_ms: now_ms,
        updated_at_ms: now_ms,
    })
}

/// Every column set, because a rule is validated as a whole row: a patch that
/// wrote only its changed columns would have been compiled against a shape
/// the database never holds.
fn active_rule(model: rewrite_rule::Model) -> rewrite_rule::ActiveModel {
    rewrite_rule::ActiveModel {
        id: Set(model.id),
        rule_set_id: Set(model.rule_set_id),
        phase: Set(model.phase),
        action: Set(model.action),
        target: Set(model.target),
        target_name: Set(model.target_name),
        paths: Set(model.paths),
        pattern: Set(model.pattern),
        replacement: Set(model.replacement),
        filter_operation_keys: Set(model.filter_operation_keys),
        filter_model_pattern: Set(model.filter_model_pattern),
        filter_header_pattern: Set(model.filter_header_pattern),
        filter_event_pattern: Set(model.filter_event_pattern),
        filter_body: Set(model.filter_body),
        filter_header: Set(model.filter_header),
        sort_order: Set(model.sort_order),
        enabled: Set(model.enabled),
        created_at_ms: Set(model.created_at_ms),
        updated_at_ms: Set(model.updated_at_ms),
    }
}

pub struct RuleSets<'a, C> {
    writer: Writer<'a, C>,
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> RuleSets<'_, C> {
    pub async fn list(&self, query: ListQuery) -> SdkResult<Page<RuleSetDto>> {
        let mut page = crud::list(self, query).await?;
        self.provider_counts(&mut page.items).await?;
        Ok(page)
    }
    pub async fn get(&self, id: &str) -> SdkResult<RuleSetDto> {
        let mut row = crud::get(self, id).await?;
        self.provider_counts(std::slice::from_mut(&mut row)).await?;
        Ok(row)
    }
    pub async fn create(&self, write: RuleSetWrite) -> SdkResult<RuleSetDto> {
        crud::create(self, write).await
    }
    pub async fn update(&self, id: &str, patch: RuleSetPatch) -> SdkResult<RuleSetDto> {
        crud::update(self, id, patch).await
    }
    pub async fn delete(&self, id: &str) -> SdkResult<()> {
        crud::delete(self, id).await
    }
    pub async fn batch(
        &self,
        items: Vec<BatchItem<RuleSetWrite, RuleSetPatch>>,
    ) -> SdkResult<Vec<Option<RuleSetDto>>> {
        crud::batch(self, items).await
    }

    async fn provider_counts(&self, rows: &mut [RuleSetDto]) -> SdkResult<()> {
        if rows.is_empty() {
            return Ok(());
        }
        let counts = self
            .writer
            .store()
            .provider_rewrite_rule_sets()
            .count_many(
                rows.iter()
                    .map(|row| {
                        provider_rewrite_rule_set::Entity::find()
                            .filter(provider_rewrite_rule_set::Column::RuleSetId.eq(&row.id))
                    })
                    .collect(),
            )
            .await?;
        for (row, count) in rows.iter_mut().zip(counts) {
            row.provider_count = Some(count);
        }
        Ok(())
    }

    async fn name(&self, name: &str, exclude: Option<&str>) -> SdkResult<String> {
        let name = crud::text(name, "name")?;
        crud::unique(
            self.writer.store().rewrite_rule_sets(),
            Condition::all().add(rewrite_rule_set::Column::Name.eq(&name)),
            exclude,
            || format!("a rewrite rule set named `{name}` already exists"),
        )
        .await?;
        Ok(name)
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Shape<C> for RuleSets<'_, C> {
    type Entity = rewrite_rule_set::Entity;
    type Dto = RuleSetDto;
    type Write = RuleSetWrite;
    type Patch = RuleSetPatch;

    const ENTITY: &'static str = "rewrite rule set";

    fn writer(&self) -> Writer<'_, C> {
        self.writer
    }
    fn repository(&self) -> Repository<'_, C, Self::Entity> {
        self.writer.store().rewrite_rule_sets()
    }
    fn scopes(&self) -> Vec<Scope> {
        vec![Scope::Rewrite]
    }
    fn select(&self, query: &ListQuery) -> Select<Self::Entity> {
        let mut select = rewrite_rule_set::Entity::find();
        if let Some(search) = crud::optional_text(query.search.clone()) {
            select = select.filter(
                Condition::any()
                    .add(rewrite_rule_set::Column::Name.contains(&search))
                    .add(rewrite_rule_set::Column::Description.contains(&search)),
            );
        }
        if let Some(enabled) = query.enabled {
            select = select.filter(rewrite_rule_set::Column::Enabled.eq(enabled));
        }
        select
    }

    async fn build(
        &self,
        write: RuleSetWrite,
    ) -> SdkResult<(rewrite_rule_set::ActiveModel, String)> {
        let id = crud::id_or_new(write.id.as_deref());
        let now = crate::rt::now_ms();
        let row = rewrite_rule_set::ActiveModel {
            id: Set(id.clone()),
            name: Set(self.name(&write.name, None).await?),
            description: Set(crud::optional_text(write.description)),
            enabled: Set(write.enabled.unwrap_or(true)),
            created_at_ms: Set(now),
            updated_at_ms: Set(now),
        };
        Ok((row, id))
    }

    async fn change(
        &self,
        current: &rewrite_rule_set::Model,
        patch: RuleSetPatch,
    ) -> SdkResult<rewrite_rule_set::ActiveModel> {
        let mut row = rewrite_rule_set::ActiveModel {
            id: Set(current.id.clone()),
            updated_at_ms: Set(crate::rt::now_ms()),
            ..Default::default()
        };
        if let Some(name) = patch.name {
            row.name = Set(self.name(&name, Some(&current.id)).await?);
        }
        if let Some(description) = patch.description {
            row.description = Set(crud::optional_text(description));
        }
        if let Some(enabled) = patch.enabled {
            row.enabled = Set(enabled);
        }
        Ok(row)
    }
}

pub struct RewriteRules<'a, C> {
    writer: Writer<'a, C>,
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> RewriteRules<'_, C> {
    pub async fn list(&self, query: ListQuery) -> SdkResult<Page<RewriteRuleDto>> {
        crud::list(self, query).await
    }
    pub async fn get(&self, id: &str) -> SdkResult<RewriteRuleDto> {
        crud::get(self, id).await
    }
    pub async fn create(&self, write: RewriteRuleWrite) -> SdkResult<RewriteRuleDto> {
        crud::create(self, write).await
    }
    pub async fn update(&self, id: &str, patch: RewriteRulePatch) -> SdkResult<RewriteRuleDto> {
        crud::update(self, id, patch).await
    }
    pub async fn delete(&self, id: &str) -> SdkResult<()> {
        crud::delete(self, id).await
    }
    pub async fn batch(
        &self,
        items: Vec<BatchItem<RewriteRuleWrite, RewriteRulePatch>>,
    ) -> SdkResult<Vec<Option<RewriteRuleDto>>> {
        crud::batch(self, items).await
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Shape<C> for RewriteRules<'_, C> {
    type Entity = rewrite_rule::Entity;
    type Dto = RewriteRuleDto;
    type Write = RewriteRuleWrite;
    type Patch = RewriteRulePatch;

    const ENTITY: &'static str = "rewrite rule";

    fn writer(&self) -> Writer<'_, C> {
        self.writer
    }
    fn repository(&self) -> Repository<'_, C, Self::Entity> {
        self.writer.store().rewrite_rules()
    }
    fn scopes(&self) -> Vec<Scope> {
        vec![Scope::Rewrite]
    }
    fn select(&self, query: &ListQuery) -> Select<Self::Entity> {
        let mut select = rewrite_rule::Entity::find();
        if let Some(rule_set_id) = crud::optional_text(query.rule_set_id.clone()) {
            select = select.filter(rewrite_rule::Column::RuleSetId.eq(rule_set_id));
        }
        if let Some(enabled) = query.enabled {
            select = select.filter(rewrite_rule::Column::Enabled.eq(enabled));
        }
        select.order_by_asc(rewrite_rule::Column::SortOrder)
    }

    async fn build(
        &self,
        write: RewriteRuleWrite,
    ) -> SdkResult<(rewrite_rule::ActiveModel, String)> {
        let rule_set_id = crud::text(
            write.rule_set_id.as_deref().unwrap_or_default(),
            "ruleSetId",
        )?;
        crud::require_rows(
            self.writer.store().rewrite_rule_sets(),
            "rewrite rule set",
            std::slice::from_ref(&rule_set_id),
        )
        .await?;
        let model = rule_model(&rule_set_id, write, crate::rt::now_ms())?;
        validate_rule(&model)?;
        let id = model.id.clone();
        Ok((active_rule(model), id))
    }

    async fn change(
        &self,
        current: &rewrite_rule::Model,
        patch: RewriteRulePatch,
    ) -> SdkResult<rewrite_rule::ActiveModel> {
        let mut merged = current.clone();
        if let Some(action) = patch.action {
            merged.action = action;
        }
        if let Some(phase) = patch.phase {
            let phase = crud::text(&phase, "phase")?.to_ascii_lowercase();
            if !PHASES.contains(&phase.as_str()) {
                return Err(SdkError::invalid(format!(
                    "phase must be one of {}",
                    PHASES.join(", ")
                )));
            }
            merged.phase = phase;
        }
        if let Some(target) = patch.target {
            merged.target = crud::enumerated(&target, "target", &TARGETS)?;
        }
        if let Some(target_name) = patch.target_name {
            merged.target_name = crud::optional_text(target_name);
        }
        if let Some(paths) = patch.paths {
            merged.paths = paths;
        }
        if let Some(pattern) = patch.pattern {
            merged.pattern = if merged.action == "replace" {
                crud::text(&pattern, "pattern")?
            } else {
                pattern
            };
        }
        if let Some(replacement) = patch.replacement {
            merged.replacement = replacement;
        }
        if let Some(keys) = patch.filter_operation_keys {
            merged.filter_operation_keys = keys;
        }
        if let Some(value) = patch.filter_model_pattern {
            merged.filter_model_pattern = crud::optional_text(value);
        }
        if let Some(value) = patch.filter_header_pattern {
            merged.filter_header_pattern = crud::optional_text(value);
        }
        if let Some(value) = patch.filter_event_pattern {
            merged.filter_event_pattern = crud::optional_text(value);
        }
        if let Some(value) = patch.filter_body {
            merged.filter_body = value;
        }
        if let Some(value) = patch.filter_header {
            merged.filter_header = value;
        }
        if let Some(sort_order) = patch.sort_order {
            merged.sort_order = sort_order;
        }
        if let Some(enabled) = patch.enabled {
            merged.enabled = enabled;
        }
        merged.updated_at_ms = crate::rt::now_ms();
        validate_rule(&merged)?;
        Ok(active_rule(merged))
    }
}

pub struct ProviderRuleSets<'a, C> {
    writer: Writer<'a, C>,
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> ProviderRuleSets<'_, C> {
    pub async fn list(&self, query: ListQuery) -> SdkResult<Page<ProviderRuleSetDto>> {
        crud::list(self, query).await
    }
    pub async fn get(&self, id: &str) -> SdkResult<ProviderRuleSetDto> {
        crud::get(self, id).await
    }
    pub async fn create(&self, write: ProviderRuleSetWrite) -> SdkResult<ProviderRuleSetDto> {
        crud::create(self, write).await
    }
    pub async fn update(
        &self,
        id: &str,
        patch: ProviderRuleSetPatch,
    ) -> SdkResult<ProviderRuleSetDto> {
        crud::update(self, id, patch).await
    }
    pub async fn delete(&self, id: &str) -> SdkResult<()> {
        crud::delete(self, id).await
    }
    pub async fn batch(
        &self,
        items: Vec<BatchItem<ProviderRuleSetWrite, ProviderRuleSetPatch>>,
    ) -> SdkResult<Vec<Option<ProviderRuleSetDto>>> {
        crud::batch(self, items).await
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Shape<C> for ProviderRuleSets<'_, C> {
    type Entity = provider_rewrite_rule_set::Entity;
    type Dto = ProviderRuleSetDto;
    type Write = ProviderRuleSetWrite;
    type Patch = ProviderRuleSetPatch;

    const ENTITY: &'static str = "rewrite rule set attachment";

    fn writer(&self) -> Writer<'_, C> {
        self.writer
    }
    fn repository(&self) -> Repository<'_, C, Self::Entity> {
        self.writer.store().provider_rewrite_rule_sets()
    }
    fn scopes(&self) -> Vec<Scope> {
        vec![Scope::Rewrite]
    }
    fn select(&self, query: &ListQuery) -> Select<Self::Entity> {
        let mut select = provider_rewrite_rule_set::Entity::find();
        if let Some(provider_id) = crud::optional_text(query.provider_id.clone()) {
            select = select.filter(provider_rewrite_rule_set::Column::ProviderId.eq(provider_id));
        }
        if let Some(rule_set_id) = crud::optional_text(query.rule_set_id.clone()) {
            select = select.filter(provider_rewrite_rule_set::Column::RuleSetId.eq(rule_set_id));
        }
        if let Some(enabled) = query.enabled {
            select = select.filter(provider_rewrite_rule_set::Column::Enabled.eq(enabled));
        }
        select.order_by_asc(provider_rewrite_rule_set::Column::SortOrder)
    }

    async fn build(
        &self,
        write: ProviderRuleSetWrite,
    ) -> SdkResult<(provider_rewrite_rule_set::ActiveModel, String)> {
        let id = crud::id_or_new(write.id.as_deref());
        let provider_id = crud::text(&write.provider_id, "providerId")?;
        let rule_set_id = crud::text(&write.rule_set_id, "ruleSetId")?;
        crud::require_rows(
            self.writer.store().providers(),
            "provider",
            std::slice::from_ref(&provider_id),
        )
        .await?;
        crud::require_rows(
            self.writer.store().rewrite_rule_sets(),
            "rewrite rule set",
            std::slice::from_ref(&rule_set_id),
        )
        .await?;
        crud::unique(
            self.writer.store().provider_rewrite_rule_sets(),
            Condition::all()
                .add(provider_rewrite_rule_set::Column::ProviderId.eq(&provider_id))
                .add(provider_rewrite_rule_set::Column::RuleSetId.eq(&rule_set_id)),
            None,
            || format!("rule set `{rule_set_id}` is already attached to `{provider_id}`"),
        )
        .await?;
        let now = crate::rt::now_ms();
        let row = provider_rewrite_rule_set::ActiveModel {
            id: Set(id.clone()),
            provider_id: Set(write.provider_id.trim().to_owned()),
            rule_set_id: Set(write.rule_set_id.trim().to_owned()),
            sort_order: Set(write.sort_order.unwrap_or(0)),
            enabled: Set(write.enabled.unwrap_or(true)),
            created_at_ms: Set(now),
            updated_at_ms: Set(now),
        };
        Ok((row, id))
    }

    async fn change(
        &self,
        current: &provider_rewrite_rule_set::Model,
        patch: ProviderRuleSetPatch,
    ) -> SdkResult<provider_rewrite_rule_set::ActiveModel> {
        let mut row = provider_rewrite_rule_set::ActiveModel {
            id: Set(current.id.clone()),
            updated_at_ms: Set(crate::rt::now_ms()),
            ..Default::default()
        };
        if let Some(sort_order) = patch.sort_order {
            row.sort_order = Set(sort_order);
        }
        if let Some(enabled) = patch.enabled {
            row.enabled = Set(enabled);
        }
        Ok(row)
    }
}
