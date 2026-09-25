//! Routing: named provider pools, their members, and the public names that
//! select them.

use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::{
    Repository,
    entity::routing::{exposed_model, route, route_member},
};
use sea_orm::{ColumnTrait, Condition, EntityTrait, QueryFilter, Select, Set};

use super::{
    Scope, Writer,
    crud::{self, Shape},
};
use crate::{
    SdkError, SdkResult,
    dto::{
        BatchItem, ExposedModelDto, ExposedModelPatch, ExposedModelWrite, ListQuery, Page,
        RouteDto, RouteMemberDto, RouteMemberPatch, RouteMemberWrite, RoutePatch, RouteWrite,
    },
};

const STRATEGIES: [&str; 3] = ["round_robin", "weighted", "failover"];

pub struct Routes<'a, C> {
    writer: Writer<'a, C>,
}

impl<'a, C> Routes<'a, C> {
    pub(crate) fn new(writer: Writer<'a, C>) -> Self {
        Self { writer }
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Routes<'_, C> {
    pub async fn list(&self, query: ListQuery) -> SdkResult<Page<RouteDto>> {
        crud::list(self, query).await
    }
    pub async fn get(&self, id: &str) -> SdkResult<RouteDto> {
        crud::get(self, id).await
    }
    pub async fn create(&self, write: RouteWrite) -> SdkResult<RouteDto> {
        crud::create(self, write).await
    }
    pub async fn update(&self, id: &str, patch: RoutePatch) -> SdkResult<RouteDto> {
        crud::update(self, id, patch).await
    }
    pub async fn delete(&self, id: &str) -> SdkResult<()> {
        crud::delete(self, id).await
    }
    pub async fn batch(
        &self,
        items: Vec<BatchItem<RouteWrite, RoutePatch>>,
    ) -> SdkResult<Vec<Option<RouteDto>>> {
        crud::batch(self, items).await
    }

    async fn name(&self, name: &str, exclude: Option<&str>) -> SdkResult<String> {
        let name = crud::text(name, "name")?;
        crud::unique(
            self.writer.store().routes(),
            Condition::all().add(route::Column::Name.eq(&name)),
            exclude,
            || format!("a route named `{name}` already exists"),
        )
        .await?;
        Ok(name)
    }
}

/// A route's attempt budget bounds a whole call, so zero would mean "never
/// send anything".
fn attempts(value: u32) -> SdkResult<u32> {
    if value == 0 {
        return Err(SdkError::invalid("maxAttempts must be positive"));
    }
    Ok(value)
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Shape<C> for Routes<'_, C> {
    type Entity = route::Entity;
    type Dto = RouteDto;
    type Write = RouteWrite;
    type Patch = RoutePatch;

    const ENTITY: &'static str = "route";

    fn writer(&self) -> Writer<'_, C> {
        self.writer
    }
    fn repository(&self) -> Repository<'_, C, Self::Entity> {
        self.writer.store().routes()
    }
    fn scopes(&self) -> Vec<Scope> {
        vec![Scope::Routing]
    }
    fn select(&self, query: &ListQuery) -> Select<Self::Entity> {
        let mut select = route::Entity::find();
        if let Some(search) = crud::optional_text(query.search.clone()) {
            select = select.filter(route::Column::Name.contains(&search));
        }
        if let Some(enabled) = query.enabled {
            select = select.filter(route::Column::Enabled.eq(enabled));
        }
        select
    }

    async fn build(&self, write: RouteWrite) -> SdkResult<(route::ActiveModel, String)> {
        let id = crud::id_or_new(write.id.as_deref());
        let row = route::ActiveModel {
            id: Set(id.clone()),
            name: Set(self.name(&write.name, None).await?),
            strategy: Set(crud::enumerated(
                write.strategy.as_deref().unwrap_or("round_robin"),
                "strategy",
                &STRATEGIES,
            )?),
            session_affinity: Set(write.session_affinity.unwrap_or(false)),
            max_attempts: Set(attempts(write.max_attempts.unwrap_or(6))?),
            enabled: Set(write.enabled.unwrap_or(true)),
        };
        Ok((row, id))
    }

    async fn change(
        &self,
        current: &route::Model,
        patch: RoutePatch,
    ) -> SdkResult<route::ActiveModel> {
        let mut row = route::ActiveModel {
            id: Set(current.id.clone()),
            ..Default::default()
        };
        if let Some(name) = patch.name {
            row.name = Set(self.name(&name, Some(&current.id)).await?);
        }
        if let Some(strategy) = patch.strategy {
            row.strategy = Set(crud::enumerated(&strategy, "strategy", &STRATEGIES)?);
        }
        if let Some(session_affinity) = patch.session_affinity {
            row.session_affinity = Set(session_affinity);
        }
        if let Some(max_attempts) = patch.max_attempts {
            row.max_attempts = Set(attempts(max_attempts)?);
        }
        if let Some(enabled) = patch.enabled {
            row.enabled = Set(enabled);
        }
        Ok(row)
    }
}

