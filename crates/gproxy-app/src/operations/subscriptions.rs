//! Issued subscriptions: one user's entitlement to one plan.

use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::{Repository, entity::subscription::user_subscription};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, Select, Set};

use super::{
    Scope, Writer,
    crud::{self, Shape},
};
use crate::{
    Result,
    dto::{BatchItem, ListQuery, Page, SubscriptionDto, SubscriptionPatch, SubscriptionWrite},
};

pub struct Subscriptions<'a, C> {
    writer: Writer<'a, C>,
}

impl<'a, C> Subscriptions<'a, C> {
    pub(crate) fn new(writer: Writer<'a, C>) -> Self {
        Self { writer }
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Subscriptions<'_, C> {
    pub async fn list(&self, query: ListQuery) -> Result<Page<SubscriptionDto>> {
        crud::list(self, query).await
    }
    pub async fn get(&self, id: &str) -> Result<SubscriptionDto> {
        crud::get(self, id).await
    }
    pub async fn create(&self, write: SubscriptionWrite) -> Result<SubscriptionDto> {
        crud::create(self, write).await
    }
    pub async fn update(&self, id: &str, patch: SubscriptionPatch) -> Result<SubscriptionDto> {
        crud::update(self, id, patch).await
    }
    /// Delete a subscription. `api_keys.subscription_id` cascades on a fresh
    /// database; unlike the organization and team bindings that column was
    /// part of the original table, so its foreign key exists everywhere and
    /// this layer has nothing to replicate.
    pub async fn delete(&self, id: &str) -> Result<()> {
        crud::delete(self, id).await
    }
    pub async fn batch(
        &self,
        items: Vec<BatchItem<SubscriptionWrite, SubscriptionPatch>>,
    ) -> Result<Vec<Option<SubscriptionDto>>> {
        crud::batch(self, items).await
    }

    async fn user(&self, id: &str) -> Result<String> {
        let id = crud::text(id, "userId")?;
        crud::require_rows(
            self.writer.store().users(),
            "user",
            std::slice::from_ref(&id),
        )
        .await?;
        Ok(id)
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
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Shape<C> for Subscriptions<'_, C> {
    type Entity = user_subscription::Entity;
    type Dto = SubscriptionDto;
    type Write = SubscriptionWrite;
    type Patch = SubscriptionPatch;

    const ENTITY: &'static str = "subscription";

    fn writer(&self) -> Writer<'_, C> {
        self.writer
    }
    fn repository(&self) -> Repository<'_, C, Self::Entity> {
        self.writer.store().subscriptions()
    }
    fn scopes(&self) -> Vec<Scope> {
        vec![Scope::Subscriptions]
    }
    fn select(&self, query: &ListQuery) -> Select<Self::Entity> {
        let mut select = user_subscription::Entity::find();
        if let Some(user_id) = crud::optional_text(query.user_id.clone()) {
            select = select.filter(user_subscription::Column::UserId.eq(user_id));
        }
        if let Some(plan_id) = crud::optional_text(query.plan_id.clone()) {
            select = select.filter(user_subscription::Column::PlanId.eq(plan_id));
        }
        if let Some(enabled) = query.enabled {
            select = select.filter(user_subscription::Column::Enabled.eq(enabled));
        }
        select
    }

    async fn build(
        &self,
        write: SubscriptionWrite,
    ) -> Result<(user_subscription::ActiveModel, String)> {
        let id = crud::id_or_new(write.id.as_deref())?;
        let now = crate::now_ms();
        let starts_at_ms = write.starts_at_ms.unwrap_or(now);
        crud::ordered_range(Some(starts_at_ms), write.expires_at_ms)?;
        let row = user_subscription::ActiveModel {
            id: Set(id.clone()),
            user_id: Set(self.user(&write.user_id).await?),
            plan_id: Set(self.plan(&write.plan_id).await?),
            enabled: Set(write.enabled.unwrap_or(true)),
            created_at_ms: Set(now),
            starts_at_ms: Set(starts_at_ms),
            expires_at_ms: Set(write.expires_at_ms),
        };
        Ok((row, id))
    }

    async fn change(
        &self,
        current: &user_subscription::Model,
        patch: SubscriptionPatch,
    ) -> Result<user_subscription::ActiveModel> {
        // Both ends are resolved against the row the patch will produce, not
        // against the half it mentioned: moving only the start past a stored
        // expiry is exactly the inversion this refuses.
        let starts_at_ms = patch.starts_at_ms.unwrap_or(current.starts_at_ms);
        let expires_at_ms = patch.expires_at_ms.unwrap_or(current.expires_at_ms);
        crud::ordered_range(Some(starts_at_ms), expires_at_ms)?;
        let mut row = user_subscription::ActiveModel {
            id: Set(current.id.clone()),
            ..Default::default()
        };
        if let Some(plan_id) = patch.plan_id {
            row.plan_id = Set(self.plan(&plan_id).await?);
        }
        if let Some(enabled) = patch.enabled {
            row.enabled = Set(enabled);
        }
        if patch.starts_at_ms.is_some() {
            row.starts_at_ms = Set(starts_at_ms);
        }
        if patch.expires_at_ms.is_some() {
            row.expires_at_ms = Set(expires_at_ms);
        }
        Ok(row)
    }
}
