//! Per-provider operation overrides: remapped operations and per-method URLs.

use gproxy_core::convert::{
    Route, RoutingMappings, conversion_targets, default_route, local_supported, resolve_route,
    validate_mapping,
};
use gproxy_protocol::{Dialect, Operation};
use gproxy_protocol::{OperationKey, spec::OPERATION_SPECS};
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
        OperationRoutingDto, OperationRuleDto, OperationRulePatch, OperationRuleWrite, Page,
        RoutingMappingDto, RoutingMappingWrite, RoutingTargetDto,
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
    pub async fn effective(&self, provider_id: &str) -> SdkResult<Vec<OperationRoutingDto>> {
        let provider = self
            .writer
            .store()
            .providers()
            .get_many(&[provider_id.to_owned()])
            .await?
            .into_iter()
            .next()
            .flatten()
            .ok_or_else(|| SdkError::not_found("provider", provider_id))?;
        let channel = self
            .writer
            .core()
            .channels()
            .get(&provider.channel)
            .ok_or_else(|| SdkError::invalid(format!("unknown channel `{}`", provider.channel)))?;
        let view = gproxy_channel::channel::ProviderView {
            id: &provider.id,
            channel: &provider.channel,
            base_url: provider.base_url.as_deref(),
            config: &provider.config,
        };
        let saved = self
            .writer
            .store()
            .operation_rules()
            .query(
                operation_rule::Entity::find()
                    .filter(operation_rule::Column::ProviderId.eq(provider_id)),
            )
            .await?;
        let mut rows = Vec::new();
        for spec in OPERATION_SPECS {
            let key = spec.key;
            let rule = saved
                .iter()
                .find(|rule| rule.operation == key.operation.id());
            let custom = match rule {
                Some(rule) if rule.action == "routing" => rule
                    .target
                    .as_ref()
                    .and_then(|v| v.get(key.dialect.id()))
                    .is_some(),
                Some(_) => true,
                None => false,
            };
            let mapping = resolve_route(channel.as_ref(), view, rule, key)
                .map_err(|e| SdkError::invalid(e.to_string()))?;
            rows.push(OperationRoutingDto {
                operation: key.operation.id().into(),
                dialect: key.dialect.id().into(),
                default_mapping: mapping_dto(default_route(channel.as_ref(), view, key)),
                mapping: mapping_dto(mapping),
                custom,
                local_available: local_supported(key),
                targets: conversion_targets(key)
                    .into_iter()
                    .map(target_dto)
                    .collect(),
            });
        }
        Ok(rows)
    }

    pub async fn set_mapping(
        &self,
        provider_id: &str,
        source_operation: &str,
        source_dialect: &str,
        write: RoutingMappingWrite,
    ) -> SdkResult<Vec<OperationRoutingDto>> {
        let source = operation_key(source_operation, source_dialect)?;
        let mapping = match write.implementation.as_str() {
            "passthrough" if write.target.is_none() => Route::Passthrough,
            "local" if write.target.is_none() => Route::Local,
            "unsupported" if write.target.is_none() => Route::Unsupported,
            "transform_to" => {
                let target = write
                    .target
                    .ok_or_else(|| SdkError::invalid("a conversion needs a target"))?;
                Route::TransformTo {
                    target: operation_key(&target.operation, &target.dialect)?,
                }
            }
            _ => {
                return Err(SdkError::invalid(
                    "invalid routing implementation or target",
                ));
            }
        };
        validate_mapping(source, mapping).map_err(SdkError::invalid)?;
        self.save_mapping(provider_id, source, Some(mapping))
            .await?;
        self.effective(provider_id).await
    }

    pub async fn reset_mapping(
        &self,
        provider_id: &str,
        source_operation: &str,
        source_dialect: &str,
    ) -> SdkResult<Vec<OperationRoutingDto>> {
        self.save_mapping(
            provider_id,
            operation_key(source_operation, source_dialect)?,
            None,
        )
        .await?;
        self.effective(provider_id).await
    }

    async fn save_mapping(
        &self,
        provider_id: &str,
        source: OperationKey,
        mapping: Option<Route>,
    ) -> SdkResult<()> {
        self.provider(provider_id).await?;
        let current = self
            .writer
            .store()
            .operation_rules()
            .query(
                operation_rule::Entity::find()
                    .filter(operation_rule::Column::ProviderId.eq(provider_id))
                    .filter(operation_rule::Column::Operation.eq(source.operation.id())),
            )
            .await?
            .into_iter()
            .next();
        let mut mappings: RoutingMappings = match &current {
            Some(row) if row.action == "routing" => {
                serde_json::from_value(row.target.clone().unwrap_or(serde_json::json!({})))
                    .map_err(|e| SdkError::invalid(e.to_string()))?
            }
            Some(_) => self
                .effective(provider_id)
                .await?
                .into_iter()
                .filter(|r| r.operation == source.operation.id())
                .map(|r| {
                    Ok((
                        r.dialect
                            .parse()
                            .map_err(|_| SdkError::invalid("invalid source protocol"))?,
                        dto_mapping(r.mapping)?,
                    ))
                })
                .collect::<SdkResult<_>>()?,
            None => Default::default(),
        };
        if let Some(mapping) = mapping {
            mappings.insert(source.dialect, mapping);
        } else {
            mappings.remove(&source.dialect);
        }
        match current {
            Some(row) if mappings.is_empty() => self.delete(&row.id).await?,
            Some(row) => {
                self.update(
                    &row.id,
                    OperationRulePatch {
                        action: Some("routing".into()),
                        target: Some(Some(
                            serde_json::to_value(mappings)
                                .map_err(|e| SdkError::invalid(e.to_string()))?,
                        )),
                        ..Default::default()
                    },
                )
                .await?;
            }
            None if !mappings.is_empty() => {
                self.create(OperationRuleWrite {
                    provider_id: provider_id.into(),
                    operation: source.operation.id().into(),
                    action: "routing".into(),
                    target: Some(
                        serde_json::to_value(mappings)
                            .map_err(|e| SdkError::invalid(e.to_string()))?,
                    ),
                    ..Default::default()
                })
                .await?;
            }
            _ => {}
        }
        Ok(())
    }

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
        validate_routing_config(&operation, &write.action, write.target.as_ref())?;
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
        validate_routing_config(
            patch.operation.as_deref().unwrap_or(&current.operation),
            patch.action.as_deref().unwrap_or(&current.action),
            patch
                .target
                .as_ref()
                .map(|v| v.as_ref())
                .unwrap_or(current.target.as_ref()),
        )?;
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

