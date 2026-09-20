//! Per-provider operation overrides: remapped operations and per-method URLs.

use gproxy_protocol::{Dialect, Operation};
use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::{
    Repository,
    entity::upstream::{operation_endpoint, operation_rule},
};
use sea_orm::{ColumnTrait, Condition, EntityTrait, QueryFilter, Select, Set};

use super::{
    Scope, Writer,
    crud::{self, Shape},
};
use crate::{
    SdkError, SdkResult,
    dto::{
        BatchItem, ListQuery, OperationEndpointDto, OperationEndpointPatch, OperationEndpointWrite,
        OperationRuleDto, OperationRulePatch, OperationRuleWrite, Page,
    },
};

const TRANSPORTS: [&str; 2] = ["http", "websocket"];

/// Both override tables of a provider. They are one family because they share
/// a scope: changing either changes how a request is addressed.
pub struct Endpoints<'a, C> {
    writer: Writer<'a, C>,
}

impl<'a, C> Endpoints<'a, C> {
    pub(crate) fn new(writer: Writer<'a, C>) -> Self {
        Self { writer }
    }
    /// Operation remapping: what a provider serves an operation with.
    pub fn operation_rules(&self) -> OperationRules<'a, C> {
        OperationRules {
            writer: self.writer,
        }
    }
    /// Per-method URLs: where a provider serves a native operation.
    pub fn operation_endpoints(&self) -> OperationEndpoints<'a, C> {
        OperationEndpoints {
            writer: self.writer,
        }
    }
}

/// The operation must be one the protocol declares; a typo would be stored
/// happily and then never match anything at execution time.
fn operation(value: &str) -> SdkResult<String> {
    let value = crud::text(value, "operation")?;
    value
        .parse::<Operation>()
        .map_err(|_| SdkError::invalid(format!("`{value}` is not a known operation")))?;
    Ok(value)
}

fn dialect(value: &str) -> SdkResult<String> {
    let value = crud::text(value, "dialect")?;
    value
        .parse::<Dialect>()
        .map_err(|_| SdkError::invalid(format!("`{value}` is not a known dialect")))?;
    Ok(value)
}

pub struct OperationRules<'a, C> {
    writer: Writer<'a, C>,
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> OperationRules<'_, C> {
    pub async fn list(&self, query: ListQuery) -> SdkResult<Page<OperationRuleDto>> {
        crud::list(self, query).await
    }
    pub async fn get(&self, id: &str) -> SdkResult<OperationRuleDto> {
        crud::get(self, id).await
    }
    pub async fn create(&self, write: OperationRuleWrite) -> SdkResult<OperationRuleDto> {
        crud::create(self, write).await
    }
    pub async fn update(&self, id: &str, patch: OperationRulePatch) -> SdkResult<OperationRuleDto> {
        crud::update(self, id, patch).await
    }
    pub async fn delete(&self, id: &str) -> SdkResult<()> {
        crud::delete(self, id).await
    }
    pub async fn batch(
        &self,
        items: Vec<BatchItem<OperationRuleWrite, OperationRulePatch>>,
    ) -> SdkResult<Vec<Option<OperationRuleDto>>> {
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

    async fn unique_pair(
        &self,
        provider_id: &str,
        operation: &str,
        exclude: Option<&str>,
    ) -> SdkResult<()> {
        crud::unique(
            self.writer.store().operation_rules(),
            Condition::all()
                .add(operation_rule::Column::ProviderId.eq(provider_id))
                .add(operation_rule::Column::Operation.eq(operation)),
            exclude,
            || format!("provider `{provider_id}` already overrides `{operation}`"),
        )
        .await
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Shape<C> for OperationRules<'_, C> {
    type Entity = operation_rule::Entity;
    type Dto = OperationRuleDto;
    type Write = OperationRuleWrite;
    type Patch = OperationRulePatch;

    const ENTITY: &'static str = "operation rule";

    fn writer(&self) -> Writer<'_, C> {
        self.writer
    }
    fn repository(&self) -> Repository<'_, C, Self::Entity> {
        self.writer.store().operation_rules()
    }
    fn scopes(&self) -> Vec<Scope> {
        vec![Scope::Endpoints]
    }
    fn select(&self, query: &ListQuery) -> Select<Self::Entity> {
        let mut select = operation_rule::Entity::find();
        if let Some(provider_id) = crud::optional_text(query.provider_id.clone()) {
            select = select.filter(operation_rule::Column::ProviderId.eq(provider_id));
        }
        if let Some(search) = crud::optional_text(query.search.clone()) {
            select = select.filter(operation_rule::Column::Operation.contains(&search));
        }
        select
    }

    async fn build(
        &self,
        write: OperationRuleWrite,
    ) -> SdkResult<(operation_rule::ActiveModel, String)> {
        let id = crud::id_or_new(write.id.as_deref());
        let provider_id = self.provider(&write.provider_id).await?;
        let operation = operation(&write.operation)?;
        self.unique_pair(&provider_id, &operation, None).await?;
        let row = operation_rule::ActiveModel {
            id: Set(id.clone()),
            provider_id: Set(provider_id),
            operation: Set(operation),
            action: Set(crud::text(&write.action, "action")?),
            target: Set(write.target),
        };
        Ok((row, id))
    }

    async fn change(
        &self,
        current: &operation_rule::Model,
        patch: OperationRulePatch,
    ) -> SdkResult<operation_rule::ActiveModel> {
        let mut row = operation_rule::ActiveModel {
            id: Set(current.id.clone()),
            ..Default::default()
        };
        if let Some(value) = patch.operation {
            let value = operation(&value)?;
            self.unique_pair(&current.provider_id, &value, Some(&current.id))
                .await?;
            row.operation = Set(value);
        }
        if let Some(action) = patch.action {
            row.action = Set(crud::text(&action, "action")?);
        }
        if let Some(target) = patch.target {
            row.target = Set(target);
        }
        Ok(row)
    }
}

