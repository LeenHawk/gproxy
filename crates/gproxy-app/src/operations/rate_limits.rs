//! Configured rate limits.
//!
//! The rows are here; the counters are in the cache, because several instances
//! share one limit and a per-process counter would multiply every limit by the
//! number of instances. A write therefore changes what is counted, never what
//! has been counted: the live window keeps running under its old ceiling until
//! it ends.
//!
//! The subject rule is the permission family's, for the same reason — a limit
//! with no subject applies to nobody and a limit with two subjects is a
//! conjunction nobody writes on purpose.

use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::{FixedDecimal, Repository, entity::limits::rate_limit};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, Select, Set};

use super::{
    Scope, Writer,
    crud::{self, Shape},
};
use crate::{
    AppError, Result,
    dto::{BatchItem, ListQuery, Page, RateLimitDto, RateLimitPatch, RateLimitWrite},
};

pub struct RateLimits<'a, C> {
    writer: Writer<'a, C>,
}

impl<'a, C> RateLimits<'a, C> {
    pub(crate) fn new(writer: Writer<'a, C>) -> Self {
        Self { writer }
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> RateLimits<'_, C> {
    pub async fn list(&self, query: ListQuery) -> Result<Page<RateLimitDto>> {
        crud::list(self, query).await
    }
    pub async fn get(&self, id: &str) -> Result<RateLimitDto> {
        crud::get(self, id).await
    }
    pub async fn create(&self, write: RateLimitWrite) -> Result<RateLimitDto> {
        crud::create(self, write).await
    }
    pub async fn update(&self, id: &str, patch: RateLimitPatch) -> Result<RateLimitDto> {
        crud::update(self, id, patch).await
    }
    pub async fn delete(&self, id: &str) -> Result<()> {
        crud::delete(self, id).await
    }
    pub async fn batch(
        &self,
        items: Vec<BatchItem<RateLimitWrite, RateLimitPatch>>,
    ) -> Result<Vec<Option<RateLimitDto>>> {
        crud::batch(self, items).await
    }

    async fn subject(
        &self,
        user_id: Option<String>,
        api_key_id: Option<String>,
    ) -> Result<(Option<String>, Option<String>)> {
        let user_id = crud::optional_text(user_id);
        let api_key_id = crud::optional_text(api_key_id);
        crud::one_subject(user_id.as_deref(), api_key_id.as_deref(), "rate limit")?;
        if let Some(user_id) = &user_id {
            crud::require_rows(
                self.writer.store().users(),
                "user",
                std::slice::from_ref(user_id),
            )
            .await?;
        }
        if let Some(api_key_id) = &api_key_id {
            crud::require_rows(
                self.writer.store().api_keys(),
                "api key",
                std::slice::from_ref(api_key_id),
            )
            .await?;
        }
        Ok((user_id, api_key_id))
    }
}

/// The window a counter is aligned to. Zero or negative would divide by zero
/// when the window start is computed (`now - now % period`), and a negative
/// one has no meaning to align to.
fn period(seconds: i64) -> Result<i64> {
    if seconds <= 0 {
        return Err(AppError::invalid("periodSeconds must be positive"));
    }
    Ok(seconds)
}

/// The ceiling. Zero is allowed and means "nothing may pass", which is a
/// legitimate way to switch a subject off without deleting its row; negative
/// is not a ceiling at all.
fn limit(value: &str) -> Result<FixedDecimal> {
    let parsed = crud::decimal(value, "limitValue")?;
    if parsed.atoms() < 0 {
        return Err(AppError::invalid("limitValue must not be negative"));
    }
    Ok(parsed)
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Shape<C> for RateLimits<'_, C> {
    type Entity = rate_limit::Entity;
    type Dto = RateLimitDto;
    type Write = RateLimitWrite;
    type Patch = RateLimitPatch;

    const ENTITY: &'static str = "rate limit";

    fn writer(&self) -> Writer<'_, C> {
        self.writer
    }
    fn repository(&self) -> Repository<'_, C, Self::Entity> {
        self.writer.store().rate_limits()
    }
    fn scopes(&self) -> Vec<Scope> {
        vec![Scope::RateLimits]
    }
    fn select(&self, query: &ListQuery) -> Select<Self::Entity> {
        let mut select = rate_limit::Entity::find();
        if let Some(user_id) = crud::optional_text(query.user_id.clone()) {
            select = select.filter(rate_limit::Column::UserId.eq(user_id));
        }
        if let Some(api_key_id) = crud::optional_text(query.api_key_id.clone()) {
            select = select.filter(rate_limit::Column::ApiKeyId.eq(api_key_id));
        }
        if let Some(enabled) = query.enabled {
            select = select.filter(rate_limit::Column::Enabled.eq(enabled));
        }
        if let Some(search) = crud::optional_text(query.search.clone()) {
            select = select.filter(rate_limit::Column::Metric.contains(&search));
        }
        select
    }

    async fn build(&self, write: RateLimitWrite) -> Result<(rate_limit::ActiveModel, String)> {
        let id = crud::id_or_new(write.id.as_deref())?;
        let (user_id, api_key_id) = self.subject(write.user_id, write.api_key_id).await?;
        let row = rate_limit::ActiveModel {
            id: Set(id.clone()),
            user_id: Set(user_id),
            api_key_id: Set(api_key_id),
            metric: Set(crud::text(&write.metric, "metric")?),
            limit_value: Set(limit(&write.limit_value)?),
            period_seconds: Set(period(write.period_seconds)?),
            model_pattern: Set(crud::optional_text(write.model_pattern)),
            enabled: Set(write.enabled.unwrap_or(true)),
        };
        Ok((row, id))
    }

    async fn change(
        &self,
        current: &rate_limit::Model,
        patch: RateLimitPatch,
    ) -> Result<rate_limit::ActiveModel> {
        let user_id = patch.user_id.clone().unwrap_or(current.user_id.clone());
        let api_key_id = patch
            .api_key_id
            .clone()
            .unwrap_or(current.api_key_id.clone());
        let (user_id, api_key_id) = self.subject(user_id, api_key_id).await?;
        let mut row = rate_limit::ActiveModel {
            id: Set(current.id.clone()),
            user_id: Set(user_id),
            api_key_id: Set(api_key_id),
            ..Default::default()
        };
        if let Some(metric) = patch.metric {
            row.metric = Set(crud::text(&metric, "metric")?);
        }
        if let Some(limit_value) = patch.limit_value {
            row.limit_value = Set(limit(&limit_value)?);
        }
        if let Some(period_seconds) = patch.period_seconds {
            row.period_seconds = Set(period(period_seconds)?);
        }
        if let Some(model_pattern) = patch.model_pattern {
            row.model_pattern = Set(crud::optional_text(model_pattern));
        }
        if let Some(enabled) = patch.enabled {
            row.enabled = Set(enabled);
        }
        Ok(row)
    }
}
