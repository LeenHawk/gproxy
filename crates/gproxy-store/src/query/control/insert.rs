use sea_query::{Alias, OnConflict, Query};

use crate::StoreError;
use crate::backend::Statement;
use crate::query::common::{decimal, insert, json, unsigned, unsigned32, value};
use crate::records::{
    AliasInput, CredentialInput, ExposedModelInput, PriceRateInput, PriceRuleInput, ProviderInput,
    ProviderModelInput, RouteInput, RouteMemberInput, SettingInput,
};

pub(crate) fn insert_provider(input: &ProviderInput) -> Result<Statement, StoreError> {
    insert(
        "providers",
        &[
            "name",
            "label",
            "channel",
            "settings_json",
            "credential_strategy",
            "proxy_url",
            "enabled",
            "tls_fingerprint",
        ],
        vec![
            value(input.name.clone()),
            value(input.label.clone()),
            value(input.channel.clone()),
            value(json(&input.settings, "settings")?),
            value(input.credential_strategy.clone()),
            value(input.proxy_url.clone()),
            value(input.enabled),
            value(
                input
                    .tls_fingerprint
                    .as_ref()
                    .map(|fingerprint| json(fingerprint, "tls_fingerprint"))
                    .transpose()?,
            ),
        ],
    )
}

pub(crate) fn insert_credential(input: &CredentialInput) -> Result<Statement, StoreError> {
    insert(
        "credentials",
        &[
            "provider_id",
            "label",
            "kind",
            "ciphertext",
            "wrapped_key",
            "payload_nonce",
            "key_nonce",
            "version",
            "enabled",
            "weight",
            "rpm_limit",
            "tpm_limit",
            "proxy_url",
            "tls_fingerprint",
        ],
        vec![
            value(input.provider_id),
            value(input.label.clone()),
            value(input.kind.clone()),
            value(input.envelope.ciphertext.clone()),
            value(input.envelope.wrapped_key.clone()),
            value(input.envelope.payload_nonce.clone()),
            value(input.envelope.key_nonce.clone()),
            value(0_i64),
            value(input.enabled),
            value(unsigned32(input.weight)),
            value(input.rpm_limit.map(unsigned32)),
            value(
                input
                    .tpm_limit
                    .map(|value| unsigned(value, "tpm_limit"))
                    .transpose()?,
            ),
            value(input.proxy_url.clone()),
            value(
                input
                    .tls_fingerprint
                    .as_ref()
                    .map(|fingerprint| json(fingerprint, "tls_fingerprint"))
                    .transpose()?,
            ),
        ],
    )
}

pub(crate) fn insert_route(input: &RouteInput) -> Result<Statement, StoreError> {
    insert(
        "routes",
        &["name", "max_attempts", "enabled"],
        vec![
            value(input.name.clone()),
            value(unsigned32(input.max_attempts)),
            value(input.enabled),
        ],
    )
}

pub(crate) fn insert_route_member(input: &RouteMemberInput) -> Result<Statement, StoreError> {
    insert(
        "route_members",
        &[
            "route_id",
            "provider_id",
            "credential_id",
            "upstream_model",
            "priority",
            "tier",
            "weight",
            "enabled",
        ],
        vec![
            value(input.route_id),
            value(input.provider_id),
            value(input.credential_id),
            value(input.upstream_model.clone()),
            value(unsigned32(input.tier)),
            value(unsigned32(input.tier)),
            value(unsigned32(input.weight)),
            value(input.enabled),
        ],
    )
}

pub(crate) fn insert_alias(input: &AliasInput) -> Result<Statement, StoreError> {
    insert(
        "aliases",
        &["alias", "target", "provider_id", "priority", "enabled"],
        vec![
            value(input.alias.clone()),
            value(input.target.clone()),
            value(input.provider_id),
            value(input.priority),
            value(input.enabled),
        ],
    )
}

pub(crate) fn insert_exposed_model(input: &ExposedModelInput) -> Result<Statement, StoreError> {
    insert(
        "exposed_models",
        &["name", "route_id", "enabled"],
        vec![
            value(input.name.clone()),
            value(input.route_id),
            value(input.enabled),
        ],
    )
}

