//! Budgets and operator limits, one family over one table.
//!
//! A `quotas` row is a window over a metric. Whose window it is depends on
//! `owner_kind`: a caller level the host names per request (`user`, `api_key`,
//! `team`, …) makes it a budget core checks before the first attempt, and
//! `credential` or `provider` makes it an operator limit that blocks an
//! upstream account when its window is spent. Both are written here, and a row
//! neither of them would compile is refused rather than skipped at assembly.

use gproxy_core::{BudgetData, BudgetOwner, CredentialLimit};
use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::{Repository, entity::limits::quota};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, Select, Set};

use super::{
    Scope, Writer,
    crud::{self, Shape},
};
use crate::{
    SdkError, SdkResult,
    dto::{
        BatchItem, BudgetStatusDto, CredentialLimitStatusDto, ListQuery, Page, QuotaDto,
        QuotaPatch, QuotaWrite,
    },
};

pub struct Quotas<'a, C> {
    writer: Writer<'a, C>,
}

impl<'a, C> Quotas<'a, C> {
    pub(crate) fn new(writer: Writer<'a, C>) -> Self {
        Self { writer }
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Quotas<'_, C> {
    pub async fn list(&self, query: ListQuery) -> SdkResult<Page<QuotaDto>> {
        crud::list(self, query).await
    }
    pub async fn get(&self, id: &str) -> SdkResult<QuotaDto> {
        crud::get(self, id).await
    }
    pub async fn create(&self, write: QuotaWrite) -> SdkResult<QuotaDto> {
        crud::create(self, write).await
    }
    pub async fn update(&self, id: &str, patch: QuotaPatch) -> SdkResult<QuotaDto> {
        crud::update(self, id, patch).await
    }
    pub async fn delete(&self, id: &str) -> SdkResult<()> {
        crud::delete(self, id).await
    }
    pub async fn batch(
        &self,
        items: Vec<BatchItem<QuotaWrite, QuotaPatch>>,
    ) -> SdkResult<Vec<Option<QuotaDto>>> {
        crud::batch(self, items).await
    }

    /// The current window of every enabled budget of `owners`, opened where it
    /// did not exist yet. Ordered by quota id.
    pub async fn budget_status(&self, owners: &[BudgetOwner]) -> SdkResult<Vec<BudgetStatusDto>> {
        Ok(self
            .writer
            .core()
            .budget_status(owners, crate::rt::now_ms())
            .await?
            .into_iter()
            .map(BudgetStatusDto::from)
            .collect())
    }

    /// Close a budget's open window and start a fresh one now. History is
    /// kept; the quota's anchor moves so later windows align to the reset.
    /// This is a state change, not a configuration one, so it advances no
    /// revision: the window rows are not part of the snapshot.
    pub async fn reset_budget(&self, quota_id: &str) -> SdkResult<BudgetStatusDto> {
        Ok(self
            .writer
            .core()
            .reset_budget(quota_id, crate::rt::now_ms())
            .await?
            .into())
    }

    /// Every operator limit covering `credential_id`, with its window's usage.
    pub async fn limit_status(
        &self,
        credential_id: &str,
    ) -> SdkResult<Vec<CredentialLimitStatusDto>> {
        Ok(self
            .writer
            .core()
            .credential_limit_status(credential_id, crate::rt::now_ms())
            .await?
            .into_iter()
            .map(CredentialLimitStatusDto::from)
            .collect())
    }

    /// Start an operator limit over on every credential it covers, clearing
    /// the blocks it caused. Returns those credential ids.
    pub async fn reset_limit(&self, quota_id: &str) -> SdkResult<Vec<String>> {
        Ok(self
            .writer
            .core()
            .reset_credential_limit(quota_id, crate::rt::now_ms())
            .await?)
    }
}

/// A row must be usable as one of the two things this table holds. Compiling
/// it here is the same check assembly runs, so a stored row is one core will
/// actually honour.
fn validate(row: &quota::Model) -> SdkResult<()> {
    let outcome = if CredentialLimit::is_limit_row(row) {
        CredentialLimit::compile(row).map(|_| ())
    } else {
        BudgetData::compile(row).map(|_| ())
    };
    outcome.map_err(|reason| {
        SdkError::invalid(format!(
            "quota is neither a usable caller budget nor a usable credential limit: {reason}"
        ))
    })
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Shape<C> for Quotas<'_, C> {
    type Entity = quota::Entity;
    type Dto = QuotaDto;
    type Write = QuotaWrite;
    type Patch = QuotaPatch;

    const ENTITY: &'static str = "quota";

    fn writer(&self) -> Writer<'_, C> {
        self.writer
    }
    fn repository(&self) -> Repository<'_, C, Self::Entity> {
        self.writer.store().quotas()
    }
    fn scopes(&self) -> Vec<Scope> {
        vec![Scope::Quotas]
    }
    fn select(&self, query: &ListQuery) -> Select<Self::Entity> {
        let mut select = quota::Entity::find();
        if let Some(kind) = crud::optional_text(query.owner_kind.clone()) {
            select = select.filter(quota::Column::OwnerKind.eq(kind));
        }
        if let Some(owner) = crud::optional_text(query.owner_id.clone()) {
            select = select.filter(quota::Column::OwnerId.eq(owner));
        }
        if let Some(search) = crud::optional_text(query.search.clone()) {
            select = select.filter(quota::Column::WindowKey.contains(&search));
        }
        if let Some(enabled) = query.enabled {
            select = select.filter(quota::Column::Enabled.eq(enabled));
        }
        select
    }

    async fn build(&self, write: QuotaWrite) -> SdkResult<(quota::ActiveModel, String)> {
        let id = crud::id_or_new(write.id.as_deref());
        let candidate = quota::Model {
            id: id.clone(),
            owner_kind: crud::text(&write.owner_kind, "ownerKind")?,
            owner_id: crud::text(&write.owner_id, "ownerId")?,
            window_key: crud::optional_text(write.window_key)
                .unwrap_or_else(|| "primary".to_owned()),
            metric: crud::text(&write.metric, "metric")?,
            unit: crud::text(&write.unit, "unit")?,
            limit_value: crud::decimal(&write.limit_value, "limitValue")?,
            period: crud::text(&write.period, "period")?,
            period_seconds: write.period_seconds,
            anchor_at_ms: write.anchor_at_ms,
            model_pattern: crud::optional_text(write.model_pattern),
            enabled: write.enabled.unwrap_or(true),
        };
        validate(&candidate)?;
        Ok((active(candidate), id))
    }

    async fn change(
        &self,
        current: &quota::Model,
        patch: QuotaPatch,
    ) -> SdkResult<quota::ActiveModel> {
        let mut merged = current.clone();
        if let Some(kind) = patch.owner_kind {
            merged.owner_kind = crud::text(&kind, "ownerKind")?;
        }
        if let Some(owner) = patch.owner_id {
            merged.owner_id = crud::text(&owner, "ownerId")?;
        }
        if let Some(key) = patch.window_key {
            merged.window_key = crud::text(&key, "windowKey")?;
        }
        if let Some(metric) = patch.metric {
            merged.metric = crud::text(&metric, "metric")?;
        }
        if let Some(unit) = patch.unit {
            merged.unit = crud::text(&unit, "unit")?;
        }
        if let Some(value) = patch.limit_value {
            merged.limit_value = crud::decimal(&value, "limitValue")?;
        }
        if let Some(period) = patch.period {
            merged.period = crud::text(&period, "period")?;
        }
        if let Some(seconds) = patch.period_seconds {
            merged.period_seconds = seconds;
        }
        if let Some(anchor) = patch.anchor_at_ms {
            merged.anchor_at_ms = anchor;
        }
        if let Some(pattern) = patch.model_pattern {
            merged.model_pattern = crud::optional_text(pattern);
        }
        if let Some(enabled) = patch.enabled {
            merged.enabled = enabled;
        }
        validate(&merged)?;
        Ok(active(merged))
    }
}

/// Every column set, because the row is validated as a whole: metric, unit and
/// period only make sense together.
fn active(model: quota::Model) -> quota::ActiveModel {
    quota::ActiveModel {
        id: Set(model.id),
        owner_kind: Set(model.owner_kind),
        owner_id: Set(model.owner_id),
        window_key: Set(model.window_key),
        metric: Set(model.metric),
        unit: Set(model.unit),
        limit_value: Set(model.limit_value),
        period: Set(model.period),
        period_seconds: Set(model.period_seconds),
        anchor_at_ms: Set(model.anchor_at_ms),
        model_pattern: Set(model.model_pattern),
        enabled: Set(model.enabled),
    }
}
