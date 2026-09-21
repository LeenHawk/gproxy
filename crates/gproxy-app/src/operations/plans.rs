//! Downstream plans and their per-window allowances.
//!
//! A plan is backed by exactly one pool, and a plan limit belongs to exactly
//! one plan. Both are checked before the row is written rather than left to a
//! foreign key, so a caller gets `plan not found` instead of an opaque
//! database error — and, for a `fixed` window, so a limit that can never reset
//! is refused at the door.

use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::{
    Repository,
    entity::subscription::{plan, plan_limit},
};
use sea_orm::{ColumnTrait, Condition, EntityTrait, QueryFilter, Select, Set};

use super::{
    Scope, Writer,
    crud::{self, Shape},
};
use crate::{
    AppError, Result,
    dto::{
        BatchItem, ListQuery, Page, PlanDto, PlanLimitDto, PlanLimitPatch, PlanLimitWrite,
        PlanPatch, PlanWrite,
    },
};

/// The reset contract a plan limit shares with a quota.
pub const PERIODS: [&str; 5] = ["total", "fixed", "day", "week", "month"];
const FIXED: &str = "fixed";

pub struct Plans<'a, C> {
    writer: Writer<'a, C>,
}

impl<'a, C> Plans<'a, C> {
    pub(crate) fn new(writer: Writer<'a, C>) -> Self {
        Self { writer }
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Plans<'_, C> {
    pub async fn list(&self, query: ListQuery) -> Result<Page<PlanDto>> {
        crud::list(self, query).await
    }
    pub async fn get(&self, id: &str) -> Result<PlanDto> {
        crud::get(self, id).await
    }
    pub async fn create(&self, write: PlanWrite) -> Result<PlanDto> {
        crud::create(self, write).await
    }
    pub async fn update(&self, id: &str, patch: PlanPatch) -> Result<PlanDto> {
        crud::update(self, id, patch).await
    }
    /// Delete a plan.
    ///
    /// `subscriptions.plan_id` is `on_delete = "Restrict"`, so a plan with
    /// issued subscriptions cannot be deleted and the database says so. That
    /// is the entity's intent — retire a plan by disabling it, which stops new
    /// issuance without revoking what was already sold.
    pub async fn delete(&self, id: &str) -> Result<()> {
        crud::delete(self, id).await
    }
    pub async fn batch(
        &self,
        items: Vec<BatchItem<PlanWrite, PlanPatch>>,
    ) -> Result<Vec<Option<PlanDto>>> {
        crud::batch(self, items).await
    }

    async fn pool(&self, id: &str) -> Result<String> {
        let id = crud::text(id, "poolId")?;
        crud::require_rows(
            self.writer.store().subscription_pools(),
            "pool",
            std::slice::from_ref(&id),
        )
        .await?;
        Ok(id)
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Shape<C> for Plans<'_, C> {
    type Entity = plan::Entity;
    type Dto = PlanDto;
    type Write = PlanWrite;
    type Patch = PlanPatch;

    const ENTITY: &'static str = "plan";

    fn writer(&self) -> Writer<'_, C> {
        self.writer
    }
    fn repository(&self) -> Repository<'_, C, Self::Entity> {
        self.writer.store().subscription_plans()
    }
    fn scopes(&self) -> Vec<Scope> {
        vec![Scope::Subscriptions]
    }
    fn select(&self, query: &ListQuery) -> Select<Self::Entity> {
        let mut select = plan::Entity::find();
        if let Some(pool_id) = crud::optional_text(query.pool_id.clone()) {
            select = select.filter(plan::Column::PoolId.eq(pool_id));
        }
        if let Some(enabled) = query.enabled {
            select = select.filter(plan::Column::Enabled.eq(enabled));
        }
        if let Some(search) = crud::optional_text(query.search.clone()) {
            select = select.filter(plan::Column::Name.contains(&search));
        }
        select
    }

    async fn build(&self, write: PlanWrite) -> Result<(plan::ActiveModel, String)> {
        let id = crud::id_or_new(write.id.as_deref())?;
        let row = plan::ActiveModel {
            id: Set(id.clone()),
            pool_id: Set(self.pool(&write.pool_id).await?),
            name: Set(crud::text(&write.name, "name")?),
            codex_plan_type: Set(crud::optional_text(write.codex_plan_type)),
            claude_subscription_type: Set(crud::optional_text(write.claude_subscription_type)),
            claude_rate_limit_tier: Set(crud::optional_text(write.claude_rate_limit_tier)),
            enabled: Set(write.enabled.unwrap_or(true)),
        };
        Ok((row, id))
    }

    async fn change(&self, current: &plan::Model, patch: PlanPatch) -> Result<plan::ActiveModel> {
        let mut row = plan::ActiveModel {
            id: Set(current.id.clone()),
            ..Default::default()
        };
        if let Some(pool_id) = patch.pool_id {
            row.pool_id = Set(self.pool(&pool_id).await?);
        }
        if let Some(name) = patch.name {
            row.name = Set(crud::text(&name, "name")?);
        }
        if let Some(value) = patch.codex_plan_type {
            row.codex_plan_type = Set(crud::optional_text(value));
        }
        if let Some(value) = patch.claude_subscription_type {
            row.claude_subscription_type = Set(crud::optional_text(value));
        }
        if let Some(value) = patch.claude_rate_limit_tier {
            row.claude_rate_limit_tier = Set(crud::optional_text(value));
        }
        if let Some(enabled) = patch.enabled {
            row.enabled = Set(enabled);
        }
        Ok(row)
    }
}

