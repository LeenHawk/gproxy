//! Providers: one upstream configuration, bound to a channel implementation.

use gproxy_seaorm::{BatchConnectionTrait, BatchStatement};
use gproxy_store::{
    Repository,
    entity::upstream::{operation_endpoint, operation_rule, provider},
};
use sea_orm::{ColumnTrait, Condition, EntityTrait, QueryFilter, Select, Set};

use super::{
    Scope, Writer,
    crud::{self, Shape},
};
use crate::{
    SdkError, SdkResult,
    dto::{BatchItem, ListQuery, Page, ProviderDto, ProviderPatch, ProviderWrite},
};

pub struct Providers<'a, C> {
    writer: Writer<'a, C>,
}

impl<'a, C> Providers<'a, C> {
    pub(crate) fn new(writer: Writer<'a, C>) -> Self {
        Self { writer }
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Providers<'_, C> {
    pub async fn list(&self, query: ListQuery) -> SdkResult<Page<ProviderDto>> {
        crud::list(self, query).await
    }
    pub async fn get(&self, id: &str) -> SdkResult<ProviderDto> {
        crud::get(self, id).await
    }
    pub async fn create(&self, write: ProviderWrite) -> SdkResult<ProviderDto> {
        crud::create(self, write).await
    }
    pub async fn update(&self, id: &str, patch: ProviderPatch) -> SdkResult<ProviderDto> {
        crud::update(self, id, patch).await
    }
    pub async fn delete(&self, id: &str) -> SdkResult<()> {
        crud::delete(self, id).await
    }
    pub async fn batch(
        &self,
        items: Vec<BatchItem<ProviderWrite, ProviderPatch>>,
    ) -> SdkResult<Vec<Option<ProviderDto>>> {
        crud::batch(self, items).await
    }

    /// Drop every operation override this provider carries — remapped
    /// operations and per-method URLs alike — so it falls back to what its
    /// channel does by default. One commit, so the provider is never left
    /// half-overridden.
    pub async fn reset_routing_defaults(&self, provider_id: &str) -> SdkResult<i64> {
        crud::row::<C, Self>(self, provider_id).await?;
        let statements = vec![
            BatchStatement::Execute(
                self.writer
                    .store()
                    .operation_rules()
                    .delete_where_statement(
                        operation_rule::Entity::delete_many()
                            .filter(operation_rule::Column::ProviderId.eq(provider_id)),
                    ),
            ),
            BatchStatement::Execute(
                self.writer
                    .store()
                    .operation_endpoints()
                    .delete_where_statement(
                        operation_endpoint::Entity::delete_many()
                            .filter(operation_endpoint::Column::ProviderId.eq(provider_id)),
                    ),
            ),
        ];
        self.writer.commit(statements, &[Scope::Endpoints]).await
    }

    /// The channel must be one this build registered: a provider naming an
    /// unknown one could never serve a request, and assembly would drop it
    /// without telling anybody.
    fn channel(&self, channel: &str) -> SdkResult<String> {
        let channel = crud::text(channel, "channel")?;
        if self.writer.core().channels().get(&channel).is_none() {
            return Err(SdkError::invalid(format!(
                "channel `{channel}` is not registered in this build"
            )));
        }
        Ok(channel)
    }

    async fn profile(&self, id: Option<String>) -> SdkResult<Option<String>> {
        let Some(id) = crud::optional_text(id) else {
            return Ok(None);
        };
        crud::require_rows(
            self.writer.store().connection_profiles(),
            "connection profile",
            std::slice::from_ref(&id),
        )
        .await?;
        Ok(Some(id))
    }

    async fn name(&self, name: &str, exclude: Option<&str>) -> SdkResult<String> {
        let name = crud::text(name, "name")?;
        crud::unique(
            self.writer.store().providers(),
            Condition::all().add(provider::Column::Name.eq(&name)),
            exclude,
            || format!("a provider named `{name}` already exists"),
        )
        .await?;
        Ok(name)
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Shape<C> for Providers<'_, C> {
    type Entity = provider::Entity;
    type Dto = ProviderDto;
    type Write = ProviderWrite;
    type Patch = ProviderPatch;

    const ENTITY: &'static str = "provider";

    fn writer(&self) -> Writer<'_, C> {
        self.writer
    }
    fn repository(&self) -> Repository<'_, C, Self::Entity> {
        self.writer.store().providers()
    }
    fn scopes(&self) -> Vec<Scope> {
        vec![Scope::Providers]
    }
    fn select(&self, query: &ListQuery) -> Select<Self::Entity> {
        let mut select = provider::Entity::find();
        if let Some(search) = crud::optional_text(query.search.clone()) {
            select = select.filter(provider::Column::Name.contains(&search));
        }
        if let Some(enabled) = query.enabled {
            select = select.filter(provider::Column::Enabled.eq(enabled));
        }
        select
    }

    async fn build(&self, write: ProviderWrite) -> SdkResult<(provider::ActiveModel, String)> {
        let id = crud::id_or_new(write.id.as_deref());
        let base_url = match crud::optional_text(write.base_url) {
            Some(value) => Some(crud::url(&value, "baseUrl")?),
            None => None,
        };
        let model = provider::ActiveModel {
            id: Set(id.clone()),
            name: Set(self.name(&write.name, None).await?),
            channel: Set(self.channel(&write.channel)?),
            base_url: Set(base_url),
            connection_profile_id: Set(self.profile(write.connection_profile_id).await?),
            config: Set(crud::object(write.config, "config")?),
            enabled: Set(write.enabled.unwrap_or(true)),
            created_at_ms: Set(crate::rt::now_ms()),
        };
        Ok((model, id))
    }

    async fn change(
        &self,
        current: &provider::Model,
        patch: ProviderPatch,
    ) -> SdkResult<provider::ActiveModel> {
        let mut model = provider::ActiveModel {
            id: Set(current.id.clone()),
            ..Default::default()
        };
        if let Some(name) = patch.name {
            model.name = Set(self.name(&name, Some(&current.id)).await?);
        }
        if let Some(channel) = patch.channel {
            model.channel = Set(self.channel(&channel)?);
        }
        if let Some(base_url) = patch.base_url {
            model.base_url = Set(match crud::optional_text(base_url) {
                Some(value) => Some(crud::url(&value, "baseUrl")?),
                None => None,
            });
        }
        if let Some(profile) = patch.connection_profile_id {
            model.connection_profile_id = Set(self.profile(profile).await?);
        }
        if let Some(config) = patch.config {
            model.config = Set(crud::object(Some(config), "config")?);
        }
        if let Some(enabled) = patch.enabled {
            model.enabled = Set(enabled);
        }
        Ok(model)
    }
}
