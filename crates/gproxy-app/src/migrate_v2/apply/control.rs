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

    let credentials = data
        .credentials
        .iter()
        .map(|value| {
            Ok(CredentialInput {
                provider_id: id(&context.providers, value.value.provider_id)?,
                label: value.value.label.clone(),
                kind: value.value.kind.clone(),
                envelope: context.cipher.seal(&value.value.stored_secret)?,
                enabled: value.value.enabled,
                weight: unsigned32(value.value.weight, "credential weight")?,
                rpm_limit: value
                    .value
                    .rpm_limit
                    .map(|limit| unsigned32(limit, "credential rpm limit"))
                    .transpose()?,
                tpm_limit: value
                    .value
                    .tpm_limit
                    .map(|limit| unsigned(limit, "credential tpm limit"))
                    .transpose()?,
                proxy_url: value.value.proxy_url.clone(),
                tls_fingerprint: value.value.tls_fingerprint.clone(),
            })
        })
        .collect::<Result<Vec<_>, crate::AppError>>()?;
    context.credentials = mapped(
        context,
        &data.credentials,
        RecordBatch::Credentials(credentials),
    )
    .await?;
    mark(counts, "credentials", data.credentials.len());

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
            super::models::for_route(data, value.id, &value.value.name).and_then(|metadata| {
                Ok(ExposedModelInput {
                    name: value.value.name.clone(),
                    route_id: id(&context.routes, value.id)?,
                    display_name: metadata.display_name,
                    variants: metadata.variants,
                    context_window: metadata.context_window,
                    max_output_tokens: metadata.max_output_tokens,
                    thinking_supported: metadata.thinking_supported,
                    thinking_adaptive_supported: metadata.thinking_adaptive_supported,
                    thinking_enabled_supported: metadata.thinking_enabled_supported,
                    enabled: value.value.enabled,
                })
            })
        })
        .collect::<Result<Vec<_>, crate::AppError>>()?;
    context
        .store
        .insert_record_batch(RecordBatch::ExposedModels(exposed))
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
