use sea_query::{Alias, Expr, ExprTrait, Query};

use crate::StoreError;
use crate::backend::Statement;
use crate::query::common::{insert, value};
use crate::records::ProviderModelInput;

const TABLES: [&str; 5] = [
    "provider_model_modalities",
    "provider_model_parameters",
    "provider_model_reasoning_levels",
    "provider_model_service_tiers",
    "provider_model_methods",
];

pub(crate) fn replace_model_metadata(
    input: &ProviderModelInput,
) -> Result<Vec<Statement>, StoreError> {
    let mut statements = TABLES
        .into_iter()
        .map(|table| delete_for(table, input.provider_id, &input.model_id))
        .collect::<Result<Vec<_>, _>>()?;
    for (direction, values) in [
        ("input", input.metadata.input_modalities.as_ref()),
        ("output", input.metadata.output_modalities.as_ref()),
    ] {
        for (sort_order, modality) in values.into_iter().flatten().enumerate() {
            statements.push(insert(
                "provider_model_modalities",
                &[
                    "provider_id",
                    "model_id",
                    "direction",
                    "modality",
                    "sort_order",
                ],
                vec![
                    value(input.provider_id),
                    value(input.model_id.clone()),
                    value(direction),
                    value(modality.clone()),
                    value(i64::try_from(sort_order).unwrap_or(i64::MAX)),
                ],
            )?);
        }
    }
    for (sort_order, parameter) in input
        .metadata
        .supported_parameters
        .iter()
        .flatten()
        .enumerate()
    {
        statements.push(insert(
            "provider_model_parameters",
            &["provider_id", "model_id", "parameter", "sort_order"],
            vec![
                value(input.provider_id),
                value(input.model_id.clone()),
                value(parameter.clone()),
                value(i64::try_from(sort_order).unwrap_or(i64::MAX)),
            ],
        )?);
    }
    for (sort_order, level) in input.metadata.reasoning_levels.iter().flatten().enumerate() {
        statements.push(insert(
            "provider_model_reasoning_levels",
            &[
                "provider_id",
                "model_id",
                "effort",
                "description",
                "sort_order",
            ],
            vec![
                value(input.provider_id),
                value(input.model_id.clone()),
                value(level.effort.clone()),
                value(level.description.clone()),
                value(i64::try_from(sort_order).unwrap_or(i64::MAX)),
            ],
        )?);
    }
    for (sort_order, tier) in input.metadata.service_tiers.iter().flatten().enumerate() {
        statements.push(insert(
            "provider_model_service_tiers",
            &[
                "provider_id",
                "model_id",
                "tier_id",
                "name",
                "description",
                "sort_order",
            ],
            vec![
                value(input.provider_id),
                value(input.model_id.clone()),
                value(tier.id.clone()),
                value(tier.name.clone()),
                value(tier.description.clone()),
                value(i64::try_from(sort_order).unwrap_or(i64::MAX)),
            ],
        )?);
    }
    for (kind, values) in [
        ("generation", input.metadata.generation_methods.as_ref()),
        ("action", input.metadata.supported_actions.as_ref()),
    ] {
        for (sort_order, method) in values.into_iter().flatten().enumerate() {
            statements.push(insert(
                "provider_model_methods",
                &["provider_id", "model_id", "kind", "method", "sort_order"],
                vec![
                    value(input.provider_id),
                    value(input.model_id.clone()),
                    value(kind),
                    value(method.clone()),
                    value(i64::try_from(sort_order).unwrap_or(i64::MAX)),
                ],
            )?);
        }
    }
    Ok(statements)
}

pub(crate) fn delete_model_metadata(
    provider_id: i64,
    model_id: &str,
) -> Result<Vec<Statement>, StoreError> {
    TABLES
        .into_iter()
        .map(|table| delete_for(table, provider_id, model_id))
        .collect()
}

fn delete_for(
    table: &'static str,
    provider_id: i64,
    model_id: &str,
) -> Result<Statement, StoreError> {
    let mut query = Query::delete();
    query
        .from_table(Alias::new(table))
        .and_where(Expr::col(Alias::new("provider_id")).eq(provider_id))
        .and_where(Expr::col(Alias::new("model_id")).eq(model_id));
    Statement::query(&query)
}
