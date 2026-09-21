//! Capacity pools and the upstream subscriptions that back them.
//!
//! A pool member points at one credential and carries the canonical
//! `source_key` that identifies the real upstream subscription behind it. Both
//! columns are unique, which is what keeps the same purchased subscription
//! from being counted twice because it was imported under two credentials.

use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::{
    Repository,
    entity::subscription::{pool, pool_member},
};
use sea_orm::{ColumnTrait, Condition, EntityTrait, QueryFilter, Select, Set};

use super::{
    Scope, Writer,
    crud::{self, Shape},
};
use crate::{
    Result,
    dto::{
        BatchItem, ListQuery, Page, PoolDto, PoolMemberDto, PoolMemberPatch, PoolMemberWrite,
        PoolPatch, PoolWrite,
    },
};

pub struct Pools<'a, C> {
    writer: Writer<'a, C>,
}

impl<'a, C> Pools<'a, C> {
    pub(crate) fn new(writer: Writer<'a, C>) -> Self {
        Self { writer }
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Pools<'_, C> {
    pub async fn list(&self, query: ListQuery) -> Result<Page<PoolDto>> {
        crud::list(self, query).await
    }
    pub async fn get(&self, id: &str) -> Result<PoolDto> {
        crud::get(self, id).await
    }
    pub async fn create(&self, write: PoolWrite) -> Result<PoolDto> {
        crud::create(self, write).await
    }
    pub async fn update(&self, id: &str, patch: PoolPatch) -> Result<PoolDto> {
        crud::update(self, id, patch).await
    }
    /// Delete a pool. Its members and plans go with it through foreign keys
    /// that exist on every database — both tables were created with them, so
    /// unlike the key bindings there is nothing for this layer to do by hand.
    pub async fn delete(&self, id: &str) -> Result<()> {
        crud::delete(self, id).await
    }
    pub async fn batch(
        &self,
        items: Vec<BatchItem<PoolWrite, PoolPatch>>,
    ) -> Result<Vec<Option<PoolDto>>> {
        crud::batch(self, items).await
    }

    async fn name(&self, name: &str, exclude: Option<&str>) -> Result<String> {
        let name = crud::text(name, "name")?;
        crud::unique(
            self.writer.store().subscription_pools(),
            Condition::all().add(pool::Column::Name.eq(&name)),
            exclude,
            || format!("a pool named `{name}` already exists"),
        )
        .await?;
        Ok(name)
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Shape<C> for Pools<'_, C> {
    type Entity = pool::Entity;
    type Dto = PoolDto;
    type Write = PoolWrite;
    type Patch = PoolPatch;

    const ENTITY: &'static str = "pool";

    fn writer(&self) -> Writer<'_, C> {
        self.writer
    }
    fn repository(&self) -> Repository<'_, C, Self::Entity> {
        self.writer.store().subscription_pools()
    }
    fn scopes(&self) -> Vec<Scope> {
        vec![Scope::Subscriptions]
    }
    fn select(&self, query: &ListQuery) -> Select<Self::Entity> {
        let mut select = pool::Entity::find();
        if let Some(search) = crud::optional_text(query.search.clone()) {
            select = select.filter(pool::Column::Name.contains(&search));
        }
        if let Some(enabled) = query.enabled {
            select = select.filter(pool::Column::Enabled.eq(enabled));
        }
        select
    }

    async fn build(&self, write: PoolWrite) -> Result<(pool::ActiveModel, String)> {
        let id = crud::id_or_new(write.id.as_deref())?;
        let row = pool::ActiveModel {
            id: Set(id.clone()),
            name: Set(self.name(&write.name, None).await?),
            enabled: Set(write.enabled.unwrap_or(true)),
            created_at_ms: Set(crate::now_ms()),
        };
        Ok((row, id))
    }

    async fn change(&self, current: &pool::Model, patch: PoolPatch) -> Result<pool::ActiveModel> {
        let mut row = pool::ActiveModel {
            id: Set(current.id.clone()),
            ..Default::default()
        };
        if let Some(name) = patch.name {
            row.name = Set(self.name(&name, Some(&current.id)).await?);
        }
        if let Some(enabled) = patch.enabled {
            row.enabled = Set(enabled);
        }
        Ok(row)
    }
}

pub struct PoolMembers<'a, C> {
    writer: Writer<'a, C>,
}

