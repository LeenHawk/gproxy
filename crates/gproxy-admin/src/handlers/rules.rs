use bytes::Bytes;
use http::{Response, StatusCode};

use crate::dto::*;
use crate::handlers::util;
use crate::route::Entity;
use crate::{AdminError, State, response};

pub(super) async fn list(
    state: &impl State,
    entity: Entity,
) -> Result<Response<Bytes>, AdminError> {
    let snapshot = state.store().control_snapshot().await?;
    match entity {
        Entity::RoutingRules => response::json(
            StatusCode::OK,
            &snapshot
                .routing_rules
                .iter()
                .map(routing_dto)
                .collect::<Result<Vec<_>, _>>()?,
        ),
        Entity::RuleSets => response::json(
            StatusCode::OK,
            &snapshot
                .rule_sets
                .iter()
                .map(|value| RuleSetDto {
                    id: value.id,
                    name: value.name.clone(),
                    description: value.description.clone(),
                    enabled: value.enabled,
                })
                .collect::<Vec<_>>(),
        ),
        Entity::Rules => response::json(
            StatusCode::OK,
            &snapshot
                .rules
                .iter()
                .map(rule_dto)
                .collect::<Result<Vec<_>, _>>()?,
        ),
        Entity::ProviderRuleSets => response::json(
            StatusCode::OK,
            &snapshot
                .provider_rule_sets
                .iter()
                .map(|value| ProviderRuleSetDto {
                    id: value.id,
                    provider_id: value.provider_id,
                    rule_set_id: value.rule_set_id,
                    sort_order: value.sort_order,
                    enabled: value.enabled,
                    inherited: value.origin == "channel_default",
                })
                .collect::<Vec<_>>(),
        ),
        _ => Err(AdminError::NotFound),
    }
}

pub(super) async fn create(
    state: &impl State,
    entity: Entity,
    body: &Bytes,
) -> Result<Response<Bytes>, AdminError> {
    let id = match entity {
        Entity::RoutingRules => {
            state
                .store()
                .insert_routing_rule(&routing_input(state, util::parse(body)?).await?)
                .await?
        }
        Entity::RuleSets => {
            state
                .store()
                .insert_rule_set(&rule_set_input(util::parse(body)?)?)
                .await?
        }
        Entity::Rules => {
            state
                .store()
                .insert_rule(&rule_input(state, util::parse(body)?).await?)
                .await?
        }
        Entity::ProviderRuleSets => {
            state
                .store()
                .insert_provider_rule_set(&attachment_input(state, util::parse(body)?).await?)
                .await?
        }
        _ => return Err(AdminError::NotFound),
    };
    util::created(state, id).await
}

pub(super) async fn update(
    state: &impl State,
    entity: Entity,
    id: i64,
    body: &Bytes,
) -> Result<Response<Bytes>, AdminError> {
    let applied = match entity {
        Entity::RoutingRules => {
            state
                .store()
                .update_routing_rule(id, &routing_input(state, util::parse(body)?).await?)
                .await?
        }
        Entity::RuleSets => {
            state
                .store()
                .update_rule_set(id, &rule_set_input(util::parse(body)?)?)
                .await?
        }
        Entity::Rules => {
            state
                .store()
                .update_rule(id, &rule_input(state, util::parse(body)?).await?)
                .await?
        }
        Entity::ProviderRuleSets => {
            state
                .store()
                .update_provider_rule_set(id, &attachment_input(state, util::parse(body)?).await?)
                .await?
        }
        _ => return Err(AdminError::NotFound),
    };
    util::updated(state, applied).await
}

pub(super) async fn delete(
    state: &impl State,
    entity: Entity,
    id: i64,
) -> Result<Response<Bytes>, AdminError> {
    let applied = match entity {
        Entity::RoutingRules => state.store().delete_routing_rule(id).await?,
        Entity::RuleSets => state.store().delete_rule_set(id).await?,
        Entity::Rules => state.store().delete_rule(id).await?,
        Entity::ProviderRuleSets => state.store().delete_provider_rule_set(id).await?,
        _ => return Err(AdminError::NotFound),
    };
    util::updated(state, applied).await
}

pub(super) async fn reset_routing_defaults(
    state: &impl State,
    provider_id: i64,
) -> Result<Response<Bytes>, AdminError> {
    let provider = state
        .store()
        .control_snapshot()
        .await?
        .providers
        .into_iter()
        .find(|provider| provider.id == provider_id)
        .ok_or(AdminError::NotFound)?;
    let channel = state
        .channel_catalogue()
        .into_iter()
        .find(|channel| channel.id == provider.channel)
        .ok_or(AdminError::NotFound)?;
    crate::reset_provider_defaults(state.store(), provider_id, &channel).await?;
    util::updated(state, true).await
}

async fn routing_input(
    state: &impl State,
    request: RoutingRuleWriteRequest,
) -> Result<gproxy_store::records::RoutingRuleInput, AdminError> {
    ensure_provider(state, request.provider_id).await?;
    let implementation = match request.implementation {
        RoutingImplementationDto::Passthrough => "passthrough",
        RoutingImplementationDto::TransformTo => "transform_to",
        RoutingImplementationDto::Local => "local",
        RoutingImplementationDto::Unsupported => "unsupported",
    }
    .to_owned();
    let input = gproxy_store::records::RoutingRuleInput {
        provider_id: request.provider_id,
        operation: request.operation,
        kind: request.kind,
        implementation,
        dest_operation: request.dest_operation,
        dest_kind: request.dest_kind,
        sort_order: request.sort_order,
        enabled: request.enabled,
    };
    gproxy_core::routing::compile(&gproxy_core::routing::RoutingRuleSpec {
        id: 0,
        operation: input.operation.clone(),
        kind: input.kind.clone(),
        implementation: input.implementation.clone(),
        dest_operation: input.dest_operation.clone(),
        dest_kind: input.dest_kind.clone(),
        sort_order: input.sort_order,
        enabled: input.enabled,
    })
    .map_err(AdminError::BadRequest)?;
    Ok(input)
}

