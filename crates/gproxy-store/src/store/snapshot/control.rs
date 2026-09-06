use super::{decimal, json, unsigned, unsigned32};
use crate::StoreError;
use crate::backend::QueryResult;
use crate::records::{
    AliasRecord, CredentialMetaRecord, ExposedModelRecord, PriceRateRecord, PriceRuleRecord,
    ProviderModelRecord, ProviderRecord, RouteMemberRecord, RouteRecord, SettingRecord,
};

pub(super) fn providers(result: QueryResult) -> Result<Vec<ProviderRecord>, StoreError> {
    result
        .rows
        .into_iter()
        .map(|row| {
            Ok(ProviderRecord {
                id: row.i64("id")?,
                name: row.text("name")?.to_owned(),
                label: row.optional_text("label")?.map(str::to_owned),
                channel: row.text("channel")?.to_owned(),
                settings: json(row.text("settings_json")?, "settings_json")?,
                credential_strategy: row.text("credential_strategy")?.to_owned(),
                proxy_url: row.optional_text("proxy_url")?.map(str::to_owned),
                tls_fingerprint: row
                    .optional_text("tls_fingerprint")?
                    .map(|value| json(value, "tls_fingerprint"))
                    .transpose()?,
                enabled: row.i64("enabled")? != 0,
            })
        })
        .collect()
}

pub(super) fn credential_meta(
    result: QueryResult,
) -> Result<Vec<CredentialMetaRecord>, StoreError> {
    result
        .rows
        .into_iter()
        .map(|row| {
            Ok(CredentialMetaRecord {
                id: row.i64("id")?,
                provider_id: row.i64("provider_id")?,
                kind: row.text("kind")?.to_owned(),
                version: unsigned(row.i64("version")?, "credential version")?,
                enabled: row.i64("enabled")? != 0,
                weight: unsigned32(row.i64("weight")?, "credential weight")?,
                rpm_limit: row
                    .optional_i64("rpm_limit")?
                    .map(|value| unsigned32(value, "credential rpm_limit"))
                    .transpose()?,
                tpm_limit: row
                    .optional_i64("tpm_limit")?
                    .map(|value| unsigned(value, "credential tpm_limit"))
                    .transpose()?,
                proxy_url: row.optional_text("proxy_url")?.map(str::to_owned),
                tls_fingerprint: row
                    .optional_text("tls_fingerprint")?
                    .map(|value| json(value, "credential tls_fingerprint"))
                    .transpose()?,
            })
        })
        .collect()
}

pub(super) fn routes(result: QueryResult) -> Result<Vec<RouteRecord>, StoreError> {
    result
        .rows
        .into_iter()
        .map(|row| {
            Ok(RouteRecord {
                id: row.i64("id")?,
                name: row.text("name")?.to_owned(),
                max_attempts: unsigned32(row.i64("max_attempts")?, "max_attempts")?,
                enabled: row.i64("enabled")? != 0,
            })
        })
        .collect()
}

pub(super) fn route_members(result: QueryResult) -> Result<Vec<RouteMemberRecord>, StoreError> {
    result
        .rows
        .into_iter()
        .map(|row| {
            Ok(RouteMemberRecord {
                id: row.i64("id")?,
                route_id: row.i64("route_id")?,
                provider_id: row.i64("provider_id")?,
                upstream_model: row.text("upstream_model")?.to_owned(),
                tier: unsigned32(row.i64("tier")?, "route member tier")?,
                weight: unsigned32(row.i64("weight")?, "route member weight")?,
                enabled: row.i64("enabled")? != 0,
            })
        })
        .collect()
}

pub(super) fn aliases(result: QueryResult) -> Result<Vec<AliasRecord>, StoreError> {
    result
        .rows
        .into_iter()
        .map(|row| {
            Ok(AliasRecord {
                id: row.i64("id")?,
                alias: row.text("alias")?.to_owned(),
                target: row.text("target")?.to_owned(),
                provider_id: row.optional_i64("provider_id")?,
                priority: row.i64("priority")?,
                enabled: row.i64("enabled")? != 0,
            })
        })
        .collect()
}

pub(super) fn exposed_models(result: QueryResult) -> Result<Vec<ExposedModelRecord>, StoreError> {
    result
        .rows
        .into_iter()
        .map(|row| {
            Ok(ExposedModelRecord {
                id: row.i64("id")?,
                name: row.text("name")?.to_owned(),
                route_id: row.i64("route_id")?,
                enabled: row.i64("enabled")? != 0,
            })
        })
        .collect()
}

