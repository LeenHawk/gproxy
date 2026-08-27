use sea_query::{Alias, Expr, ExprTrait};

use crate::StoreError;
use crate::backend::Statement;
use crate::query::common::{decimal, json, unsigned, unsigned32, update, value};
use crate::records::{
    AliasInput, CredentialUpdateInput, ExposedModelInput, PriceRateInput, PriceRuleInput,
    ProviderInput, RouteInput, RouteMemberInput,
};

pub(crate) fn update_provider(id: i64, input: &ProviderInput) -> Result<Statement, StoreError> {
    update(
        "providers",
        id,
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

pub(crate) fn update_credential(
    id: i64,
    input: &CredentialUpdateInput,
) -> Result<Statement, StoreError> {
    let mut columns = vec![
        "provider_id",
        "label",
        "kind",
        "enabled",
        "weight",
        "rpm_limit",
        "tpm_limit",
        "proxy_url",
        "tls_fingerprint",
        "version",
    ];
    let mut values = vec![
        value(input.provider_id),
        value(input.label.clone()),
        value(input.kind.clone()),
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
        Expr::col(Alias::new("version")).add(1),
    ];
    if let Some(envelope) = &input.envelope {
        columns.extend(["ciphertext", "wrapped_key", "payload_nonce", "key_nonce"]);
        values.extend([
            value(envelope.ciphertext.clone()),
            value(envelope.wrapped_key.clone()),
            value(envelope.payload_nonce.clone()),
            value(envelope.key_nonce.clone()),
        ]);
    }
    update("credentials", id, &columns, values)
}

pub(crate) fn update_route(id: i64, input: &RouteInput) -> Result<Statement, StoreError> {
    update(
        "routes",
        id,
        &["name", "max_attempts", "enabled"],
        vec![
            value(input.name.clone()),
            value(unsigned32(input.max_attempts)),
            value(input.enabled),
        ],
    )
}

pub(crate) fn update_route_member(
    id: i64,
    input: &RouteMemberInput,
) -> Result<Statement, StoreError> {
    update(
        "route_members",
        id,
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

pub(crate) fn update_alias(id: i64, input: &AliasInput) -> Result<Statement, StoreError> {
    update(
        "aliases",
        id,
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

pub(crate) fn update_exposed_model(
    id: i64,
    input: &ExposedModelInput,
) -> Result<Statement, StoreError> {
    update(
        "exposed_models",
        id,
        &[
            "name",
            "route_id",
            "display_name",
            "variants_json",
            "context_window",
            "max_output_tokens",
            "thinking_supported",
            "thinking_adaptive_supported",
            "thinking_enabled_supported",
            "enabled",
        ],
        vec![
            value(input.name.clone()),
            value(input.route_id),
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
            value(input.enabled),
        ],
    )
}

pub(crate) fn update_price_rule(id: i64, input: &PriceRuleInput) -> Result<Statement, StoreError> {
    crate::records::parse_price_tiers(input.tiers.as_ref())?;
    update(
        "price_rules",
        id,
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

pub(crate) fn update_price_rate(id: i64, input: &PriceRateInput) -> Result<Statement, StoreError> {
    update(
        "price_rates",
        id,
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