fn rule_set_input(
    request: RuleSetWriteRequest,
) -> Result<gproxy_store::records::RuleSetInput, AdminError> {
    if request.name.trim().is_empty() {
        return Err(AdminError::BadRequest(
            "rule set name must not be blank".into(),
        ));
    }
    Ok(gproxy_store::records::RuleSetInput {
        name: request.name,
        description: request.description,
        enabled: request.enabled,
    })
}

async fn rule_input(
    state: &impl State,
    request: RuleWriteRequest,
) -> Result<gproxy_store::records::RuleInput, AdminError> {
    let snapshot = state.store().control_snapshot().await?;
    if !snapshot
        .rule_sets
        .iter()
        .any(|set| set.id == request.rule_set_id)
    {
        return Err(AdminError::BadRequest("unknown rule set".into()));
    }
    let input = gproxy_store::records::RuleInput {
        rule_set_id: request.rule_set_id,
        kind: request.config.kind().into(),
        config: request.config.storage(),
        filter_model_pattern: request.filter_model_pattern,
        filter_operations: request.filter_operations,
        filter_header_pattern: request.filter_header_pattern,
        sort_order: request.sort_order,
        enabled: request.enabled,
    };
    gproxy_core::process::compile(&process_spec(0, &input)).map_err(AdminError::BadRequest)?;
    Ok(input)
}

async fn attachment_input(
    state: &impl State,
    request: ProviderRuleSetWriteRequest,
) -> Result<gproxy_store::records::ProviderRuleSetInput, AdminError> {
    let snapshot = state.store().control_snapshot().await?;
    if !snapshot
        .providers
        .iter()
        .any(|provider| provider.id == request.provider_id)
    {
        return Err(AdminError::BadRequest("unknown provider".into()));
    }
    if !snapshot
        .rule_sets
        .iter()
        .any(|set| set.id == request.rule_set_id)
    {
        return Err(AdminError::BadRequest("unknown rule set".into()));
    }
    Ok(gproxy_store::records::ProviderRuleSetInput {
        provider_id: request.provider_id,
        rule_set_id: request.rule_set_id,
        sort_order: request.sort_order,
        enabled: request.enabled,
    })
}

async fn ensure_provider(state: &impl State, id: i64) -> Result<(), AdminError> {
    if state
        .store()
        .control_snapshot()
        .await?
        .providers
        .iter()
        .any(|provider| provider.id == id)
    {
        Ok(())
    } else {
        Err(AdminError::BadRequest("unknown provider".into()))
    }
}

pub(super) fn routing_dto(
    value: &gproxy_store::records::RoutingRuleRecord,
) -> Result<RoutingRuleDto, AdminError> {
    let implementation = match value.implementation.as_str() {
        "passthrough" => RoutingImplementationDto::Passthrough,
        "transform_to" => RoutingImplementationDto::TransformTo,
        "local" => RoutingImplementationDto::Local,
        "unsupported" => RoutingImplementationDto::Unsupported,
        other => {
            return Err(AdminError::Internal(format!(
                "stored routing implementation `{other}` is invalid"
            )));
        }
    };
    Ok(RoutingRuleDto {
        id: value.id,
        provider_id: value.provider_id,
        operation: value.operation.clone(),
        kind: value.kind.clone(),
        implementation,
        dest_operation: value.dest_operation.clone(),
        dest_kind: value.dest_kind.clone(),
        sort_order: value.sort_order,
        enabled: value.enabled,
        inherited: value.origin == "channel_default",
    })
}

pub(super) fn rule_dto(value: &gproxy_store::records::RuleRecord) -> Result<RuleDto, AdminError> {
    let mut config = value.config.clone();
    let object = config
        .as_object_mut()
        .ok_or_else(|| AdminError::Internal("stored rule config is not an object".into()))?;
    if value.kind == "transform"
        && let Some(locate) = object.get_mut("locate")
        && let Some(map) = locate.as_object_mut()
    {
        let tagged = ["path", "paths", "match"].into_iter().find_map(|kind| {
            map.remove(kind)
                .map(|value| serde_json::json!({"type": kind, "value": value}))
        });
        if let Some(tagged) = tagged {
            *locate = tagged;
        }
    }
    if value.kind == "rewrite"
        && let Some(value) = object.remove("value_json")
    {
        object.insert("value".into(), value);
    }
    object.insert("kind".into(), value.kind.clone().into());
    Ok(RuleDto {
        id: value.id,
        rule_set_id: value.rule_set_id,
        config: serde_json::from_value(config)
            .map_err(|error| AdminError::Internal(error.to_string()))?,
        filter_model_pattern: value.filter_model_pattern.clone(),
        filter_operations: value.filter_operations.clone(),
        filter_header_pattern: value.filter_header_pattern.clone(),
        sort_order: value.sort_order,
        enabled: value.enabled,
    })
}

fn process_spec(
    id: i64,
    input: &gproxy_store::records::RuleInput,
) -> gproxy_core::process::RuleSpec {
    gproxy_core::process::RuleSpec {
        id,
        kind: input.kind.clone(),
        config: input.config.clone(),
        filter_model_pattern: input.filter_model_pattern.clone(),
        filter_operations: input.filter_operations.clone(),
        filter_header_pattern: input.filter_header_pattern.clone(),
        sort_order: input.sort_order,
        enabled: input.enabled,
    }
}