pub(super) fn provider_models(
    result: QueryResult,
    modalities: QueryResult,
    parameters: QueryResult,
    reasoning: QueryResult,
    tiers: QueryResult,
    methods: QueryResult,
) -> Result<Vec<ProviderModelRecord>, StoreError> {
    let mut metadata = model_metadata(modalities, parameters, reasoning, tiers, methods)?;
    result
        .rows
        .into_iter()
        .map(|row| {
            let key = (row.i64("provider_id")?, row.text("model_id")?.to_owned());
            let mut collections = metadata.remove(&key).unwrap_or_default();
            if row.i64("input_modalities_known")? == 0 {
                collections.input_modalities = None;
            } else {
                collections.input_modalities.get_or_insert_default();
            }
            if row.i64("output_modalities_known")? == 0 {
                collections.output_modalities = None;
            } else {
                collections.output_modalities.get_or_insert_default();
            }
            if row.i64("parameters_known")? == 0 {
                collections.supported_parameters = None;
            } else {
                collections.supported_parameters.get_or_insert_default();
            }
            if row.i64("reasoning_levels_known")? == 0 {
                collections.reasoning_levels = None;
            } else {
                collections.reasoning_levels.get_or_insert_default();
            }
            if row.i64("service_tiers_known")? == 0 {
                collections.service_tiers = None;
            } else {
                collections.service_tiers.get_or_insert_default();
            }
            if row.i64("generation_methods_known")? == 0 {
                collections.generation_methods = None;
            } else {
                collections.generation_methods.get_or_insert_default();
            }
            if row.i64("supported_actions_known")? == 0 {
                collections.supported_actions = None;
            } else {
                collections.supported_actions.get_or_insert_default();
            }
            Ok(ProviderModelRecord {
                id: row.i64("id")?,
                provider_id: key.0,
                model_id: key.1,
                display_name: row.optional_text("display_name")?.map(str::to_owned),
                variants: row
                    .optional_text("variants_json")?
                    .map(|value| json(value, "model variants"))
                    .transpose()?,
                context_window: row.optional_i64("context_window")?,
                max_output_tokens: row.optional_i64("max_output_tokens")?,
                thinking_supported: row
                    .optional_i64("thinking_supported")?
                    .map(|value| value != 0),
                thinking_adaptive_supported: row
                    .optional_i64("thinking_adaptive_supported")?
                    .map(|value| value != 0),
                thinking_enabled_supported: row
                    .optional_i64("thinking_enabled_supported")?
                    .map(|value| value != 0),
                metadata: gproxy_core::ModelMetadata {
                    description: optional_text(&row, "description")?,
                    instructions: optional_text(&row, "instructions")?,
                    max_context_window: row.optional_i64("max_context_window")?,
                    default_reasoning_level: optional_text(&row, "default_reasoning_level")?,
                    default_service_tier: optional_text(&row, "default_service_tier")?,
                    shell_type: optional_text(&row, "shell_type")?,
                    support_verbosity: optional_bool(&row, "support_verbosity")?,
                    default_verbosity: optional_text(&row, "default_verbosity")?,
                    supports_reasoning_summary_parameter: optional_bool(
                        &row,
                        "reasoning_summary_supported",
                    )?,
                    default_reasoning_summary: optional_text(&row, "default_reasoning_summary")?,
                    apply_patch_tool_type: optional_text(&row, "apply_patch_tool_type")?,
                    web_search_tool_type: optional_text(&row, "web_search_tool_type")?,
                    truncation_mode: optional_text(&row, "truncation_mode")?,
                    truncation_limit: row.optional_i64("truncation_limit")?,
                    auto_compact_token_limit: row.optional_i64("auto_compact_token_limit")?,
                    effective_context_window_percent: row
                        .optional_i64("effective_context_window_percent")?,
                    batch_supported: optional_bool(&row, "batch_supported")?,
                    citations_supported: optional_bool(&row, "citations_supported")?,
                    code_execution_supported: optional_bool(&row, "code_execution_supported")?,
                    context_management_supported: optional_bool(
                        &row,
                        "context_management_supported",
                    )?,
                    structured_outputs_supported: optional_bool(
                        &row,
                        "structured_outputs_supported",
                    )?,
                    pdf_input_supported: optional_bool(&row, "pdf_input_supported")?,
                    supports_image_detail_original: optional_bool(
                        &row,
                        "image_detail_original_supported",
                    )?,
                    supports_search_tool: optional_bool(&row, "search_supported")?,
                    ..collections
                },
                enabled: row.i64("enabled")? != 0,
            })
        })
        .collect()
}

