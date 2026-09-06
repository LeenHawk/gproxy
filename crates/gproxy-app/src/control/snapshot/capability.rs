use std::collections::{BTreeMap, BTreeSet};

use gproxy_store::records::ProviderModelRecord;
use serde_json::{Value, json};

use super::types::CompiledRoute;

/// What a route can advertise, given what each of its members supports.
///
/// A request may land on any member, so the catalogue promises only what every member
/// can keep. Two rules do the work, and both are deliberately conservative:
///
/// - A limit is known only when **every** member states one; a single silent member
///   makes the route's limit unknown, because silence is not a promise.
/// - A capability flag is false if any member says false, true only if all say true.
///
/// Variants are declared against a member's own upstream model id, so the surviving
/// suffixes are re-based onto the public name before they reach the catalogue.
#[derive(Default)]
pub(super) struct Folded {
    pub display_name: Option<String>,
    pub variants: Option<Value>,
    pub context_window: Option<i64>,
    pub max_output_tokens: Option<i64>,
    pub thinking_supported: Option<bool>,
    pub thinking_adaptive_supported: Option<bool>,
    pub thinking_enabled_supported: Option<bool>,
    pub metadata: gproxy_core::ModelMetadata,
}

pub(super) fn by_provider_model(
    models: &[ProviderModelRecord],
) -> BTreeMap<(i64, &str), &ProviderModelRecord> {
    models
        .iter()
        .filter(|model| model.enabled)
        .map(|model| ((model.provider_id, model.model_id.as_str()), model))
        .collect()
}

pub(super) fn fold(
    route: &CompiledRoute,
    public_name: &str,
    index: &BTreeMap<(i64, &str), &ProviderModelRecord>,
) -> Result<Folded, gproxy_store::StoreError> {
    let members = route
        .targets
        .iter()
        .filter_map(|target| {
            index
                .get(&(target.provider_id, target.upstream_model.as_str()))
                .copied()
        })
        .collect::<Vec<_>>();
    if members.is_empty() {
        return Ok(Folded::default());
    }
    Ok(Folded {
        display_name: common(members.iter().map(|model| model.display_name.as_ref())).cloned(),
        variants: variants(&members, public_name)?,
        context_window: minimum(members.iter().map(|model| model.context_window)),
        max_output_tokens: minimum(members.iter().map(|model| model.max_output_tokens)),
        thinking_supported: intersection(members.iter().map(|model| model.thinking_supported)),
        thinking_adaptive_supported: intersection(
            members
                .iter()
                .map(|model| model.thinking_adaptive_supported),
        ),
        thinking_enabled_supported: intersection(
            members.iter().map(|model| model.thinking_enabled_supported),
        ),
        metadata: fold_metadata(&members),
    })
}

fn fold_metadata(members: &[&ProviderModelRecord]) -> gproxy_core::ModelMetadata {
    let strings = |get: fn(&gproxy_core::ModelMetadata) -> &Option<String>| {
        common(members.iter().map(|model| get(&model.metadata).as_ref())).cloned()
    };
    let numbers = |get: fn(&gproxy_core::ModelMetadata) -> Option<i64>| {
        minimum(members.iter().map(|model| get(&model.metadata)))
    };
    let flags = |get: fn(&gproxy_core::ModelMetadata) -> Option<bool>| {
        intersection(members.iter().map(|model| get(&model.metadata)))
    };
    gproxy_core::ModelMetadata {
        description: strings(|value| &value.description),
        instructions: strings(|value| &value.instructions),
        max_context_window: numbers(|value| value.max_context_window),
        input_modalities: intersect_values(members, |value| &value.input_modalities),
        output_modalities: intersect_values(members, |value| &value.output_modalities),
        supported_parameters: intersect_values(members, |value| &value.supported_parameters),
        reasoning_levels: intersect_values(members, |value| &value.reasoning_levels),
        default_reasoning_level: strings(|value| &value.default_reasoning_level),
        service_tiers: intersect_values(members, |value| &value.service_tiers),
        default_service_tier: strings(|value| &value.default_service_tier),
        generation_methods: intersect_values(members, |value| &value.generation_methods),
        supported_actions: intersect_values(members, |value| &value.supported_actions),
        shell_type: strings(|value| &value.shell_type),
        support_verbosity: flags(|value| value.support_verbosity),
        default_verbosity: strings(|value| &value.default_verbosity),
        supports_reasoning_summary_parameter: flags(|value| {
            value.supports_reasoning_summary_parameter
        }),
        default_reasoning_summary: strings(|value| &value.default_reasoning_summary),
        apply_patch_tool_type: strings(|value| &value.apply_patch_tool_type),
        web_search_tool_type: strings(|value| &value.web_search_tool_type),
        truncation_mode: strings(|value| &value.truncation_mode),
        truncation_limit: numbers(|value| value.truncation_limit),
        auto_compact_token_limit: numbers(|value| value.auto_compact_token_limit),
        effective_context_window_percent: numbers(|value| value.effective_context_window_percent),
        batch_supported: flags(|value| value.batch_supported),
        citations_supported: flags(|value| value.citations_supported),
        code_execution_supported: flags(|value| value.code_execution_supported),
        context_management_supported: flags(|value| value.context_management_supported),
        structured_outputs_supported: flags(|value| value.structured_outputs_supported),
        pdf_input_supported: flags(|value| value.pdf_input_supported),
        supports_image_detail_original: flags(|value| value.supports_image_detail_original),
        supports_search_tool: flags(|value| value.supports_search_tool),
    }
}

