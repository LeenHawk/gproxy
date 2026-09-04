use gproxy_store::records::{
    AliasInput, CredentialInput, ExposedModelInput, PriceRateInput, PriceRuleInput, RecordBatch,
};

use super::{Context, id, mapped, mark, optional, unsigned, unsigned32};
use crate::migrate_v2::model::SourceData;
use crate::migrate_v2::report::ImportCount;

pub(super) async fn base(
    context: &mut Context<'_>,
    data: &SourceData,
    counts: &mut [ImportCount],
) -> Result<(), crate::AppError> {
    context.providers = mapped(
        context,
        &data.providers,
        RecordBatch::Providers(
            data.providers
                .iter()
                .map(|value| value.value.clone())
                .collect(),
        ),
    )
    .await?;
    mark(counts, "providers", data.providers.len());
    let tombstone_providers = mapped(
        context,
        &data.usage_tombstone_providers,
        RecordBatch::Providers(
            data.usage_tombstone_providers
                .iter()
                .map(|value| value.value.clone())
                .collect(),
        ),
    )
    .await?;
    context.providers.extend(tombstone_providers);
    mark(
        counts,
        "usage_provider_tombstones",
        data.usage_tombstone_providers.len(),
    );

    let credentials = data
        .credentials
        .iter()
        .map(|value| credential(context, &value.value))
        .collect::<Result<Vec<_>, crate::AppError>>()?;
    context.credentials = mapped(
        context,
        &data.credentials,
        RecordBatch::Credentials(credentials),
    )
    .await?;
    mark(counts, "credentials", data.credentials.len());
    let tombstone_credentials = data
        .usage_tombstone_credentials
        .iter()
        .map(|value| credential(context, &value.value))
        .collect::<Result<Vec<_>, crate::AppError>>()?;
    let tombstone_credentials = mapped(
        context,
        &data.usage_tombstone_credentials,
        RecordBatch::Credentials(tombstone_credentials),
    )
    .await?;
    context.credentials.extend(tombstone_credentials);
    mark(
        counts,
        "usage_credential_tombstones",
        data.usage_tombstone_credentials.len(),
    );

    context.routes = mapped(
        context,
        &data.routes,
        RecordBatch::Routes(
            data.routes
                .iter()
                .map(|value| value.value.clone())
                .collect(),
        ),
    )
    .await?;
    mark(counts, "routes", data.routes.len());
    let members = data
        .route_members
        .iter()
        .map(|value| {
            let mut input = value.value.clone();
            input.route_id = id(&context.routes, input.route_id)?;
            input.provider_id = id(&context.providers, input.provider_id)?;
            Ok(input)
        })
        .collect::<Result<Vec<_>, crate::AppError>>()?;
    context
        .store
        .insert_record_batch(RecordBatch::RouteMembers(members))
        .await?;
    mark(counts, "route_members", data.route_members.len());

    let exposed = data
        .routes
        .iter()
        .map(|value| {
            Ok(ExposedModelInput {
                name: value.value.name.clone(),
                route_id: id(&context.routes, value.id)?,
                enabled: value.value.enabled,
            })
        })
        .collect::<Result<Vec<_>, crate::AppError>>()?;
    context
        .store
        .insert_record_batch(RecordBatch::ExposedModels(exposed))
        .await?;

    // v2 and v3 store this at the same grain, so the rows carry across unchanged.
    let provider_models = data
        .provider_models
        .iter()
        .map(|value| {
            let legacy = &value.value;
            Ok(gproxy_store::records::ProviderModelInput {
                provider_id: id(&context.providers, legacy.provider_id)?,
                model_id: legacy.model_id.clone(),
                display_name: legacy.display_name.clone(),
                variants: legacy.variants.clone(),
                context_window: legacy.context_window,
                max_output_tokens: legacy.max_output_tokens,
                thinking_supported: legacy.thinking_supported,
                thinking_adaptive_supported: legacy.thinking_adaptive_supported,
                thinking_enabled_supported: legacy.thinking_enabled_supported,
                enabled: legacy.enabled,
            })
        })
        .collect::<Result<Vec<_>, crate::AppError>>()?;
    mark(counts, "provider_models", provider_models.len());
    context
        .store
        .insert_record_batch(RecordBatch::ProviderModels(provider_models))
        .await?;

    let provider_names = data
        .providers
        .iter()
        .map(|value| (value.value.name.as_str(), value.id))
        .collect::<std::collections::BTreeMap<_, _>>();
    let aliases = data
        .aliases
        .iter()
        .map(|value| {
            Ok(AliasInput {
                alias: value.value.alias.clone(),
                target: value.value.target.clone(),
                provider_id: if value.value.provider == "*" {
                    None
                } else {
                    Some(id(
                        &context.providers,
                        provider_names[&value.value.provider.as_str()],
                    )?)
                },
                priority: value.value.sort_order,
                enabled: value.value.enabled,
            })
        })
        .collect::<Result<Vec<_>, crate::AppError>>()?;
    context
        .store
        .insert_record_batch(RecordBatch::Aliases(aliases))
        .await?;
    mark(counts, "aliases", data.aliases.len());
    Ok(())
}