pub(crate) fn insert_provider_model(input: &ProviderModelInput) -> Result<Statement, StoreError> {
    insert(
        "provider_models",
        &[
            "provider_id",
            "model_id",
            "display_name",
            "variants_json",
            "context_window",
            "max_output_tokens",
            "thinking_supported",
            "thinking_adaptive_supported",
            "thinking_enabled_supported",
            "description",
            "instructions",
            "max_context_window",
            "default_reasoning_level",
            "default_service_tier",
            "shell_type",
            "support_verbosity",
            "default_verbosity",
            "reasoning_summary_supported",
            "default_reasoning_summary",
            "apply_patch_tool_type",
            "web_search_tool_type",
            "truncation_mode",
            "truncation_limit",
            "auto_compact_token_limit",
            "effective_context_window_percent",
            "batch_supported",
            "citations_supported",
            "code_execution_supported",
            "context_management_supported",
            "structured_outputs_supported",
            "pdf_input_supported",
            "image_detail_original_supported",
            "search_supported",
            "input_modalities_known",
            "output_modalities_known",
            "parameters_known",
            "reasoning_levels_known",
            "service_tiers_known",
            "generation_methods_known",
            "supported_actions_known",
            "enabled",
        ],
        vec![
            value(input.provider_id),
            value(input.model_id.clone()),
            value(input.display_name.clone()),
            value(
                input
                    .variants
                    .as_ref()
                    .map(|variants| json(variants, "model variants"))
                    .transpose()?,
            ),
            value(input.context_window),
            value(input.max_output_tokens),
            value(input.thinking_supported),
            value(input.thinking_adaptive_supported),
            value(input.thinking_enabled_supported),
            value(input.metadata.description.clone()),
            value(input.metadata.instructions.clone()),
            value(input.metadata.max_context_window),
            value(input.metadata.default_reasoning_level.clone()),
            value(input.metadata.default_service_tier.clone()),
            value(input.metadata.shell_type.clone()),
            value(input.metadata.support_verbosity),
            value(input.metadata.default_verbosity.clone()),
            value(input.metadata.supports_reasoning_summary_parameter),
            value(input.metadata.default_reasoning_summary.clone()),
            value(input.metadata.apply_patch_tool_type.clone()),
            value(input.metadata.web_search_tool_type.clone()),
            value(input.metadata.truncation_mode.clone()),
            value(input.metadata.truncation_limit),
            value(input.metadata.auto_compact_token_limit),
            value(input.metadata.effective_context_window_percent),
            value(input.metadata.batch_supported),
            value(input.metadata.citations_supported),
            value(input.metadata.code_execution_supported),
            value(input.metadata.context_management_supported),
            value(input.metadata.structured_outputs_supported),
            value(input.metadata.pdf_input_supported),
            value(input.metadata.supports_image_detail_original),
            value(input.metadata.supports_search_tool),
            value(input.metadata.input_modalities.is_some()),
            value(input.metadata.output_modalities.is_some()),
            value(input.metadata.supported_parameters.is_some()),
            value(input.metadata.reasoning_levels.is_some()),
            value(input.metadata.service_tiers.is_some()),
            value(input.metadata.generation_methods.is_some()),
            value(input.metadata.supported_actions.is_some()),
            value(input.enabled),
        ],
    )
}

pub(crate) fn insert_price_rule(input: &PriceRuleInput) -> Result<Statement, StoreError> {
    crate::records::parse_price_tiers(input.tiers.as_ref())?;
    insert(
        "price_rules",
        &[
            "provider_id",
            "model_pattern",
            "tiers_json",
            "priority",
            "enabled",
        ],
        vec![
            value(input.provider_id),
            value(input.model_pattern.clone()),
            value(
                input
                    .tiers
                    .as_ref()
                    .map(|tiers| json(tiers, "tiers"))
                    .transpose()?,
            ),
            value(input.priority),
            value(input.enabled),
        ],
    )
}

pub(crate) fn insert_price_rate(input: &PriceRateInput) -> Result<Statement, StoreError> {
    insert(
        "price_rates",
        &[
            "rule_id",
            "metric",
            "unit_size",
            "price",
            "conditions_json",
            "priority",
        ],
        vec![
            value(input.rule_id),
            value(input.metric.clone()),
            value(unsigned(input.unit_size, "unit_size")?),
            value(decimal(input.price)),
            value(
                input
                    .conditions
                    .as_ref()
                    .map(|conditions| json(conditions, "conditions"))
                    .transpose()?,
            ),
            value(input.priority),
        ],
    )
}

pub(crate) fn insert_setting(input: &SettingInput) -> Result<Statement, StoreError> {
    let mut query = Query::insert();
    query
        .into_table(Alias::new("settings"))
        .columns([Alias::new("key"), Alias::new("value_json")])
        .values_panic([
            value(input.key.clone()),
            value(json(&input.value, "value")?),
        ])
        .on_conflict(
            OnConflict::column(Alias::new("key"))
                .update_column(Alias::new("value_json"))
                .to_owned(),
        );
    Statement::query(&query)
}