fn intersect_values<T: Clone + PartialEq>(
    members: &[&ProviderModelRecord],
    get: impl Fn(&gproxy_core::ModelMetadata) -> &Option<Vec<T>>,
) -> Option<Vec<T>> {
    let first = get(&members.first()?.metadata).as_ref()?.clone();
    let rest = members
        .iter()
        .skip(1)
        .map(|model| get(&model.metadata).as_ref())
        .collect::<Option<Vec<_>>>()?;
    Some(
        first
            .into_iter()
            .filter(|value| rest.iter().all(|values| values.contains(value)))
            .collect(),
    )
}

fn variants(
    members: &[&ProviderModelRecord],
    public_name: &str,
) -> Result<Option<Value>, gproxy_store::StoreError> {
    let mut expose_base = true;
    let mut common_suffixes: Option<BTreeSet<String>> = None;
    for model in members {
        let parsed = gproxy_store::records::parse_model_variants(model.variants.as_ref()).map_err(
            |message| gproxy_store::StoreError::InvalidData {
                field: "model variants",
                message: format!("{}: {message}", model.model_id),
            },
        )?;
        expose_base &= parsed.expose_base;
        let suffixes = parsed
            .names
            .into_iter()
            .filter_map(|name| name.strip_prefix(&model.model_id).map(str::to_owned))
            .filter(|suffix| !suffix.is_empty())
            .collect::<BTreeSet<_>>();
        common_suffixes = Some(match common_suffixes {
            None => suffixes,
            Some(common) => common.intersection(&suffixes).cloned().collect(),
        });
    }
    let names = common_suffixes
        .unwrap_or_default()
        .into_iter()
        .map(|suffix| format!("{public_name}{suffix}"))
        .collect::<Vec<_>>();
    Ok(match (expose_base, names.is_empty()) {
        (true, true) => None,
        (true, false) => Some(json!(names)),
        (false, _) => Some(json!({ "expose_base": false, "variants": names })),
    })
}

fn minimum(values: impl Iterator<Item = Option<i64>>) -> Option<i64> {
    values.collect::<Option<Vec<_>>>()?.into_iter().min()
}

fn intersection(values: impl Iterator<Item = Option<bool>>) -> Option<bool> {
    let values = values.collect::<Vec<_>>();
    if values.contains(&Some(false)) {
        Some(false)
    } else if values.iter().all(|value| *value == Some(true)) {
        Some(true)
    } else {
        None
    }
}

fn common<'a, T: PartialEq>(mut values: impl Iterator<Item = Option<&'a T>>) -> Option<&'a T> {
    let first = values.next()?;
    values
        .all(|value| value == first)
        .then_some(first)
        .flatten()
}

/// The operator's rows as a client would see them: `provider/model`, disabled rows
/// omitted. Discovery refreshes these; the operator's edits outlive the refresh.
pub(super) fn provider_catalogue(
    stored: &gproxy_store::records::ControlSnapshot,
) -> Vec<gproxy_core::ExposedModel> {
    let providers = stored
        .providers
        .iter()
        .filter(|provider| provider.enabled)
        .map(|provider| (provider.id, provider.name.as_str()))
        .collect::<BTreeMap<_, _>>();
    stored
        .provider_models
        .iter()
        .filter(|model| model.enabled)
        .filter_map(|model| {
            let provider = providers.get(&model.provider_id)?;
            Some(gproxy_core::ExposedModel {
                id: format!("{provider}/{}", model.model_id),
                display_name: model.display_name.clone(),
                context_window: model.context_window,
                max_output_tokens: model.max_output_tokens,
                thinking_supported: model.thinking_supported,
                thinking_adaptive_supported: model.thinking_adaptive_supported,
                thinking_enabled_supported: model.thinking_enabled_supported,
                metadata: model.metadata.clone(),
            })
        })
        .collect()
}
