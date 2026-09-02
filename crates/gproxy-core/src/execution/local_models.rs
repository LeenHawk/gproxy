use std::collections::BTreeMap;

use gproxy_channel_api::{Channel, ModelInfo};

use crate::api::Core;
use crate::control::Target;
use crate::host::{CredentialStore, Host};

pub(super) fn route_is_local(
    channel: &dyn Channel,
    target: &Target,
    key: gproxy_protocol::OperationKey,
) -> bool {
    let declared = channel
        .routing_table()
        .iter()
        .find(|support| support.source == key);
    if declared.is_some_and(|support| {
        support.action == gproxy_channel_api::ChannelRouteAction::Unsupported
    }) {
        return false;
    }
    match crate::routing::decide(&target.rules.routing, key) {
        Some(crate::routing::RoutingDecision::Local) => true,
        Some(_) => false,
        None => declared
            .is_some_and(|support| support.action == gproxy_channel_api::ChannelRouteAction::Local),
    }
}

pub(super) async fn run<H: Host>(
    core: &Core<H>,
    channel: &dyn Channel,
    targets: &[Target],
    namespace: bool,
) -> Vec<ModelInfo> {
    let mut models = BTreeMap::<String, ModelInfo>::new();
    for target in targets {
        let Ok(current) = core.host.credentials().load(target.credential).await else {
            continue;
        };
        if channel.local_models(&current.secret).is_none() {
            continue;
        }
        let Ok(fresh) = super::credential::load_fresh(
            core.host.as_ref(),
            channel,
            target.credential,
            &target.provider,
        )
        .await
        else {
            continue;
        };
        let Some(local) = channel.local_models(&fresh.secret) else {
            continue;
        };
        for mut model in local {
            if namespace {
                model.id = format!("{}/{}", target.provider.name, model.id);
            }
            match models.entry(model.id.clone()) {
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert(model);
                }
                std::collections::btree_map::Entry::Occupied(mut entry) => {
                    merge(entry.get_mut(), model);
                }
            }
        }
    }
    models.into_values().collect()
}

fn merge(current: &mut ModelInfo, incoming: ModelInfo) {
    if current.display_name.is_none() {
        current.display_name = incoming.display_name;
    }
    if current.context_window.is_none() {
        current.context_window = incoming.context_window;
    }
    if current.max_output_tokens.is_none() {
        current.max_output_tokens = incoming.max_output_tokens;
    }
    if current.thinking_supported.is_none() {
        current.thinking_supported = incoming.thinking_supported;
    }
    if current.thinking_adaptive_supported.is_none() {
        current.thinking_adaptive_supported = incoming.thinking_adaptive_supported;
    }
    if current.thinking_enabled_supported.is_none() {
        current.thinking_enabled_supported = incoming.thinking_enabled_supported;
    }
}
