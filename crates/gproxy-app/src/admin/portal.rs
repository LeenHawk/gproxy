use std::collections::{BTreeMap, BTreeSet};

use gproxy_admin::dto::{PortalModelCapabilityDto, PortalModelDto};
use gproxy_admin::{AdminError, PortalIdentity};
use gproxy_channel_api::CallerIdentity;
use gproxy_core::{ControlPlane, RoutingMode};

use crate::AppHandle;

pub(super) fn identity(
    handle: &AppHandle,
    headers: &http::HeaderMap,
) -> Result<PortalIdentity, AdminError> {
    let caller = crate::host::authenticate_headers(&handle.inner.host, headers)
        .map_err(|_| AdminError::Unauthorized)?;
    let snapshot = handle.inner.host.services.control.current();
    let key = snapshot
        .user_keys
        .iter()
        .find(|key| key.id == caller.user_key_id)
        .ok_or(AdminError::Unauthorized)?;
    let user = snapshot
        .users
        .iter()
        .find(|user| user.id == caller.user_id)
        .ok_or(AdminError::Unauthorized)?;
    Ok(PortalIdentity {
        user_id: caller.user_id,
        user_key_id: caller.user_key_id,
        org_id: caller.org_id,
        team_id: caller.team_id,
        user_name: user.name.clone(),
        key_prefix: key.prefix.clone(),
        key_label: key.label.clone(),
        expires_at: key.expires_at,
    })
}

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
        user_key_id: identity.user_key_id,
        org_id: identity.org_id,
        team_id: identity.team_id,
    }
}