pub struct OperationEndpoints<'a, C> {
    writer: Writer<'a, C>,
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> OperationEndpoints<'_, C> {
    pub async fn list(&self, query: ListQuery) -> SdkResult<Page<OperationEndpointDto>> {
        crud::list(self, query).await
    }
    pub async fn get(&self, id: &str) -> SdkResult<OperationEndpointDto> {
        crud::get(self, id).await
    }
    pub async fn create(&self, write: OperationEndpointWrite) -> SdkResult<OperationEndpointDto> {
        crud::create(self, write).await
    }
    pub async fn update(
        &self,
        id: &str,
        patch: OperationEndpointPatch,
    ) -> SdkResult<OperationEndpointDto> {
        crud::update(self, id, patch).await
    }
    pub async fn delete(&self, id: &str) -> SdkResult<()> {
        crud::delete(self, id).await
    }
    pub async fn batch(
        &self,
        items: Vec<BatchItem<OperationEndpointWrite, OperationEndpointPatch>>,
    ) -> SdkResult<Vec<Option<OperationEndpointDto>>> {
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

    async fn unique_key(
        &self,
        provider_id: &str,
        operation: &str,
        dialect: &str,
        transport: operation_endpoint::EndpointTransport,
        exclude: Option<&str>,
    ) -> SdkResult<()> {
        crud::unique(
            self.writer.store().operation_endpoints(),
            Condition::all()
                .add(operation_endpoint::Column::ProviderId.eq(provider_id))
                .add(operation_endpoint::Column::Operation.eq(operation))
                .add(operation_endpoint::Column::Dialect.eq(dialect))
                .add(operation_endpoint::Column::Transport.eq(transport)),
            exclude,
            || format!("provider `{provider_id}` already has a URL for `{operation}`/`{dialect}`"),
        )
        .await
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Shape<C> for OperationEndpoints<'_, C> {
    type Entity = operation_endpoint::Entity;
    type Dto = OperationEndpointDto;
    type Write = OperationEndpointWrite;
    type Patch = OperationEndpointPatch;

    const ENTITY: &'static str = "operation endpoint";

    fn writer(&self) -> Writer<'_, C> {
        self.writer
    }
    fn repository(&self) -> Repository<'_, C, Self::Entity> {
        self.writer.store().operation_endpoints()
    }
    fn scopes(&self) -> Vec<Scope> {
        vec![Scope::Endpoints]
    }
    fn select(&self, query: &ListQuery) -> Select<Self::Entity> {
        let mut select = operation_endpoint::Entity::find();
        if let Some(provider_id) = crud::optional_text(query.provider_id.clone()) {
            select = select.filter(operation_endpoint::Column::ProviderId.eq(provider_id));
        }
        if let Some(search) = crud::optional_text(query.search.clone()) {
            select = select.filter(operation_endpoint::Column::Operation.contains(&search));
        }
        if let Some(enabled) = query.enabled {
            select = select.filter(operation_endpoint::Column::Enabled.eq(enabled));
        }
        select
    }

    async fn build(
        &self,
        write: OperationEndpointWrite,
    ) -> SdkResult<(operation_endpoint::ActiveModel, String)> {
        let id = crud::id_or_new(write.id.as_deref());
        let provider_id = self.provider(&write.provider_id).await?;
        let operation = operation(&write.operation)?;
        let dialect = dialect(&write.dialect)?;
        let transport: operation_endpoint::EndpointTransport = crud::enumerated(
            write.transport.as_deref().unwrap_or("http"),
            "transport",
            &TRANSPORTS,
        )?;
        self.unique_key(&provider_id, &operation, &dialect, transport, None)
            .await?;
        let row = operation_endpoint::ActiveModel {
            id: Set(id.clone()),
            provider_id: Set(provider_id),
            operation: Set(operation),
            dialect: Set(dialect),
            transport: Set(transport),
            url: Set(crud::url(&write.url, "url")?),
            enabled: Set(write.enabled.unwrap_or(true)),
        };
        Ok((row, id))
    }

    async fn change(
        &self,
        current: &operation_endpoint::Model,
        patch: OperationEndpointPatch,
    ) -> SdkResult<operation_endpoint::ActiveModel> {
        let mut row = operation_endpoint::ActiveModel {
            id: Set(current.id.clone()),
            ..Default::default()
        };
        let operation = match patch.operation {
            Some(value) => {
                let value = operation(&value)?;
                row.operation = Set(value.clone());
                value
            }
            None => current.operation.clone(),
        };
        let dialect = match patch.dialect {
            Some(value) => {
                let value = dialect(&value)?;
                row.dialect = Set(value.clone());
                value
            }
            None => current.dialect.clone(),
        };
        let transport = match patch.transport {
            Some(value) => {
                let value: operation_endpoint::EndpointTransport =
                    crud::enumerated(&value, "transport", &TRANSPORTS)?;
                row.transport = Set(value);
                value
            }
            None => current.transport,
        };
        if operation != current.operation
            || dialect != current.dialect
            || transport != current.transport
        {
            self.unique_key(
                &current.provider_id,
                &operation,
                &dialect,
                transport,
                Some(&current.id),
            )
            .await?;
        }
        if let Some(url) = patch.url {
            row.url = Set(crud::url(&url, "url")?);
        }
        if let Some(enabled) = patch.enabled {
            row.enabled = Set(enabled);
        }
        Ok(row)
    }
}
