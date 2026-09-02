use std::collections::BTreeMap;

use crate::AdminError;
use crate::dto::{ChannelDto, RoutingImplementationDto};

pub async fn seed_provider_defaults(
    store: &gproxy_store::Store,
    provider_id: i64,
    provider_name: &str,
    channel: &ChannelDto,
) -> Result<(), AdminError> {
    seed_routing_defaults(store, provider_id, channel).await?;
    seed_provider_rule_set(store, provider_id, provider_name).await
}

async fn seed_routing_defaults(
    store: &gproxy_store::Store,
    provider_id: i64,
    channel: &ChannelDto,
) -> Result<(), AdminError> {
    for (sort_order, support) in channel.routing_defaults.iter().enumerate() {
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
    Ok(())
}

pub async fn backfill_provider_defaults(
    store: &gproxy_store::Store,
    channels: &[ChannelDto],
) -> Result<(), AdminError> {
    remove_legacy_claude_default(store).await?;
    let channels = channels
        .iter()
        .map(|channel| (channel.id.as_str(), channel))
        .collect::<BTreeMap<_, _>>();
    for provider in store.control_snapshot().await?.providers {
        if let Some(channel) = channels.get(provider.channel.as_str()) {
            seed_routing_defaults(store, provider.id, channel).await?;
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
    seed_routing_defaults(store, provider_id, channel).await
}

pub async fn seed_provider_rule_set(
    store: &gproxy_store::Store,
    provider_id: i64,
    provider_name: &str,
) -> Result<(), AdminError> {
    let sentinel = format!("gproxy:provider-default:{provider_id}");
    let snapshot = store.control_snapshot().await?;
    let rule_set_id = match snapshot
        .rule_sets
        .iter()
        .find(|set| set.description.as_deref() == Some(sentinel.as_str()))
    {
        Some(set) => set.id,
        None => {
            store
                .insert_rule_set(&gproxy_store::records::RuleSetInput {
                    name: available_rule_set_name(&snapshot.rule_sets, provider_name, provider_id),
                    description: Some(sentinel),
                    enabled: true,
                })
                .await?
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
        .insert_provider_rule_set(&gproxy_store::records::ProviderRuleSetInput {
            provider_id,
            rule_set_id,
            sort_order,
            enabled: true,
        })
        .await?;
    Ok(())
}

pub async fn delete_provider_rule_set(
    store: &gproxy_store::Store,
    provider_id: i64,
) -> Result<(), AdminError> {
    let sentinel = format!("gproxy:provider-default:{provider_id}");
    let snapshot = store.control_snapshot().await?;
    let Some(rule_set) = snapshot
        .rule_sets
        .iter()
        .find(|set| set.description.as_deref() == Some(sentinel.as_str()))
    else {
        return Ok(());
    };
    delete_rule_set(store, &snapshot, rule_set.id).await
}

fn available_rule_set_name(
    rule_sets: &[gproxy_store::records::RuleSetRecord],
    provider_name: &str,
    provider_id: i64,
) -> String {
    let base = format!("{provider_name} · defaults");
    if rule_sets.iter().all(|set| set.name != base) {
        return base;
    }
    let mut suffix = provider_id.to_string();
    loop {
        let candidate = format!("{base} · {suffix}");
        if rule_sets.iter().all(|set| set.name != candidate) {
            return candidate;
        }
        suffix.push('_');
    }
}

async fn remove_legacy_claude_default(store: &gproxy_store::Store) -> Result<(), AdminError> {
    const SENTINEL: &str = "gproxy:channel-default:claudeapi:system-cache";
    let snapshot = store.control_snapshot().await?;
    let Some(rule_set) = snapshot
        .rule_sets
        .iter()
        .find(|set| set.description.as_deref() == Some(SENTINEL))
    else {
        return Ok(());
    };
    delete_rule_set(store, &snapshot, rule_set.id).await
}

async fn delete_rule_set(
    store: &gproxy_store::Store,
    snapshot: &gproxy_store::records::ControlSnapshot,
    rule_set_id: i64,
) -> Result<(), AdminError> {
    for attachment in snapshot
        .provider_rule_sets
        .iter()
        .filter(|attachment| attachment.rule_set_id == rule_set_id)
    {
        store.delete_provider_rule_set(attachment.id).await?;
    }
    for rule in snapshot
        .rules
        .iter()
        .filter(|rule| rule.rule_set_id == rule_set_id)
    {
        store.delete_rule(rule.id).await?;
    }
    store.delete_rule_set(rule_set_id).await?;
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
