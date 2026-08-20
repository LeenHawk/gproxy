//! Model metadata lookup for public model catalogue entries.

use crate::app::snapshot::{ControlPlaneSnapshot, ResolvedRoute};
use crate::pipeline::preprocess;
use crate::store::persistence::records::ProviderModel;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ModelLimits {
    pub context_window: Option<i64>,
    pub max_output_tokens: Option<i64>,
}

impl ModelLimits {
    pub fn new(context_window: Option<i64>, max_output_tokens: Option<i64>) -> Self {
        Self {
            context_window: positive(context_window),
            max_output_tokens: positive(max_output_tokens),
        }
    }

    fn strict_min(self, other: Self) -> Self {
        Self {
            context_window: min_if_both(self.context_window, other.context_window),
            max_output_tokens: min_if_both(self.max_output_tokens, other.max_output_tokens),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ModelThinking {
    pub supported: Option<bool>,
    pub adaptive_supported: Option<bool>,
    pub enabled_supported: Option<bool>,
}

impl ModelThinking {
    pub fn new(
        supported: Option<bool>,
        adaptive_supported: Option<bool>,
        enabled_supported: Option<bool>,
    ) -> Self {
        Self {
            supported,
            adaptive_supported,
            enabled_supported,
        }
    }

    fn strict_intersection(self, other: Self) -> Self {
        Self {
            supported: intersect_bool(self.supported, other.supported),
            adaptive_supported: intersect_bool(self.adaptive_supported, other.adaptive_supported),
            enabled_supported: intersect_bool(self.enabled_supported, other.enabled_supported),
        }
    }
}

pub fn for_target(cp: &ControlPlaneSnapshot, target: &str) -> ModelLimits {
    if let Some(route) = cp.routes_by_name.get(target) {
        return limits_for_route(cp, route);
    }
    resolve_provider_target(cp, target)
        .map(|(provider_id, model)| for_provider_model(cp, provider_id, &model))
        .unwrap_or_default()
}

pub fn thinking_for_target(cp: &ControlPlaneSnapshot, target: &str) -> ModelThinking {
    if let Some(route) = cp.routes_by_name.get(target) {
        return thinking_for_route(cp, route);
    }
    resolve_provider_target(cp, target)
        .map(|(provider_id, model)| thinking_for_provider_model(cp, provider_id, &model))
        .unwrap_or_default()
}

pub fn for_provider_model(
    cp: &ControlPlaneSnapshot,
    provider_id: i64,
    exposed_id: &str,
) -> ModelLimits {
    provider_model(cp, provider_id, exposed_id)
        .map(|model| ModelLimits::new(model.context_window, model.max_output_tokens))
        .unwrap_or_default()
}

pub fn thinking_for_provider_model(
    cp: &ControlPlaneSnapshot,
    provider_id: i64,
    exposed_id: &str,
) -> ModelThinking {
    provider_model(cp, provider_id, exposed_id)
        .map(|model| {
            ModelThinking::new(
                model.thinking_supported,
                model.thinking_adaptive_supported,
                model.thinking_enabled_supported,
            )
        })
        .unwrap_or_default()
}

fn resolve_provider_target(cp: &ControlPlaneSnapshot, target: &str) -> Option<(i64, String)> {
    let (provider_name, requested) = preprocess::split_provider_model(target)?;
    let provider = cp
        .providers_by_name
        .get(provider_name)
        .filter(|provider| provider.enabled)?;
    let model = preprocess::apply_provider_alias(cp, provider_name, requested);
    Some((provider.id, model))
}

fn provider_model<'a>(
    cp: &'a ControlPlaneSnapshot,
    provider_id: i64,
    exposed_id: &str,
) -> Option<&'a ProviderModel> {
    let base_id = cp
        .variant_base_by_provider
        .get(&provider_id)
        .and_then(|variants| variants.get(exposed_id))
        .map(String::as_str)
        .unwrap_or(exposed_id);
    cp.models_by_provider
        .get(&provider_id)?
        .iter()
        .find(|model| model.enabled && model.model_id == base_id)
        .map(AsRef::as_ref)
}

fn limits_for_route(cp: &ControlPlaneSnapshot, route: &ResolvedRoute) -> ModelLimits {
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

fn thinking_for_route(cp: &ControlPlaneSnapshot, route: &ResolvedRoute) -> ModelThinking {
    let mut members = route.members.iter();
    let Some(first) = members.next() else {
        return ModelThinking::default();
    };
    members.fold(
        thinking_for_provider_model(cp, first.provider_id, &first.upstream_model_id),
        |thinking, member| {
            thinking.strict_intersection(thinking_for_provider_model(
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

fn intersect_bool(left: Option<bool>, right: Option<bool>) -> Option<bool> {
    match (left, right) {
        (Some(false), _) | (_, Some(false)) => Some(false),
        (Some(true), Some(true)) => Some(true),
        _ => None,
    }
}

#[cfg(test)]
#[path = "model_limits/tests.rs"]
mod tests;