pub struct RouteMembers<'a, C> {
    writer: Writer<'a, C>,
}

impl<'a, C> RouteMembers<'a, C> {
    pub(crate) fn new(writer: Writer<'a, C>) -> Self {
        Self { writer }
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> RouteMembers<'_, C> {
    pub async fn list(&self, query: ListQuery) -> SdkResult<Page<RouteMemberDto>> {
        crud::list(self, query).await
    }
    pub async fn get(&self, id: &str) -> SdkResult<RouteMemberDto> {
        crud::get(self, id).await
    }
    pub async fn create(&self, write: RouteMemberWrite) -> SdkResult<RouteMemberDto> {
        crud::create(self, write).await
    }
    pub async fn update(&self, id: &str, patch: RouteMemberPatch) -> SdkResult<RouteMemberDto> {
        crud::update(self, id, patch).await
    }
    pub async fn delete(&self, id: &str) -> SdkResult<()> {
        crud::delete(self, id).await
    }
    pub async fn batch(
        &self,
        items: Vec<BatchItem<RouteMemberWrite, RouteMemberPatch>>,
    ) -> SdkResult<Vec<Option<RouteMemberDto>>> {
        crud::batch(self, items).await
    }

    async fn route(&self, id: &str) -> SdkResult<String> {
        let id = crud::text(id, "routeId")?;
        crud::require_rows(
            self.writer.store().routes(),
            "route",
            std::slice::from_ref(&id),
        )
        .await?;
        Ok(id)
    }

    async fn provider(&self, id: &str) -> SdkResult<String> {
        let id = crud::text(id, "providerId")?;
        crud::require_rows(
            self.writer.store().providers(),
            "provider",
            std::slice::from_ref(&id),
        )
        .await?;
        Ok(id)
    }
}

/// A zero weight would leave the member in the pool while never being picked
/// by the weighted strategy, which reads as a silent outage.
fn weight(value: u32) -> SdkResult<u32> {
    if value == 0 {
        return Err(SdkError::invalid(
            "weight must be positive; disable the member instead",
        ));
    }
    Ok(value)
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Shape<C> for RouteMembers<'_, C> {
    type Entity = route_member::Entity;
    type Dto = RouteMemberDto;
    type Write = RouteMemberWrite;
    type Patch = RouteMemberPatch;

    const ENTITY: &'static str = "route member";

    fn writer(&self) -> Writer<'_, C> {
        self.writer
    }
    fn repository(&self) -> Repository<'_, C, Self::Entity> {
        self.writer.store().route_members()
    }
    fn scopes(&self) -> Vec<Scope> {
        vec![Scope::Routing]
    }
    fn select(&self, query: &ListQuery) -> Select<Self::Entity> {
        let mut select = route_member::Entity::find();
        if let Some(route_id) = crud::optional_text(query.route_id.clone()) {
            select = select.filter(route_member::Column::RouteId.eq(route_id));
        }
        if let Some(provider_id) = crud::optional_text(query.provider_id.clone()) {
            select = select.filter(route_member::Column::ProviderId.eq(provider_id));
        }
        if let Some(enabled) = query.enabled {
            select = select.filter(route_member::Column::Enabled.eq(enabled));
        }
        select
    }

    async fn build(
        &self,
        write: RouteMemberWrite,
    ) -> SdkResult<(route_member::ActiveModel, String)> {
        let id = crud::id_or_new(write.id.as_deref());
        let row = route_member::ActiveModel {
            id: Set(id.clone()),
            route_id: Set(self.route(&write.route_id).await?),
            provider_id: Set(self.provider(&write.provider_id).await?),
            upstream_model: Set(crud::text(&write.upstream_model, "upstreamModel")?),
            tier: Set(write.tier.unwrap_or(0)),
            weight: Set(weight(write.weight.unwrap_or(100))?),
            enabled: Set(write.enabled.unwrap_or(true)),
        };
        Ok((row, id))
    }

    async fn change(
        &self,
        current: &route_member::Model,
        patch: RouteMemberPatch,
    ) -> SdkResult<route_member::ActiveModel> {
        let mut row = route_member::ActiveModel {
            id: Set(current.id.clone()),
            ..Default::default()
        };
        if let Some(route_id) = patch.route_id {
            row.route_id = Set(self.route(&route_id).await?);
        }
        if let Some(provider_id) = patch.provider_id {
            row.provider_id = Set(self.provider(&provider_id).await?);
        }
        if let Some(upstream_model) = patch.upstream_model {
            row.upstream_model = Set(crud::text(&upstream_model, "upstreamModel")?);
        }
        if let Some(tier) = patch.tier {
            row.tier = Set(tier);
        }
        if let Some(value) = patch.weight {
            row.weight = Set(weight(value)?);
        }
        if let Some(enabled) = patch.enabled {
            row.enabled = Set(enabled);
        }
        Ok(row)
    }
}

