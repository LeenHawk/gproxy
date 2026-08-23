use gproxy_channel_api::{CallerIdentity, SurfaceAction, SurfaceAffinity, SurfaceEntry};
use gproxy_protocol::match_path;
use std::collections::BTreeSet;

use crate::api::Core;
use crate::boundary::RequestCtx;
use crate::control::{Plan, Target};
use crate::error::CoreError;
use crate::host::Host;
use crate::surface_pin::{AffinityPin, cache_key, cached, value_key};

pub(crate) struct TableMatch {
    pub channel: &'static str,
    pub entry: &'static SurfaceEntry,
    pub params: Vec<(&'static str, String)>,
}

pub(crate) struct Selected {
    pub entry: &'static SurfaceEntry,
    pub params: Vec<(&'static str, String)>,
    pub target: Target,
    pub pin: Option<AffinityPin>,
}

pub(crate) fn table_matches<H: Host>(core: &Core<H>, ctx: &RequestCtx) -> Vec<TableMatch> {
    let mut matches = Vec::new();
    for channel in core.channels.iter() {
        for entry in channel.surfaces().0 {
            let websocket = matches!(&entry.action, SurfaceAction::ForwardWebSocket(_));
            if entry.method == ctx.method
                && websocket == ctx.upgrade
                && let Some(params) = match_path(entry.pattern, &ctx.path)
            {
                matches.push(TableMatch {
                    channel: channel.descriptor().id,
                    entry,
                    params,
                });
            }
        }
    }
    matches
}

pub(crate) async fn select<H: Host>(
    core: &Core<H>,
    ctx: &RequestCtx,
    identity: &CallerIdentity,
    plan: &Plan,
    matches: Vec<TableMatch>,
) -> Result<Selected, CoreError> {
    let mut providers = BTreeSet::new();
    let mut binding_missing = false;
    for target in &plan.targets {
        if !providers.insert(target.provider.id) {
            continue;
        }
        let Some(index) = matches
            .iter()
            .position(|matched| matched.channel == target.provider.channel)
        else {
            continue;
        };
        let matched = &matches[index];
        let candidates: Vec<_> = plan
            .targets
            .iter()
            .filter(|candidate| candidate.provider.id == target.provider.id)
            .collect();
        let Some((target, pin)) = pin(core, ctx, identity, matched, &candidates).await? else {
            binding_missing = true;
            continue;
        };
        let matched = matches
            .into_iter()
            .nth(index)
            .expect("matched index exists");
        return Ok(Selected {
            entry: matched.entry,
            params: matched.params,
            target,
            pin,
        });
    }
    if binding_missing {
        return Err(CoreError::UnknownRoute("surface binding not found".into()));
    }
    Err(CoreError::UnknownProvider(
        "no resolved provider serves the matched surface".into(),
    ))
}

async fn pin<H: Host>(
    core: &Core<H>,
    ctx: &RequestCtx,
    identity: &CallerIdentity,
    matched: &TableMatch,
    candidates: &[&Target],
) -> Result<Option<(Target, Option<AffinityPin>)>, CoreError> {
    let first = candidates.first().ok_or(CoreError::NoCredentials)?;
    match matched.entry.affinity {
        SurfaceAffinity::None => Ok(Some(((*first).clone(), None))),
        SurfaceAffinity::Header { name, ttl_secs } => {
            let Some(value) = ctx.headers.get(name) else {
                return Ok(Some(((*first).clone(), None)));
            };
            let value = value.to_str().map_err(|_| CoreError::Unsupported)?;
            let key = cache_key(first, identity, "header", name, value);
            cached(core, first, candidates, key, ttl_secs).await
        }
        SurfaceAffinity::BodyField { name, ttl_secs } => {
            let body = serde_json::from_slice::<serde_json::Value>(&ctx.body)
                .map_err(|_| CoreError::Unsupported)?;
            let value = body.get(name).cloned().and_then(value_key);
            match value {
                Some(value) => {
                    let key = cache_key(first, identity, "body", name, &value);
                    cached(core, first, candidates, key, ttl_secs).await
                }
                None => Ok(Some(((*first).clone(), None))),
            }
        }
        SurfaceAffinity::Binding { kind, param } => {
            let id = matched
                .params
                .iter()
                .find_map(|(name, value)| (*name == param).then_some(value.as_str()))
                .ok_or(CoreError::Unsupported)?;
            let binding = core
                .host
                .bindings()
                .expect("surface registration requires a binding store")
                .find(first.provider.id, identity.user_id, kind, id)
                .await?;
            let Some(binding) = binding else {
                return Ok(None);
            };
            let target = candidates
                .iter()
                .find(|target| target.credential == binding.credential)
                .map(|target| (*target).clone())
                .ok_or(CoreError::NoCredentials)?;
            Ok(Some((target, None)))
        }
    }
}
