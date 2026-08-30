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
    })
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
