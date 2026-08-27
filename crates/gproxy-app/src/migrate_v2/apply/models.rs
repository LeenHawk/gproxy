use std::collections::BTreeSet;

use serde_json::{Value, json};

use crate::migrate_v2::model::{ProviderModel, SourceData};

pub(super) struct Metadata {
    pub display_name: Option<String>,
    pub variants: Option<Value>,
    pub context_window: Option<i64>,
    pub max_output_tokens: Option<i64>,
    pub thinking_supported: Option<bool>,
    pub thinking_adaptive_supported: Option<bool>,
    pub thinking_enabled_supported: Option<bool>,
}

pub(super) fn for_route(
    data: &SourceData,
    route_id: i64,
    public_name: &str,
) -> Result<Metadata, crate::AppError> {
    let rows = data
        .route_members
        .iter()
        .filter(|member| member.value.route_id == route_id && member.value.enabled)
        .map(|member| {
            data.provider_models.iter().find(|model| {
                model.value.enabled
                    && model.value.provider_id == member.value.provider_id
                    && model.value.model_id == member.value.upstream_model
            })
        })
        .collect::<Option<Vec<_>>>();
    let Some(rows) = rows.filter(|rows| !rows.is_empty()) else {
        return Ok(empty());
    };
    let display_name = common(rows.iter().map(|row| row.value.display_name.as_ref())).cloned();
    let context_window = minimum(rows.iter().map(|row| row.value.context_window));
    let max_output_tokens = minimum(rows.iter().map(|row| row.value.max_output_tokens));
    let thinking_supported = intersection(rows.iter().map(|row| row.value.thinking_supported));
    let thinking_adaptive_supported =
        intersection(rows.iter().map(|row| row.value.thinking_adaptive_supported));
    let thinking_enabled_supported =
        intersection(rows.iter().map(|row| row.value.thinking_enabled_supported));
    let variants = route_variants(&rows, public_name)?;
    Ok(Metadata {
        display_name,
        variants,
        context_window,
        max_output_tokens,
        thinking_supported,
        thinking_adaptive_supported,
        thinking_enabled_supported,
    })
}

fn route_variants(
    rows: &[&crate::migrate_v2::model::Legacy<ProviderModel>],
    public_name: &str,
) -> Result<Option<Value>, crate::AppError> {
    let mut expose_base = true;
    let mut common_suffixes: Option<BTreeSet<String>> = None;
    for row in rows {
        let parsed = gproxy_store::records::parse_model_variants(row.value.variants.as_ref())
            .map_err(crate::AppError::Migration)?;
        expose_base &= parsed.expose_base;
        let suffixes = parsed
            .names
            .into_iter()
            .filter_map(|name| name.strip_prefix(&row.value.model_id).map(str::to_owned))
            .filter(|suffix| !suffix.is_empty())
            .collect::<BTreeSet<_>>();
        common_suffixes = Some(match common_suffixes {
            None => suffixes,
            Some(common) => common.intersection(&suffixes).cloned().collect(),
        });
    }
    let variants = common_suffixes
        .unwrap_or_default()
        .into_iter()
        .map(|suffix| format!("{public_name}{suffix}"))
        .collect::<Vec<_>>();
    match (expose_base, variants.is_empty()) {
        (true, true) => Ok(None),
        (true, false) => Ok(Some(json!(variants))),
        (false, _) => Ok(Some(json!({ "expose_base": false, "variants": variants }))),
    }
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
    let first = values.next()??;
    values.all(|value| value == Some(first)).then_some(first)
}

fn empty() -> Metadata {
    Metadata {
        display_name: None,
        variants: None,
        context_window: None,
        max_output_tokens: None,
        thinking_supported: None,
        thinking_adaptive_supported: None,
        thinking_enabled_supported: None,
    }
}
