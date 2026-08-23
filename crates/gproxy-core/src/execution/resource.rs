use std::collections::{BTreeMap, BTreeSet};

use gproxy_channel_api::{Channel, CredentialId, ResourceCtx, ResourceMutation};

use crate::api::Core;
use crate::control::Plan;
use crate::error::CoreError;
use crate::funnel::FunnelCtx;
use crate::host::Host;

use super::request::Classified;

pub(crate) async fn pins<H: Host>(
    core: &Core<H>,
    plan: &Plan,
    classified: &Classified,
    owner_user_id: i64,
) -> Result<Option<BTreeMap<i64, CredentialId>>, CoreError> {
    let Some((kind, id)) = classified.resource() else {
        return Ok(None);
    };
    let bindings = core
        .host
        .bindings()
        .ok_or_else(|| CoreError::Internal("resource affinity requires a binding store".into()))?;
    let mut providers = BTreeMap::<i64, Vec<_>>::new();
    for target in &plan.targets {
        if crate::attempt::support(core, target, classified.key)?.is_some() {
            providers
                .entry(target.provider.id)
                .or_default()
                .push(target);
        }
    }
    let mut pins = BTreeMap::new();
    for (provider_id, targets) in providers {
        let credentials = targets
            .iter()
            .map(|target| target.credential)
            .collect::<BTreeSet<_>>();
        let credential = match bindings.find(provider_id, owner_user_id, kind, id).await? {
            Some(binding) if credentials.contains(&binding.credential) => Some(binding.credential),
            Some(_) => None,
            None if credentials.len() == 1 => credentials.first().copied(),
            None => None,
        };
        if let Some(credential) = credential {
            pins.insert(provider_id, credential);
        }
    }
    if pins.is_empty() {
        return Err(CoreError::UnknownRoute(format!(
            "{kind} resource `{id}` is not bound to an eligible credential"
        )));
    }
    Ok(Some(pins))
}

pub(crate) async fn observe<H: Host>(
    host: &H,
    channel: &dyn Channel,
    ctx: &FunnelCtx,
    status: http::StatusCode,
    headers: &http::HeaderMap,
    body: &[u8],
) {
    if !status.is_success() {
        return;
    }
    let (Some(owner_user_id), Some(key)) = (ctx.owner_user_id, ctx.key) else {
        return;
    };
    let mutations = match channel.resource_mutations(ResourceCtx {
        key,
        request_resource: ctx.resource.as_ref().map(|(kind, id)| (*kind, id.as_str())),
        request_body: &ctx.request_body,
        response_headers: headers,
        response_body: body,
    }) {
        Ok(mutations) => mutations,
        Err(error) => {
            tracing::error!(request_id = %ctx.request_id, error = %error, "resource observation failed");
            return;
        }
    };
    let Some(bindings) = host.bindings() else {
        return;
    };
    for mutation in mutations {
        match mutation {
            ResourceMutation::Save { kind, id, summary } => {
                save_binding(bindings, ctx, owner_user_id, kind, &id, summary).await;
            }
            ResourceMutation::Delete { kind, id } => {
                if let Err(error) = bindings
                    .delete(ctx.target.provider.id, owner_user_id, kind, &id)
                    .await
                {
                    tracing::error!(
                        request_id = %ctx.request_id,
                        resource_kind = kind,
                        resource_id = id,
                        error = %error,
                        "resource binding delete failed"
                    );
                }
            }
        }
    }
}

async fn save_binding(
    bindings: &dyn gproxy_channel_api::BindingStore,
    ctx: &FunnelCtx,
    owner_user_id: i64,
    kind: &'static str,
    id: &str,
    summary: serde_json::Value,
) {
    if let Err(error) = bindings
        .save(
            ctx.target.provider.id,
            owner_user_id,
            kind,
            id,
            ctx.target.credential,
            summary,
        )
        .await
    {
        tracing::error!(
            request_id = %ctx.request_id,
            resource_kind = kind,
            resource_id = id,
            error = %error,
            "resource binding save failed"
        );
    }
}
