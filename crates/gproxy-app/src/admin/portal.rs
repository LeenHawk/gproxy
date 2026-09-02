use std::collections::{BTreeMap, BTreeSet};

use gproxy_admin::PortalIdentity;
use gproxy_admin::dto::{PortalModelCapabilityDto, PortalModelDto};
use gproxy_channel_api::CallerIdentity;
use gproxy_core::{ControlPlane, RoutingMode};

use crate::AppHandle;

pub(super) fn models(handle: &AppHandle, identity: &PortalIdentity) -> Vec<PortalModelDto> {
    let control = &handle.inner.host.services.control;
    let snapshot = control.current();
    let caller = caller(identity);
    let descriptors = handle
        .inner
        .core
        .channel_descriptors()
        .map(|descriptor| (descriptor.id, descriptor))
        .collect::<BTreeMap<_, _>>();
    let mut names = snapshot
        .exposed_models
        .iter()
        .filter(|model| model.enabled)
        .map(|model| model.name.clone())
        .collect::<BTreeSet<_>>();
    names.extend(
        snapshot
            .aliases
            .iter()
            .filter(|alias| alias.enabled && alias.provider_id.is_none())
            .map(|alias| alias.alias.clone()),
    );

    names
        .into_iter()
        .filter_map(|name| {
            let plan = control
                .resolve(
                    Some(&name),
                    &RoutingMode::Aggregated,
                    Some(caller.user_key_id),
                )
                .ok()?;
            let mut capabilities = BTreeMap::new();
            for target in &plan.targets {
                let descriptor = descriptors.get(target.provider.channel.as_str())?;
                for support in descriptor.supports {
                    if crate::host::authorize(&snapshot, &caller, Some(support.source), &plan)
                        .is_ok()
                    {
                        let capability = PortalModelCapabilityDto {
                            source: support.source.kind.id().into(),
                            operation: support.source.operation.id().into(),
                            group: support.source.operation.group().id().into(),
                        };
                        capabilities
                            .entry((
                                capability.source.clone(),
                                capability.operation.clone(),
                                capability.group.clone(),
                            ))
                            .or_insert(capability);
                    }
                }
            }
            (!capabilities.is_empty()).then(|| PortalModelDto {
                name,
                capabilities: capabilities.into_values().collect(),
            })
        })
        .collect()
}

fn caller(identity: &PortalIdentity) -> CallerIdentity {
    CallerIdentity {
        user_id: identity.user_id,
        user_key_id: -identity.user_id,
        org_id: identity.org_id,
        team_id: identity.team_id,
    }
}