pub struct PlanLimits<'a, C> {
    writer: Writer<'a, C>,
}

impl<'a, C> PlanLimits<'a, C> {
    pub(crate) fn new(writer: Writer<'a, C>) -> Self {
        Self { writer }
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> PlanLimits<'_, C> {
    pub async fn list(&self, query: ListQuery) -> Result<Page<PlanLimitDto>> {
        crud::list(self, query).await
    }
    pub async fn get(&self, id: &str) -> Result<PlanLimitDto> {
        crud::get(self, id).await
    }
    pub async fn create(&self, write: PlanLimitWrite) -> Result<PlanLimitDto> {
        crud::create(self, write).await
    }
    pub async fn update(&self, id: &str, patch: PlanLimitPatch) -> Result<PlanLimitDto> {
        crud::update(self, id, patch).await
    }
    pub async fn delete(&self, id: &str) -> Result<()> {
        crud::delete(self, id).await
    }
    pub async fn batch(
        &self,
        items: Vec<BatchItem<PlanLimitWrite, PlanLimitPatch>>,
    ) -> Result<Vec<Option<PlanLimitDto>>> {
        crud::batch(self, items).await
    }

    async fn plan(&self, id: &str) -> Result<String> {
        let id = crud::text(id, "planId")?;
        crud::require_rows(
            self.writer.store().subscription_plans(),
            "plan",
            std::slice::from_ref(&id),
        )
        .await?;
        Ok(id)
    }

    async fn window_key(
        &self,
        plan_id: &str,
        window_key: &str,
        exclude: Option<&str>,
    ) -> Result<String> {
        let window_key = crud::text(window_key, "windowKey")?;
        crud::unique(
            self.writer.store().subscription_plan_limits(),
            Condition::all()
                .add(plan_limit::Column::PlanId.eq(plan_id))
                .add(plan_limit::Column::WindowKey.eq(&window_key)),
            exclude,
            || format!("this plan already has a `{window_key}` window"),
        )
        .await?;
        Ok(window_key)
    }
}

/// A `fixed` window needs a positive duration; every other period derives its
/// boundary from the calendar or never resets, so a duration there would be
/// read by nothing.
fn window(period: &str, period_seconds: Option<i64>) -> Result<(String, Option<i64>)> {
    let period = crud::one_of(period, "period", &PERIODS)?;
    if period != FIXED {
        return Ok((period, None));
    }
    match period_seconds {
        Some(seconds) if seconds > 0 => Ok((period, Some(seconds))),
        _ => Err(AppError::invalid(
            "a fixed window needs a positive periodSeconds",
        )),
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Shape<C> for PlanLimits<'_, C> {
    type Entity = plan_limit::Entity;
    type Dto = PlanLimitDto;
    type Write = PlanLimitWrite;
    type Patch = PlanLimitPatch;

    const ENTITY: &'static str = "plan limit";

    fn writer(&self) -> Writer<'_, C> {
        self.writer
    }
    fn repository(&self) -> Repository<'_, C, Self::Entity> {
        self.writer.store().subscription_plan_limits()
    }
    fn scopes(&self) -> Vec<Scope> {
        vec![Scope::Subscriptions]
    }
    fn select(&self, query: &ListQuery) -> Select<Self::Entity> {
        let mut select = plan_limit::Entity::find();
        if let Some(plan_id) = crud::optional_text(query.plan_id.clone()) {
            select = select.filter(plan_limit::Column::PlanId.eq(plan_id));
        }
        if let Some(search) = crud::optional_text(query.search.clone()) {
            select = select.filter(plan_limit::Column::WindowKey.contains(&search));
        }
        select
    }

    async fn build(&self, write: PlanLimitWrite) -> Result<(plan_limit::ActiveModel, String)> {
        let id = crud::id_or_new(write.id.as_deref())?;
        let plan_id = self.plan(&write.plan_id).await?;
        let (period, period_seconds) = window(&write.period, write.period_seconds)?;
        let row = plan_limit::ActiveModel {
            id: Set(id.clone()),
            window_key: Set(self.window_key(&plan_id, &write.window_key, None).await?),
            plan_id: Set(plan_id),
            limit: Set(crud::decimal(&write.limit, "limit")?),
            period: Set(period),
            period_seconds: Set(period_seconds),
            model_pattern: Set(crud::optional_text(write.model_pattern)),
        };
        Ok((row, id))
    }

    async fn change(
        &self,
        current: &plan_limit::Model,
        patch: PlanLimitPatch,
    ) -> Result<plan_limit::ActiveModel> {
        let mut row = plan_limit::ActiveModel {
            id: Set(current.id.clone()),
            ..Default::default()
        };
        if let Some(window_key) = patch.window_key {
            row.window_key = Set(self
                .window_key(&current.plan_id, &window_key, Some(&current.id))
                .await?);
        }
        if let Some(value) = patch.limit {
            row.limit = Set(crud::decimal(&value, "limit")?);
        }
        // The period and its duration are one rule, so a patch that names
        // either is validated as the pair the row will end up holding.
        if patch.period.is_some() || patch.period_seconds.is_some() {
            let period = patch.period.unwrap_or_else(|| current.period.clone());
            let seconds = patch.period_seconds.unwrap_or(current.period_seconds);
            let (period, seconds) = window(&period, seconds)?;
            row.period = Set(period);
            row.period_seconds = Set(seconds);
        }
        if let Some(model_pattern) = patch.model_pattern {
            row.model_pattern = Set(crud::optional_text(model_pattern));
        }
        Ok(row)
    }
}
