mod catalog;
mod opencode;

use bytes::Bytes;
use http::{Response, StatusCode};

use crate::dto::{RulePresetDto, RuleSetDto};
use crate::{AdminError, State, response};

pub(super) fn list() -> Result<Response<Bytes>, AdminError> {
    response::json(StatusCode::OK, &catalog::all())
}

pub(super) async fn apply(
    state: &impl State,
    provider_id: i64,
    preset_id: &str,
) -> Result<Response<Bytes>, AdminError> {
    let preset = catalog::all()
        .into_iter()
        .find(|preset| preset.id == preset_id)
        .ok_or(AdminError::NotFound)?;
    let snapshot = state.store().control_snapshot().await?;
    if !snapshot
        .providers
        .iter()
        .any(|value| value.id == provider_id)
    {
        return Err(AdminError::BadRequest("unknown provider".into()));
    }
    let set_input = gproxy_store::records::RuleSetInput {
        name: format!("{} compatibility", preset.name),
        description: Some(preset.description.clone()),
        enabled: true,
    };
    let existing_set = snapshot
        .rule_sets
        .iter()
        .find(|value| value.description.as_deref() == Some(&preset.description));
    let rule_set_id = match existing_set {
        Some(value) => {
            state.store().update_rule_set(value.id, &set_input).await?;
            value.id
        }
        None => state.store().insert_rule_set(&set_input).await?,
    };
    upsert_rules(state, &snapshot, rule_set_id, &preset).await?;
    upsert_attachment(state, &snapshot, provider_id, rule_set_id).await?;
    state.reload().await?;
    response::json(
        StatusCode::OK,
        &RuleSetDto {
            id: rule_set_id,
            name: set_input.name,
            description: set_input.description,
            enabled: true,
        },
    )
}

async fn upsert_rules(
    state: &impl State,
    snapshot: &gproxy_store::records::ControlSnapshot,
    rule_set_id: i64,
    preset: &RulePresetDto,
) -> Result<(), AdminError> {
    for rule in &preset.rules {
        let input = gproxy_store::records::RuleInput {
            rule_set_id,
            kind: rule.config.kind().into(),
            config: rule.config.storage(),
            filter_model_pattern: rule.filter_model_pattern.clone(),
            filter_operations: rule.filter_operations.clone(),
            filter_header_pattern: rule.filter_header_pattern.clone(),
            sort_order: rule.sort_order,
            enabled: rule.enabled,
        };
        validate(&input)?;
        let existing = snapshot.rules.iter().find(|value| {
            value.rule_set_id == rule_set_id
                && value.kind == input.kind
                && value.sort_order == input.sort_order
        });
        match existing {
            Some(value) => {
                state.store().update_rule(value.id, &input).await?;
            }
            None => {
                state.store().insert_rule(&input).await?;
            }
        }
    }
    Ok(())
}

fn validate(input: &gproxy_store::records::RuleInput) -> Result<(), AdminError> {
    gproxy_core::process::compile(&gproxy_core::process::RuleSpec {
        id: 0,
        kind: input.kind.clone(),
        config: input.config.clone(),
        filter_model_pattern: input.filter_model_pattern.clone(),
        filter_operations: input.filter_operations.clone(),
        filter_header_pattern: input.filter_header_pattern.clone(),
        sort_order: input.sort_order,
        enabled: input.enabled,
    })
    .map(|_| ())
    .map_err(AdminError::Internal)
}

async fn upsert_attachment(
    state: &impl State,
    snapshot: &gproxy_store::records::ControlSnapshot,
    provider_id: i64,
    rule_set_id: i64,
) -> Result<(), AdminError> {
    let existing = snapshot
        .provider_rule_sets
        .iter()
        .find(|value| value.provider_id == provider_id && value.rule_set_id == rule_set_id);
    let input = gproxy_store::records::ProviderRuleSetInput {
        provider_id,
        rule_set_id,
        sort_order: existing.map_or_else(
            || {
                snapshot
                    .provider_rule_sets
                    .iter()
                    .filter(|value| value.provider_id == provider_id)
                    .map(|value| value.sort_order)
                    .max()
                    .unwrap_or(-1)
                    + 1
            },
            |value| value.sort_order,
        ),
        enabled: true,
    };
    match existing {
        Some(value) => {
            state
                .store()
                .update_provider_rule_set(value.id, &input)
                .await?;
        }
        None => {
            state.store().insert_provider_rule_set(&input).await?;
        }
    }
    Ok(())
}
