//! The model catalog: global model metadata, and what each provider calls it.

use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::{
    Repository,
    entity::{
        pricing::price_rule,
        upstream::{model, provider_model},
    },
};
use sea_orm::{ColumnTrait, Condition, EntityTrait, QueryFilter, Select, Set};

use super::{
    Scope, Writer,
    crud::{self, Shape},
};
use crate::{
    SdkResult,
    dto::{
        BatchItem, ListQuery, ModelDto, ModelPatch, ModelWrite, Page, ProviderModelDto,
        ProviderModelPatch, ProviderModelWrite,
    },
};

/// Global model metadata. Routing can name a model that is not in here; the
/// catalog exists for vocabularies, pricing patterns and presentation.
pub struct Models<'a, C> {
    pub(super) writer: Writer<'a, C>,
}

impl<'a, C> Models<'a, C> {
    pub(crate) fn new(writer: Writer<'a, C>) -> Self {
        Self { writer }
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Models<'_, C> {
    pub async fn list(&self, query: ListQuery) -> SdkResult<Page<ModelDto>> {
        self.page(query).await
    }
    pub async fn get(&self, id: &str) -> SdkResult<ModelDto> {
        crud::get(self, id).await
    }
    pub async fn create(&self, write: ModelWrite) -> SdkResult<ModelDto> {
        crud::create(self, write).await
    }
    pub async fn update(&self, id: &str, patch: ModelPatch) -> SdkResult<ModelDto> {
        crud::update(self, id, patch).await
    }
    pub async fn delete(&self, id: &str) -> SdkResult<()> {
        crud::delete(self, id).await
    }
    pub async fn batch(
        &self,
        items: Vec<BatchItem<ModelWrite, ModelPatch>>,
    ) -> SdkResult<Vec<Option<ModelDto>>> {
        crud::batch(self, items).await
    }

    async fn name(&self, name: &str, exclude: Option<&str>) -> SdkResult<String> {
        let name = crud::text(name, "name")?;
        crud::unique(
            self.writer.store().models(),
            Condition::all().add(model::Column::Name.eq(&name)),
            exclude,
            || format!("a model named `{name}` already exists"),
        )
        .await?;
        Ok(name)
    }

    async fn vocabulary(&self, id: Option<String>) -> SdkResult<Option<String>> {
        let Some(id) = crud::optional_text(id) else {
            return Ok(None);
        };
        crud::require_rows(
            self.writer.store().file_objects(),
            "file object",
            std::slice::from_ref(&id),
        )
        .await?;
        Ok(Some(id))
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Shape<C> for Models<'_, C> {
    type Entity = model::Entity;
    type Dto = ModelDto;
    type Write = ModelWrite;
    type Patch = ModelPatch;

    const ENTITY: &'static str = "model";

    fn writer(&self) -> Writer<'_, C> {
        self.writer
    }
    fn repository(&self) -> Repository<'_, C, Self::Entity> {
        self.writer.store().models()
    }
    fn scopes(&self) -> Vec<Scope> {
        vec![Scope::Models]
    }
    fn select(&self, query: &ListQuery) -> Select<Self::Entity> {
        let mut select = model::Entity::find();
        if let Some(search) = crud::optional_text(query.search.clone()) {
            select = select.filter(model::Column::Name.contains(&search));
        }
        select
    }

    async fn build(&self, write: ModelWrite) -> SdkResult<(model::ActiveModel, String)> {
        let id = crud::id_or_new(write.id.as_deref());
        let row = model::ActiveModel {
            id: Set(id.clone()),
            name: Set(self.name(&write.name, None).await?),
            metadata: Set(crud::object(write.metadata, "metadata")?),
            vocabulary_file_id: Set(self.vocabulary(write.vocabulary_file_id).await?),
        };
        Ok((row, id))
    }

    async fn change(
        &self,
        current: &model::Model,
        patch: ModelPatch,
    ) -> SdkResult<model::ActiveModel> {
        let mut row = model::ActiveModel {
            id: Set(current.id.clone()),
            ..Default::default()
        };
        if let Some(name) = patch.name {
            row.name = Set(self.name(&name, Some(&current.id)).await?);
        }
        if let Some(metadata) = patch.metadata {
            row.metadata = Set(crud::object(Some(metadata), "metadata")?);
        }
        if let Some(vocabulary) = patch.vocabulary_file_id {
            row.vocabulary_file_id = Set(self.vocabulary(vocabulary).await?);
        }
        Ok(row)
    }
}

/// What one provider's upstream answers to, optionally mapped to a catalog
/// entry. `(provider_id, upstream_name)` is unique: one provider cannot offer
/// the same upstream model twice.
pub struct ProviderModels<'a, C> {
    writer: Writer<'a, C>,
}