fn operation_key(operation: &str, dialect: &str) -> SdkResult<OperationKey> {
    Ok(OperationKey {
        operation: operation
            .parse()
            .map_err(|_| SdkError::invalid("unknown operation"))?,
        dialect: dialect
            .parse()
            .map_err(|_| SdkError::invalid("unknown protocol"))?,
    })
}
fn target_dto(key: OperationKey) -> RoutingTargetDto {
    RoutingTargetDto {
        operation: key.operation.id().into(),
        dialect: key.dialect.id().into(),
    }
}
fn mapping_dto(mapping: Route) -> RoutingMappingDto {
    let (implementation, target) = match mapping {
        Route::Passthrough => ("passthrough", None),
        Route::Local => ("local", None),
        Route::Unsupported => ("unsupported", None),
        Route::TransformTo { target } => ("transform_to", Some(target_dto(target))),
    };
    RoutingMappingDto {
        implementation: implementation.into(),
        target,
    }
}
fn dto_mapping(mapping: RoutingMappingDto) -> SdkResult<Route> {
    Ok(match mapping.implementation.as_str() {
        "passthrough" => Route::Passthrough,
        "local" => Route::Local,
        "unsupported" => Route::Unsupported,
        "transform_to" => {
            let t = mapping
                .target
                .ok_or_else(|| SdkError::invalid("missing route target"))?;
            Route::TransformTo {
                target: operation_key(&t.operation, &t.dialect)?,
            }
        }
        _ => return Err(SdkError::invalid("invalid route implementation")),
    })
}

fn validate_routing_config(
    operation: &str,
    action: &str,
    target: Option<&serde_json::Value>,
) -> SdkResult<()> {
    let operation: Operation = operation
        .parse()
        .map_err(|_| SdkError::invalid("unknown operation"))?;
    match action {
        "deny" if target.is_none() => {}
        "routing" => {
            let mappings: RoutingMappings =
                serde_json::from_value(target.cloned().unwrap_or(serde_json::json!({})))
                    .map_err(|e| SdkError::invalid(e.to_string()))?;
            for (dialect, mapping) in mappings {
                validate_mapping(OperationKey { operation, dialect }, mapping)
                    .map_err(SdkError::invalid)?;
            }
        }
        "dialects" => {
            let _: Vec<Dialect> =
                serde_json::from_value(target.cloned().unwrap_or(serde_json::json!([])))
                    .map_err(|e| SdkError::invalid(e.to_string()))?;
        }
        _ => return Err(SdkError::invalid("unknown routing action")),
    }
    Ok(())
}