impl<'a, C> PoolMembers<'a, C> {
    pub(crate) fn new(writer: Writer<'a, C>) -> Self {
        Self { writer }
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> PoolMembers<'_, C> {
    pub async fn list(&self, query: ListQuery) -> Result<Page<PoolMemberDto>> {
        crud::list(self, query).await
    }
    pub async fn get(&self, id: &str) -> Result<PoolMemberDto> {
        crud::get(self, id).await
    }
    pub async fn create(&self, write: PoolMemberWrite) -> Result<PoolMemberDto> {
        crud::create(self, write).await
    }
    pub async fn update(&self, id: &str, patch: PoolMemberPatch) -> Result<PoolMemberDto> {
        crud::update(self, id, patch).await
    }
    pub async fn delete(&self, id: &str) -> Result<()> {
        crud::delete(self, id).await
    }
    pub async fn batch(
        &self,
        items: Vec<BatchItem<PoolMemberWrite, PoolMemberPatch>>,
    ) -> Result<Vec<Option<PoolMemberDto>>> {
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

    /// One credential backs at most one pool, and one real upstream
    /// subscription is one member however many credentials reached it.
    async fn credential(&self, id: &str, exclude: Option<&str>) -> Result<String> {
        let id = crud::text(id, "credentialId")?;
        crud::require_rows(
            self.writer.store().credentials(),
            "credential",
            std::slice::from_ref(&id),
        )
        .await?;
        crud::unique(
            self.writer.store().subscription_pool_members(),
            Condition::all().add(pool_member::Column::CredentialId.eq(&id)),
            exclude,
            || format!("credential `{id}` already backs a pool"),
        )
        .await?;
        Ok(id)
    }

    async fn source_key(&self, key: &str, exclude: Option<&str>) -> Result<String> {
        let key = crud::text(key, "sourceKey")?;
        crud::unique(
            self.writer.store().subscription_pool_members(),
            Condition::all().add(pool_member::Column::SourceKey.eq(&key)),
            exclude,
            || format!("upstream subscription `{key}` is already pooled"),
        )
        .await?;
        Ok(key)
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Shape<C> for PoolMembers<'_, C> {
    type Entity = pool_member::Entity;
    type Dto = PoolMemberDto;
    type Write = PoolMemberWrite;
    type Patch = PoolMemberPatch;

    const ENTITY: &'static str = "pool member";

    fn writer(&self) -> Writer<'_, C> {
        self.writer
    }
    fn repository(&self) -> Repository<'_, C, Self::Entity> {
        self.writer.store().subscription_pool_members()
    }
    fn scopes(&self) -> Vec<Scope> {
        vec![Scope::Subscriptions]
    }
    fn select(&self, query: &ListQuery) -> Select<Self::Entity> {
        let mut select = pool_member::Entity::find();
        if let Some(pool_id) = crud::optional_text(query.pool_id.clone()) {
            select = select.filter(pool_member::Column::PoolId.eq(pool_id));
        }
        if let Some(credential_id) = crud::optional_text(query.credential_id.clone()) {
            select = select.filter(pool_member::Column::CredentialId.eq(credential_id));
        }
        if let Some(enabled) = query.enabled {
            select = select.filter(pool_member::Column::Enabled.eq(enabled));
        }
        select
    }

    async fn build(&self, write: PoolMemberWrite) -> Result<(pool_member::ActiveModel, String)> {
        let id = crud::id_or_new(write.id.as_deref())?;
        let row = pool_member::ActiveModel {
            id: Set(id.clone()),
            pool_id: Set(self.pool(&write.pool_id).await?),
            credential_id: Set(self.credential(&write.credential_id, None).await?),
            source_key: Set(self.source_key(&write.source_key, None).await?),
            enabled: Set(write.enabled.unwrap_or(true)),
        };
        Ok((row, id))
    }

    async fn change(
        &self,
        current: &pool_member::Model,
        patch: PoolMemberPatch,
    ) -> Result<pool_member::ActiveModel> {
        let mut row = pool_member::ActiveModel {
            id: Set(current.id.clone()),
            ..Default::default()
        };
        if let Some(pool_id) = patch.pool_id {
            row.pool_id = Set(self.pool(&pool_id).await?);
        }
        if let Some(credential_id) = patch.credential_id {
            row.credential_id = Set(self.credential(&credential_id, Some(&current.id)).await?);
        }
        if let Some(source_key) = patch.source_key {
            row.source_key = Set(self.source_key(&source_key, Some(&current.id)).await?);
        }
        if let Some(enabled) = patch.enabled {
            row.enabled = Set(enabled);
        }
        Ok(row)
    }
}
