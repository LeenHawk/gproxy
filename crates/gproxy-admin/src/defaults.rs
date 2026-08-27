use std::collections::BTreeMap;

use crate::AdminError;
use crate::dto::{ChannelDto, RoutingImplementationDto};

pub async fn seed_provider_defaults(
    store: &gproxy_store::Store,
    provider_id: i64,
    channel: &ChannelDto,
) -> Result<(), AdminError> {
    for (sort_order, support) in channel.supports.iter().enumerate() {
        let transform = support.implementation == RoutingImplementationDto::TransformTo;
        store
            .insert_routing_default(&gproxy_store::records::RoutingRuleInput {
                provider_id,
                operation: support.operation.clone(),
                kind: support.source.clone(),
                implementation: implementation(support.implementation).into(),
                dest_operation: transform.then(|| support.target_operation.clone()),
                dest_kind: transform.then(|| support.target.clone()),
                sort_order: sort_order.try_into().unwrap_or(i64::MAX),
                enabled: true,
            })
            .await?;
    }
    seed_rule_set(store, provider_id, channel).await
}

pub async fn backfill_provider_defaults(
    store: &gproxy_store::Store,
    channels: &[ChannelDto],
) -> Result<(), AdminError> {
    let channels = channels
        .iter()
        .map(|channel| (channel.id.as_str(), channel))
        .collect::<BTreeMap<_, _>>();
    for provider in store.control_snapshot().await?.providers {
        if let Some(channel) = channels.get(provider.channel.as_str()) {
            seed_provider_defaults(store, provider.id, channel).await?;
        }
    }
    Ok(())
}

pub async fn reset_provider_defaults(
    store: &gproxy_store::Store,
    provider_id: i64,
    channel: &ChannelDto,
) -> Result<(), AdminError> {
    store.delete_provider_routing_rules(provider_id).await?;
    if let Some(default) = &channel.default_rule_set {
        let snapshot = store.control_snapshot().await?;
        if let Some(set) = snapshot.rule_sets.iter().find(|set| {
            set.name == default.name
                && set.description.as_deref() == Some(default.description.as_str())
        }) {
            for attachment in snapshot.provider_rule_sets.iter().filter(|attachment| {
                attachment.provider_id == provider_id && attachment.rule_set_id == set.id
            }) {
                store.delete_provider_rule_set(attachment.id).await?;
            }
        }
    }
    seed_provider_defaults(store, provider_id, channel).await
}

async fn seed_rule_set(
    store: &gproxy_store::Store,
    provider_id: i64,
    channel: &ChannelDto,
) -> Result<(), AdminError> {
    let Some(default) = &channel.default_rule_set else {
        return Ok(());
    };
    let snapshot = store.control_snapshot().await?;
    let rule_set_id = match snapshot.rule_sets.iter().find(|set| {
        set.name == default.name && set.description.as_deref() == Some(default.description.as_str())
    }) {
        Some(set) => set.id,
        None => {
            let id = store
                .insert_rule_set(&gproxy_store::records::RuleSetInput {
                    name: default.name.clone(),
                    description: Some(default.description.clone()),
                    enabled: true,
                })
                .await?;
            for rule in &default.rules {
                store
                    .insert_rule(&gproxy_store::records::RuleInput {
                        rule_set_id: id,
                        kind: rule.kind.clone(),
                        config: rule.config.clone(),
                        filter_model_pattern: None,
                        filter_operations: rule.filter_operations.clone(),
                        filter_header_pattern: None,
                        sort_order: rule.sort_order,
                        enabled: true,
                    })
                    .await?;
            }
            id
        }
    };
    let snapshot = store.control_snapshot().await?;
    if snapshot.provider_rule_sets.iter().any(|attachment| {
        attachment.provider_id == provider_id && attachment.rule_set_id == rule_set_id
    }) {
        return Ok(());
    }
    let sort_order = snapshot
        .provider_rule_sets
        .iter()
        .filter(|attachment| attachment.provider_id == provider_id)
        .map(|attachment| attachment.sort_order)
        .max()
        .unwrap_or(-1)
        + 1;
    store
        .insert_provider_rule_set_default(&gproxy_store::records::ProviderRuleSetInput {
            provider_id,
            rule_set_id,
            sort_order,
            enabled: true,
        })
        .await?;
    Ok(())
}

fn implementation(value: RoutingImplementationDto) -> &'static str {
    match value {
        RoutingImplementationDto::Passthrough => "passthrough",
        RoutingImplementationDto::TransformTo => "transform_to",
        RoutingImplementationDto::Local => "local",
        RoutingImplementationDto::Unsupported => "unsupported",
    }
}