pub struct ExposedModels<'a, C> {
    writer: Writer<'a, C>,
}

impl<'a, C> ExposedModels<'a, C> {
    pub(crate) fn new(writer: Writer<'a, C>) -> Self {
        Self { writer }
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> ExposedModels<'_, C> {
    pub async fn list(&self, query: ListQuery) -> SdkResult<Page<ExposedModelDto>> {
        crud::list(self, query).await
    }
    pub async fn get(&self, id: &str) -> SdkResult<ExposedModelDto> {
        crud::get(self, id).await
    }
    pub async fn create(&self, write: ExposedModelWrite) -> SdkResult<ExposedModelDto> {
        crud::create(self, write).await
    }
    pub async fn update(&self, id: &str, patch: ExposedModelPatch) -> SdkResult<ExposedModelDto> {
        crud::update(self, id, patch).await
    }
    pub async fn delete(&self, id: &str) -> SdkResult<()> {
        crud::delete(self, id).await
    }
    pub async fn batch(
        &self,
        items: Vec<BatchItem<ExposedModelWrite, ExposedModelPatch>>,
    ) -> SdkResult<Vec<Option<ExposedModelDto>>> {
        crud::batch(self, items).await
    }

    async fn route(&self, id: &str) -> SdkResult<String> {
        let id = crud::text(id, "routeId")?;
        crud::require_rows(
            self.writer.store().routes(),
            "route",
            std::slice::from_ref(&id),
        )
        .await?;
        Ok(id)
    }

    /// Unique, and not shadowed by a narrowing prefix. `channel/model` and
    /// `provider/model` are how a caller picks a specific upstream without a
    /// route; a public name whose first segment is a channel id or a provider
    /// name would be resolved as one of those and never reach its route.
    async fn name(&self, name: &str, exclude: Option<&str>) -> SdkResult<String> {
        let name = crud::text(name, "name")?;
        if let Some((prefix, rest)) = name.split_once('/')
            && !rest.is_empty()
        {
            let reserved = crud::reserved_prefixes(self.writer).await?;
            if reserved.contains(&prefix.to_ascii_lowercase()) {
                return Err(SdkError::invalid(format!(
                    "`{prefix}/` is reserved: a first segment naming a channel or a provider \
                     already means `channel/model` or `provider/model` narrowing, so \
                     `{name}` could never reach its route"
                )));
            }
        }
        crud::unique(
            self.writer.store().exposed_models(),
            Condition::all().add(exposed_model::Column::Name.eq(&name)),
            exclude,
            || format!("`{name}` is already exposed"),
        )
        .await?;
        Ok(name)
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Shape<C> for ExposedModels<'_, C> {
    type Entity = exposed_model::Entity;
    type Dto = ExposedModelDto;
    type Write = ExposedModelWrite;
    type Patch = ExposedModelPatch;

    const ENTITY: &'static str = "exposed model";

    fn writer(&self) -> Writer<'_, C> {
        self.writer
    }
    fn repository(&self) -> Repository<'_, C, Self::Entity> {
        self.writer.store().exposed_models()
    }
    fn scopes(&self) -> Vec<Scope> {
        vec![Scope::Routing]
    }
    fn select(&self, query: &ListQuery) -> Select<Self::Entity> {
        let mut select = exposed_model::Entity::find();
        if let Some(route_id) = crud::optional_text(query.route_id.clone()) {
            select = select.filter(exposed_model::Column::RouteId.eq(route_id));
        }
        if let Some(search) = crud::optional_text(query.search.clone()) {
            select = select.filter(exposed_model::Column::Name.contains(&search));
        }
        if let Some(enabled) = query.enabled {
            select = select.filter(exposed_model::Column::Enabled.eq(enabled));
        }
        select
    }

    async fn build(
        &self,
        write: ExposedModelWrite,
    ) -> SdkResult<(exposed_model::ActiveModel, String)> {
        let id = crud::id_or_new(write.id.as_deref());
        let row = exposed_model::ActiveModel {
            id: Set(id.clone()),
            name: Set(self.name(&write.name, None).await?),
            route_id: Set(self.route(&write.route_id).await?),
            enabled: Set(write.enabled.unwrap_or(true)),
        };
        Ok((row, id))
    }

    async fn change(
        &self,
        current: &exposed_model::Model,
        patch: ExposedModelPatch,
    ) -> SdkResult<exposed_model::ActiveModel> {
        let mut row = exposed_model::ActiveModel {
            id: Set(current.id.clone()),
            ..Default::default()
        };
        if let Some(name) = patch.name {
            row.name = Set(self.name(&name, Some(&current.id)).await?);
        }
        if let Some(route_id) = patch.route_id {
            row.route_id = Set(self.route(&route_id).await?);
        }
        if let Some(enabled) = patch.enabled {
            row.enabled = Set(enabled);
        }
        Ok(row)
    }
}
