use std::collections::{BTreeMap, BTreeSet};

use gproxy_channel_api::CredentialId;
use gproxy_protocol::{Affinity, Operation};

use crate::api::Core;
use crate::control::Plan;
use crate::error::CoreError;
use crate::funnel::FunnelCtx;
use crate::host::Host;
use crate::request::Classified;

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
    ctx: &FunnelCtx,
    status: http::StatusCode,
    body: &[u8],
) {
    if !status.is_success() {
        return;
    }
    let (Some(owner_user_id), Some(key)) = (ctx.owner_user_id, ctx.source_key) else {
        return;
    };
    let Affinity::Resource(kind) = key.operation.spec().affinity else {
        return;
    };
    let Some(bindings) = host.bindings() else {
        return;
    };
    if matches!(
        key.operation,
        Operation::DeleteFile | Operation::DeleteVideo
    ) {
        if let Some((_, id)) = &ctx.resource
            && let Err(error) = bindings
                .delete(ctx.target.provider.id, owner_user_id, kind, id)
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
        return;
    }
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(body) else {
        return;
    };
    let mut resources = Vec::new();
    if value
        .get("id")
        .and_then(serde_json::Value::as_str)
        .is_some()
    {
        resources.push(value.clone());
    }
    for field in ["data", "videos", "files"] {
        if let Some(items) = value.get(field).and_then(serde_json::Value::as_array) {
            resources.extend(items.iter().cloned());
        }
    }
    if resources.is_empty()
        && let Some((_, id)) = &ctx.resource
    {
        let mut summary = value.clone();
        if let Some(object) = summary.as_object_mut() {
            object.insert("id".into(), serde_json::Value::String(id.clone()));
            resources.push(summary);
        }
    }
    for summary in resources {
        let Some(id) = summary.get("id").and_then(serde_json::Value::as_str) else {
            continue;
        };
        if let Err(error) = bindings
            .save(
                ctx.target.provider.id,
                owner_user_id,
                kind,
                id,
                ctx.target.credential,
                summary.clone(),
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
}
