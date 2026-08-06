//! Model-limit lookup for public model catalogue entries.

use crate::app::snapshot::{ControlPlaneSnapshot, ResolvedRoute};
use crate::pipeline::preprocess;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ModelLimits {
    pub context_window: Option<i64>,
    pub max_input_tokens: Option<i64>,
    pub max_output_tokens: Option<i64>,
}

impl ModelLimits {
    pub fn new(
        context_window: Option<i64>,
        max_input_tokens: Option<i64>,
        max_output_tokens: Option<i64>,
    ) -> Self {
        Self {
            context_window: positive(context_window),
            max_input_tokens: positive(max_input_tokens),
            max_output_tokens: positive(max_output_tokens),
        }
    }

    fn strict_min(self, other: Self) -> Self {
        Self {
            context_window: min_if_both(self.context_window, other.context_window),
            max_input_tokens: min_if_both(self.max_input_tokens, other.max_input_tokens),
            max_output_tokens: min_if_both(self.max_output_tokens, other.max_output_tokens),
        }
    }
}

pub fn for_target(cp: &ControlPlaneSnapshot, target: &str) -> ModelLimits {
    if let Some(route) = cp.routes_by_name.get(target) {
        return for_route(cp, route);
    }
    let Some((provider_name, requested)) = preprocess::split_provider_model(target) else {
        return ModelLimits::default();
    };
    let Some(provider) = cp
        .providers_by_name
        .get(provider_name)
        .filter(|provider| provider.enabled)
    else {
        return ModelLimits::default();
    };
    let model = preprocess::apply_provider_alias(cp, provider_name, requested);
    for_provider_model(cp, provider.id, &model)
}

pub fn for_provider_model(
    cp: &ControlPlaneSnapshot,
    provider_id: i64,
    exposed_id: &str,
) -> ModelLimits {
    let base_id = cp
        .variant_base_by_provider
        .get(&provider_id)
        .and_then(|variants| variants.get(exposed_id))
        .map(String::as_str)
        .unwrap_or(exposed_id);
    cp.models_by_provider
        .get(&provider_id)
        .and_then(|models| {
            models
                .iter()
                .find(|model| model.enabled && model.model_id == base_id)
        })
        .map(|model| {
            ModelLimits::new(
                model.context_window,
                model.max_input_tokens,
                model.max_output_tokens,
            )
        })
        .unwrap_or_default()
}

fn for_route(cp: &ControlPlaneSnapshot, route: &ResolvedRoute) -> ModelLimits {
    let mut members = route.members.iter();
    let Some(first) = members.next() else {
        return ModelLimits::default();
    };
    members.fold(
        for_provider_model(cp, first.provider_id, &first.upstream_model_id),
        |limits, member| {
            limits.strict_min(for_provider_model(
                cp,
                member.provider_id,
                &member.upstream_model_id,
            ))
        },
    )
}

fn positive(value: Option<i64>) -> Option<i64> {
    value.filter(|value| *value > 0)
}

fn min_if_both(left: Option<i64>, right: Option<i64>) -> Option<i64> {
    Some(left?.min(right?))
}
