use std::collections::BTreeSet;
use std::sync::LazyLock;

use bytes::Bytes;
use http::{Response, StatusCode};
use rust_decimal::Decimal;

use crate::dto::{DefaultModelCatalogDto, DefaultModelDto, DefaultModelPricingDto};
use crate::{AdminError, response};

const CATALOG_JSON: &[u8] = include_bytes!("../../../assets/default-model-catalog.json");
static CATALOG: LazyLock<Result<DefaultModelCatalogDto, String>> =
    LazyLock::new(|| parse().map_err(|error| error.to_string()));

pub(crate) fn list() -> Result<Response<Bytes>, AdminError> {
    response::json(StatusCode::OK, catalog()?)
}

pub(crate) fn model(model: &str) -> Option<&'static DefaultModelDto> {
    let needle = model.trim().to_ascii_lowercase();
    if needle.is_empty() {
        return None;
    }
    let catalog = catalog().ok()?;
    if let Some(exact) = catalog
        .models
        .iter()
        .find(|entry| entry.model_id.eq_ignore_ascii_case(&needle))
    {
        return Some(exact);
    }
    let basename = needle.rsplit('/').next()?;
    let mut matches = catalog.models.iter().filter(|entry| {
        entry
            .model_id
            .rsplit('/')
            .next()
            .is_some_and(|candidate| candidate.eq_ignore_ascii_case(basename))
    });
    let found = matches.next()?;
    matches.next().is_none().then_some(found)
}

pub(super) fn price(model: &str) -> Option<&'static DefaultModelPricingDto> {
    let needle = model.trim().to_ascii_lowercase();
    catalog()
        .ok()?
        .models
        .iter()
        .filter_map(|model| model.pricing.as_ref())
        .filter_map(|pricing| {
            let fragment = pricing.model_pattern.strip_prefix('*')?.strip_suffix('*')?;
            needle
                .contains(&fragment.to_ascii_lowercase())
                .then_some((fragment.len(), pricing))
        })
        .fold(None, |best, candidate| match best {
            Some((length, _)) if length >= candidate.0 => best,
            _ => Some(candidate),
        })
        .map(|(_, pricing)| pricing)
}

pub(crate) fn has_price(model: &str) -> bool {
    price(model).is_some()
}

pub(crate) fn price_count() -> usize {
    catalog()
        .map(|catalog| catalog.source.priced_models)
        .unwrap_or_default()
}

pub(super) fn catalog() -> Result<&'static DefaultModelCatalogDto, AdminError> {
    CATALOG
        .as_ref()
        .map_err(|error| AdminError::Internal(error.clone()))
}

fn parse() -> Result<DefaultModelCatalogDto, AdminError> {
    let catalog: DefaultModelCatalogDto = serde_json::from_slice(CATALOG_JSON)
        .map_err(|error| AdminError::Internal(format!("default model catalog: {error}")))?;
    let priced = catalog
        .models
        .iter()
        .filter(|model| model.pricing.is_some())
        .count();
    if catalog.schema_version != 2
        || !matches!(
            catalog.source.catalog.as_str(),
            "openrouter" | "openrouter+codex"
        )
        || catalog.source.total_models != catalog.models.len()
        || catalog.source.priced_models != priced
    {
        return invalid("metadata");
    }
    let mut ids = BTreeSet::new();
    let mut patterns = BTreeSet::new();
    for model in &catalog.models {
        if model.model_id.trim().is_empty()
            || !model.model_id.contains('/')
            || !ids.insert(model.model_id.as_str())
            || model.context_window.is_some_and(|value| value <= 0)
            || model.max_output_tokens.is_some_and(|value| value <= 0)
            || !valid_optional_strings(model.metadata.input_modalities.as_deref())
            || !valid_optional_strings(model.metadata.output_modalities.as_deref())
            || !valid_optional_strings(model.metadata.supported_parameters.as_deref())
            || !valid_reasoning(model.metadata.reasoning_levels.as_deref())
            || !valid_tiers(model.metadata.service_tiers.as_deref())
            || model.metadata.truncation_mode.is_some() != model.metadata.truncation_limit.is_some()
        {
            return invalid("model");
        }
        if let Some(pricing) = &model.pricing {
            validate_pricing(pricing, &mut patterns)?;
        }
    }
    Ok(catalog)
}

fn validate_pricing<'a>(
    pricing: &'a DefaultModelPricingDto,
    patterns: &mut BTreeSet<&'a str>,
) -> Result<(), AdminError> {
    let fragment = pricing
        .model_pattern
        .strip_prefix('*')
        .and_then(|value| value.strip_suffix('*'));
    if fragment.is_none_or(str::is_empty)
        || !patterns.insert(pricing.model_pattern.as_str())
        || pricing.rates.is_empty()
    {
        return invalid("pricing");
    }
    gproxy_store::records::parse_price_tiers(pricing.tiers.as_ref())?;
    for rate in &pricing.rates {
        let price = rate.price.parse::<Decimal>().map_err(|_| {
            AdminError::Internal("default model catalog contains an invalid price".into())
        })?;
        if rate.metric.trim().is_empty() || rate.unit_size == 0 || price < Decimal::ZERO {
            return invalid("price rate");
        }
    }
    Ok(())
}

fn valid_strings(values: &[String]) -> bool {
    let mut seen = BTreeSet::new();
    values
        .iter()
        .all(|value| !value.trim().is_empty() && seen.insert(value.as_str()))
}

fn valid_optional_strings(values: Option<&[String]>) -> bool {
    values.is_none_or(valid_strings)
}

fn valid_reasoning(values: Option<&[crate::dto::ModelReasoningLevelDto]>) -> bool {
    values.is_none_or(|values| {
        let mut seen = BTreeSet::new();
        values.iter().all(|value| {
            matches!(
                value.effort.as_str(),
                "none" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max" | "ultra"
            ) && seen.insert(value.effort.as_str())
        })
    })
}

fn valid_tiers(values: Option<&[crate::dto::ModelServiceTierDto]>) -> bool {
    values.is_none_or(|values| {
        let mut seen = BTreeSet::new();
        values.iter().all(|value| {
            !value.id.trim().is_empty()
                && !value.name.trim().is_empty()
                && seen.insert(value.id.as_str())
        })
    })
}

fn invalid<T>(part: &str) -> Result<T, AdminError> {
    Err(AdminError::Internal(format!(
        "default model catalog contains invalid {part}"
    )))
}

#[cfg(test)]
mod tests {
    #[test]
    fn embedded_catalog_is_valid_and_matches_metadata_and_price() {
        let catalog = super::catalog().expect("embedded catalog");
        assert_eq!(catalog.models.len(), catalog.source.total_models);
        assert!(catalog.source.priced_models > 400);
        let model = super::model("GPT-5.6-SOL").expect("model metadata");
        assert_eq!(model.model_id, "openai/gpt-5.6-sol");
        assert!(model.context_window.is_some());
        let price = super::price("OPENAI/GPT-5.6-SOL-PRO:BATCH").expect("model price");
        assert!(price.model_pattern.contains("gpt-5.6-sol-pro:batch"));
    }
}