fn optional_text(
    row: &crate::backend::Row,
    name: &'static str,
) -> Result<Option<String>, StoreError> {
    Ok(row.optional_text(name)?.map(str::to_owned))
}

fn optional_bool(
    row: &crate::backend::Row,
    name: &'static str,
) -> Result<Option<bool>, StoreError> {
    Ok(row.optional_i64(name)?.map(|value| value != 0))
}

fn model_metadata(
    modalities: QueryResult,
    parameters: QueryResult,
    reasoning: QueryResult,
    tiers: QueryResult,
    methods: QueryResult,
) -> Result<std::collections::BTreeMap<(i64, String), gproxy_core::ModelMetadata>, StoreError> {
    let mut result = std::collections::BTreeMap::new();
    for row in modalities.rows {
        let metadata = metadata_entry(&mut result, &row)?;
        let value = row.text("modality")?.to_owned();
        match row.text("direction")? {
            "input" => metadata
                .input_modalities
                .get_or_insert_default()
                .push(value),
            "output" => metadata
                .output_modalities
                .get_or_insert_default()
                .push(value),
            direction => {
                return Err(StoreError::InvalidData {
                    field: "model modality direction",
                    message: direction.to_owned(),
                });
            }
        }
    }
    for row in parameters.rows {
        metadata_entry(&mut result, &row)?
            .supported_parameters
            .get_or_insert_default()
            .push(row.text("parameter")?.to_owned());
    }
    for row in reasoning.rows {
        metadata_entry(&mut result, &row)?
            .reasoning_levels
            .get_or_insert_default()
            .push(gproxy_core::ModelReasoningLevel {
                effort: row.text("effort")?.to_owned(),
                description: row.text("description")?.to_owned(),
            });
    }
    for row in tiers.rows {
        metadata_entry(&mut result, &row)?
            .service_tiers
            .get_or_insert_default()
            .push(gproxy_core::ModelServiceTier {
                id: row.text("tier_id")?.to_owned(),
                name: row.text("name")?.to_owned(),
                description: row.text("description")?.to_owned(),
            });
    }
    for row in methods.rows {
        let metadata = metadata_entry(&mut result, &row)?;
        let value = row.text("method")?.to_owned();
        match row.text("kind")? {
            "generation" => metadata
                .generation_methods
                .get_or_insert_default()
                .push(value),
            "action" => metadata
                .supported_actions
                .get_or_insert_default()
                .push(value),
            kind => {
                return Err(StoreError::InvalidData {
                    field: "model method kind",
                    message: kind.to_owned(),
                });
            }
        }
    }
    Ok(result)
}

fn metadata_entry<'a>(
    metadata: &'a mut std::collections::BTreeMap<(i64, String), gproxy_core::ModelMetadata>,
    row: &crate::backend::Row,
) -> Result<&'a mut gproxy_core::ModelMetadata, StoreError> {
    Ok(metadata
        .entry((row.i64("provider_id")?, row.text("model_id")?.to_owned()))
        .or_default())
}

pub(super) fn price_rules(result: QueryResult) -> Result<Vec<PriceRuleRecord>, StoreError> {
    result
        .rows
        .into_iter()
        .map(|row| {
            Ok(PriceRuleRecord {
                id: row.i64("id")?,
                provider_id: row.optional_i64("provider_id")?,
                model_pattern: row.text("model_pattern")?.to_owned(),
                tiers: row
                    .optional_text("tiers_json")?
                    .map(|value| json(value, "tiers_json"))
                    .transpose()?,
                priority: row.i64("priority")?,
                enabled: row.i64("enabled")? != 0,
            })
        })
        .collect()
}

pub(super) fn price_rates(result: QueryResult) -> Result<Vec<PriceRateRecord>, StoreError> {
    result
        .rows
        .into_iter()
        .map(|row| {
            Ok(PriceRateRecord {
                id: row.i64("id")?,
                rule_id: row.i64("rule_id")?,
                metric: row.text("metric")?.to_owned(),
                unit_size: unsigned(row.i64("unit_size")?, "unit_size")?,
                price: decimal(row.text("price")?, "price")?,
                conditions: row
                    .optional_text("conditions_json")?
                    .map(|value| json(value, "conditions_json"))
                    .transpose()?,
                priority: row.i64("priority")?,
            })
        })
        .collect()
}

pub(super) fn settings(result: QueryResult) -> Result<Vec<SettingRecord>, StoreError> {
    result
        .rows
        .into_iter()
        .map(|row| {
            Ok(SettingRecord {
                key: row.text("key")?.to_owned(),
                value: json(row.text("value_json")?, "value_json")?,
            })
        })
        .collect()
}