fn credential(
    context: &Context<'_>,
    value: &crate::migrate_v2::model::Credential,
) -> Result<CredentialInput, crate::AppError> {
    Ok(CredentialInput {
        provider_id: id(&context.providers, value.provider_id)?,
        label: value.label.clone(),
        kind: value.kind.clone(),
        envelope: context.cipher.seal(&value.stored_secret)?,
        enabled: value.enabled,
        weight: unsigned32(value.weight, "credential weight")?,
        rpm_limit: value
            .rpm_limit
            .map(|limit| unsigned32(limit, "credential rpm limit"))
            .transpose()?,
        tpm_limit: value
            .tpm_limit
            .map(|limit| unsigned(limit, "credential tpm limit"))
            .transpose()?,
        proxy_url: value.proxy_url.clone(),
        tls_fingerprint: value.tls_fingerprint.clone(),
    })
}

pub(super) async fn pricing(
    context: &mut Context<'_>,
    data: &SourceData,
    counts: &mut [ImportCount],
) -> Result<(), crate::AppError> {
    let rules = data
        .price_rules
        .iter()
        .map(|value| {
            Ok(PriceRuleInput {
                provider_id: optional(&context.providers, value.value.provider_id)?,
                model_pattern: match value.value.match_type.as_str() {
                    "exact" => value.value.model_match.clone(),
                    "contains" => format!("*{}*", value.value.model_match),
                    _ => {
                        return Err(crate::AppError::Migration(
                            "invalid price match type after validation".into(),
                        ));
                    }
                },
                tiers: value.value.tiers.clone(),
                priority: if value.value.match_type == "exact" {
                    0
                } else {
                    1_000_000_i64.saturating_sub(
                        i64::try_from(value.value.model_match.len()).unwrap_or(i64::MAX),
                    )
                },
                enabled: value.value.enabled,
            })
        })
        .collect::<Result<Vec<_>, crate::AppError>>()?;
    context.price_rules =
        mapped(context, &data.price_rules, RecordBatch::PriceRules(rules)).await?;
    mark(counts, "price_rules", data.price_rules.len());

    let rates = data
        .price_rates
        .iter()
        .map(|value| {
            Ok(PriceRateInput {
                rule_id: id(&context.price_rules, value.value.rule_id)?,
                metric: match value.value.metric.as_str() {
                    "cache_read_tokens" => "cached_input_tokens".into(),
                    other => other.into(),
                },
                unit_size: unsigned(value.value.unit_size, "price unit size")?,
                price: value.value.price,
                conditions: value.value.conditions.clone(),
                priority: value.value.sort_order,
            })
        })
        .collect::<Result<Vec<_>, crate::AppError>>()?;
    context
        .store
        .insert_record_batch(RecordBatch::PriceRates(rates))
        .await?;
    mark(counts, "price_rates", data.price_rates.len());
    Ok(())
}
