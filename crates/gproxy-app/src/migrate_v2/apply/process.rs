use gproxy_store::records::RecordBatch;

use super::{Context, id, mapped, mark};
use crate::migrate_v2::model::SourceData;
use crate::migrate_v2::report::ImportCount;

pub(super) async fn run(
    context: &mut Context<'_>,
    data: &SourceData,
    counts: &mut [ImportCount],
) -> Result<(), crate::AppError> {
    let routing = data
        .routing_rules
        .iter()
        .map(|value| {
            let mut input = value.value.clone();
            input.provider_id = id(&context.providers, input.provider_id)?;
            Ok(input)
        })
        .collect::<Result<Vec<_>, crate::AppError>>()?;
    context
        .store
        .insert_record_batch(RecordBatch::RoutingRules(routing))
        .await?;
    mark(counts, "routing_rules", data.routing_rules.len());

    context.rule_sets = mapped(
        context,
        &data.rule_sets,
        RecordBatch::RuleSets(
            data.rule_sets
                .iter()
                .map(|value| value.value.clone())
                .collect(),
        ),
    )
    .await?;
    mark(counts, "rule_sets", data.rule_sets.len());

    let rules = data
        .rules
        .iter()
        .map(|value| {
            let mut input = value.value.clone();
            input.rule_set_id = id(&context.rule_sets, input.rule_set_id)?;
            Ok(input)
        })
        .collect::<Result<Vec<_>, crate::AppError>>()?;
    context
        .store
        .insert_record_batch(RecordBatch::Rules(rules))
        .await?;
    mark(counts, "rules", data.rules.len());

    let attachments = data
        .provider_rule_sets
        .iter()
        .map(|value| {
            let mut input = value.value.clone();
            input.provider_id = id(&context.providers, input.provider_id)?;
            input.rule_set_id = id(&context.rule_sets, input.rule_set_id)?;
            Ok(input)
        })
        .collect::<Result<Vec<_>, crate::AppError>>()?;
    context
        .store
        .insert_record_batch(RecordBatch::ProviderRuleSets(attachments))
        .await?;
    mark(counts, "provider_rule_sets", data.provider_rule_sets.len());
    Ok(())
}