impl<'a, C> ProviderModels<'a, C> {
    pub(crate) fn new(writer: Writer<'a, C>) -> Self {
        Self { writer }
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> ProviderModels<'_, C> {
    pub async fn list(&self, query: ListQuery) -> SdkResult<Page<ProviderModelDto>> {
        let mut page = crud::list(self, query).await?;
        if !page.items.is_empty() {
            let counts = self
                .writer
                .store()
                .price_rules()
                .count_many(
                    page.items
                        .iter()
                        .map(|row| {
                            price_rule::Entity::find()
                                .filter(price_rule::Column::ProviderId.eq(&row.provider_id))
                                .filter(price_rule::Column::ModelPattern.eq(&row.upstream_name))
                                .filter(price_rule::Column::Enabled.eq(true))
                        })
                        .collect(),
                )
                .await?;
            for (row, count) in page.items.iter_mut().zip(counts) {
                row.has_price = Some(count > 0);
            }
        }
        Ok(page)
    }
    pub async fn get(&self, id: &str) -> SdkResult<ProviderModelDto> {
        crud::get(self, id).await
    }
    pub async fn create(&self, write: ProviderModelWrite) -> SdkResult<ProviderModelDto> {
        crud::create(self, write).await
    }
    pub async fn update(&self, id: &str, patch: ProviderModelPatch) -> SdkResult<ProviderModelDto> {
        crud::update(self, id, patch).await
    }
    pub async fn delete(&self, id: &str) -> SdkResult<()> {
        crud::delete(self, id).await
    }
    pub async fn batch(
        &self,
        items: Vec<BatchItem<ProviderModelWrite, ProviderModelPatch>>,
    ) -> SdkResult<Vec<Option<ProviderModelDto>>> {
        crud::batch(self, items).await
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

    async fn catalog_model(&self, id: Option<String>) -> SdkResult<Option<String>> {
        let Some(id) = crud::optional_text(id) else {
            return Ok(None);
        };
        crud::require_rows(
            self.writer.store().models(),
            "model",
            std::slice::from_ref(&id),
        )
        .await?;
        Ok(Some(id))
    }

    async fn metadata(
        &self,
        provider_id: &str,
        name: &str,
        metadata: serde_json::Value,
        exclude: Option<&str>,
    ) -> SdkResult<serde_json::Value> {
        let mut names = std::collections::HashSet::from([name.to_owned()]);
        if let Some(variants) = metadata.get("variants") {
            let variants = variants
                .as_array()
                .ok_or_else(|| crate::SdkError::invalid("variants must be an array of names"))?;
            for value in variants {
                let alias = value
                    .as_str()
                    .filter(|s| !s.trim().is_empty() && s.trim() == *s && !s.contains(['*', '?']))
                    .ok_or_else(|| {
                        crate::SdkError::invalid("variant names must be nonempty literal names")
                    })?;
                if !names.insert(alias.to_owned()) {
                    return Err(crate::SdkError::invalid(
                        "duplicate variant or base model name",
                    ));
                }
            }
        }
        if metadata.get("expose_base").is_some_and(|v| !v.is_boolean()) {
            return Err(crate::SdkError::invalid("expose_base must be boolean"));
        }
        for key in [
            "context_window",
            "max_context_window",
            "max_output_tokens",
            "auto_compact_token_limit",
            "truncation_limit",
        ] {
            if metadata
                .get(key)
                .is_some_and(|v| !v.is_null() && v.as_u64().is_none())
            {
                return Err(crate::SdkError::invalid(format!(
                    "{key} must be a nonnegative integer"
                )));
            }
        }
        for row in self
            .writer
            .store()
            .provider_models()
            .query(
                provider_model::Entity::find()
                    .filter(provider_model::Column::ProviderId.eq(provider_id)),
            )
            .await?
        {
            if Some(row.id.as_str()) == exclude {
                continue;
            }
            if names.contains(&row.upstream_name)
                || row
                    .variant_names()
                    .iter()
                    .any(|alias| names.contains(*alias))
            {
                return Err(crate::SdkError::invalid(
                    "model or variant name is already used by this provider",
                ));
            }
        }
        Ok(metadata)
    }

    async fn unique_pair(
        &self,
        provider_id: &str,
        upstream_name: &str,
        exclude: Option<&str>,
    ) -> SdkResult<()> {
        crud::unique(
            self.writer.store().provider_models(),
            Condition::all()
                .add(provider_model::Column::ProviderId.eq(provider_id))
                .add(provider_model::Column::UpstreamName.eq(upstream_name)),
            exclude,
            || format!("provider `{provider_id}` already offers upstream model `{upstream_name}`"),
        )
        .await
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Shape<C> for ProviderModels<'_, C> {
    type Entity = provider_model::Entity;
    type Dto = ProviderModelDto;
    type Write = ProviderModelWrite;
    type Patch = ProviderModelPatch;

    const ENTITY: &'static str = "provider model";

    fn writer(&self) -> Writer<'_, C> {
        self.writer
    }
    fn repository(&self) -> Repository<'_, C, Self::Entity> {
        self.writer.store().provider_models()
    }
    fn scopes(&self) -> Vec<Scope> {
        vec![Scope::Models]
    }
    fn select(&self, query: &ListQuery) -> Select<Self::Entity> {
        let mut select = provider_model::Entity::find();
        if let Some(provider_id) = crud::optional_text(query.provider_id.clone()) {
            select = select.filter(provider_model::Column::ProviderId.eq(provider_id));
        }
        if let Some(model_id) = crud::optional_text(query.model_id.clone()) {
            select = select.filter(provider_model::Column::ModelId.eq(model_id));
        }
        if let Some(search) = crud::optional_text(query.search.clone()) {
            use sea_orm::sea_query::{Expr, ExprTrait, Func};
            let display_name = match self.writer.backend() {
                sea_orm::DbBackend::Postgres => Expr::cust_with_expr(
                    "$1 ->> 'display_name'",
                    Expr::col(provider_model::Column::Metadata),
                ),
                sea_orm::DbBackend::MySql => Expr::cust_with_expr(
                    "JSON_UNQUOTE(JSON_EXTRACT(?, '$.display_name'))",
                    Expr::col(provider_model::Column::Metadata),
                ),
                _ => Expr::cust_with_expr(
                    "json_extract(?, '$.display_name')",
                    Expr::col(provider_model::Column::Metadata),
                ),
            };
            select = select.filter(
                Condition::any()
                    .add(provider_model::Column::UpstreamName.contains(&search))
                    .add(
                        Expr::expr(Func::lower(display_name))
                            .like(format!("%{}%", search.to_lowercase())),
                    ),
            );
        }
        if let Some(enabled) = query.enabled {
            select = select.filter(provider_model::Column::Enabled.eq(enabled));
        }
        select
    }

    async fn build(
        &self,
        write: ProviderModelWrite,
    ) -> SdkResult<(provider_model::ActiveModel, String)> {
        let id = crud::id_or_new(write.id.as_deref());
        let provider_id = self.provider(&write.provider_id).await?;
        let upstream_name = crud::text(&write.upstream_name, "upstreamName")?;
        self.unique_pair(&provider_id, &upstream_name, None).await?;
        let metadata = self
            .metadata(
                &provider_id,
                &upstream_name,
                crud::object(write.metadata, "metadata")?,
                None,
            )
            .await?;
        let row = provider_model::ActiveModel {
            id: Set(id.clone()),
            provider_id: Set(provider_id),
            upstream_name: Set(upstream_name),
            model_id: Set(self.catalog_model(write.model_id).await?),
            metadata: Set(metadata),
            enabled: Set(write.enabled.unwrap_or(true)),
        };
        Ok((row, id))
    }

    async fn change(
        &self,
        current: &provider_model::Model,
        patch: ProviderModelPatch,
    ) -> SdkResult<provider_model::ActiveModel> {
        let mut row = provider_model::ActiveModel {
            id: Set(current.id.clone()),
            ..Default::default()
        };
        let provider_id = match patch.provider_id {
            Some(provider_id) => {
                let provider_id = self.provider(&provider_id).await?;
                row.provider_id = Set(provider_id.clone());
                provider_id
            }
            None => current.provider_id.clone(),
        };
        let upstream_name = match patch.upstream_name {
            Some(name) => {
                let name = crud::text(&name, "upstreamName")?;
                row.upstream_name = Set(name.clone());
                name
            }
            None => current.upstream_name.clone(),
        };
        if provider_id != current.provider_id || upstream_name != current.upstream_name {
            self.unique_pair(&provider_id, &upstream_name, Some(&current.id))
                .await?;
        }
        if let Some(model_id) = patch.model_id {
            row.model_id = Set(self.catalog_model(model_id).await?);
        }
        let metadata = crud::object(
            Some(patch.metadata.unwrap_or_else(|| current.metadata.clone())),
            "metadata",
        )?;
        row.metadata = Set(self
            .metadata(&provider_id, &upstream_name, metadata, Some(&current.id))
            .await?);
        if let Some(enabled) = patch.enabled {
            row.enabled = Set(enabled);
        }
        Ok(row)
    }
}
