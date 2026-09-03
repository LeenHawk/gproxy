mod catalog;
mod opencode;

use bytes::Bytes;
use http::{Response, StatusCode};
use std::collections::BTreeMap;

use crate::dto::{RulePresetDto, RuleSetDto};
use crate::{AdminError, State, response};

pub(super) fn list() -> Result<Response<Bytes>, AdminError> {
    response::json(StatusCode::OK, &catalog::all())
}

pub(super) async fn apply(
    state: &impl State,
    rule_set_id: i64,
    preset_id: &str,
) -> Result<Response<Bytes>, AdminError> {
    let preset = catalog::all()
        .into_iter()
        .find(|preset| preset.id == preset_id)
        .ok_or(AdminError::NotFound)?;
    let snapshot = state.store().control_snapshot().await?;
    let rule_set = snapshot
        .rule_sets
        .iter()
        .find(|value| value.id == rule_set_id)
        .ok_or(AdminError::NotFound)?;
    upsert_rules(state, &snapshot, rule_set_id, &preset).await?;
    state.reload().await?;
    response::json(
        StatusCode::OK,
        &RuleSetDto {
            id: rule_set_id,
            name: rule_set.name.clone(),
            description: rule_set.description.clone(),
            enabled: rule_set.enabled,
        },
    )
}

async fn upsert_rules(
    state: &impl State,
    snapshot: &gproxy_store::records::ControlSnapshot,
    rule_set_id: i64,
    preset: &RulePresetDto,
) -> Result<(), AdminError> {
    let mut next_orders = BTreeMap::<String, i64>::new();
    for rule in snapshot
        .rules
        .iter()
        .filter(|rule| rule.rule_set_id == rule_set_id)
    {
        let next = next_orders.entry(rule.kind.clone()).or_default();
        *next = (*next).max(rule.sort_order + 1);
    }
    for rule in &preset.rules {
        let mut input = gproxy_store::records::RuleInput {
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
            value.rule_set_id == input.rule_set_id
                && value.kind == input.kind
                && value.config == input.config
                && value.filter_model_pattern == input.filter_model_pattern
                && value.filter_operations == input.filter_operations
                && value.filter_header_pattern == input.filter_header_pattern
        });
        match existing {
            Some(value) => {
                input.sort_order = value.sort_order;
                state.store().update_rule(value.id, &input).await?;
            }
            None => {
                let next = next_orders.entry(input.kind.clone()).or_default();
                input.sort_order = *next;
                *next += 1;
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
