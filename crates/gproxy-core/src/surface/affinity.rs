use gproxy_channel_api::{CallerIdentity, SurfaceAction, SurfaceAffinity, SurfaceEntry};
use gproxy_protocol::match_path;
use std::collections::BTreeSet;

use crate::api::Core;
use crate::boundary::RequestCtx;
use crate::control::{Plan, Target};
use crate::error::CoreError;
use crate::host::{CacheBackend, Host};

use super::pin::{AffinityPin, cache_key, cached, value_key};

pub(crate) struct TableMatch {
    pub channel: &'static str,
    pub entry: &'static SurfaceEntry,
    pub params: Vec<(&'static str, String)>,
}

pub(crate) struct Selected {
    pub entry: &'static SurfaceEntry,
    pub params: Vec<(&'static str, String)>,
    pub target: Target,
    pub candidates: Vec<Target>,
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
        let Some((target, pin, pinned)) = pin(core, ctx, identity, matched, &candidates).await?
        else {
            binding_missing = true;
            continue;
        };
        let eligible = if pinned {
            vec![target.clone()]
        } else {
            candidates.iter().map(|target| (*target).clone()).collect()
        };
        let matched = matches
            .into_iter()
            .nth(index)
            .expect("matched index exists");
        return Ok(Selected {
            entry: matched.entry,
            params: matched.params,
            target,
            candidates: eligible,
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

pub(crate) async fn bearer_identity<H: Host>(
    core: &Core<H>,
    ctx: &RequestCtx,
    plan: &Plan,
    matches: &[TableMatch],
) -> Result<Option<CallerIdentity>, CoreError> {
    let Some(token) = bearer_value(ctx) else {
        return Ok(None);
    };
    let mut providers = BTreeSet::new();
    for target in &plan.targets {
        if !providers.insert(target.provider.id) {
            continue;
        }
        let Some(matched) = matches
            .iter()
            .find(|matched| matched.channel == target.provider.channel)
        else {
            continue;
        };
        let SurfaceAffinity::BearerToken { namespace } = matched.entry.affinity else {
            continue;
        };
        let key = super::pin::token_key(target.provider.id, namespace, token);
        let Some(binding) = core
            .host
            .cache()
            .get(&key)
            .await?
            .and_then(super::pin::decode_token)
        else {
            continue;
        };
        if plan.targets.iter().any(|candidate| {
            candidate.provider.id == target.provider.id
                && candidate.credential == binding.credential
        }) {
            return Ok(Some(binding.identity));
        }
    }
    Ok(None)
}

async fn pin<H: Host>(
    core: &Core<H>,
    ctx: &RequestCtx,
    identity: &CallerIdentity,
    matched: &TableMatch,
    candidates: &[&Target],
) -> Result<Option<(Target, Option<AffinityPin>, bool)>, CoreError> {
    let first = candidates.first().ok_or(CoreError::NoCredentials)?;
    match matched.entry.affinity {
        SurfaceAffinity::None => Ok(Some(((*first).clone(), None, false))),
        SurfaceAffinity::Header { name, ttl_secs } => {
            let Some(value) = ctx.headers.get(name) else {
                return Ok(Some(((*first).clone(), None, false)));
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
                None => Ok(Some(((*first).clone(), None, false))),
            }
        }
        SurfaceAffinity::HeaderOrBodyField {
            header,
            body_field,
            ttl_secs,
        } => {
            let body_value = serde_json::from_slice::<serde_json::Value>(&ctx.body)
                .ok()
                .and_then(|body| body.get(body_field).cloned())
                .and_then(value_key);
            let value = body_value.or_else(|| {
                ctx.headers
                    .get(header)
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_owned)
            });
            match value {
                Some(value) => {
                    let key = cache_key(first, identity, "body", body_field, &value);
                    cached(core, first, candidates, key, ttl_secs).await
                }
                None => Ok(Some(((*first).clone(), None, false))),
            }
        }
        SurfaceAffinity::PathParam { name, ttl_secs } => {
            let Some(value) = matched
                .params
                .iter()
                .find_map(|(candidate, value)| (*candidate == name).then_some(value.as_str()))
            else {
                return Ok(Some(((*first).clone(), None, false)));
            };
            let key = cache_key(first, identity, "path", name, value);
            cached(core, first, candidates, key, ttl_secs).await
        }
        SurfaceAffinity::ResponseBodyToken {
            request_body_field,
            ttl_secs,
            ..
        } => {
            let Some(name) = request_body_field else {
                return Ok(Some(((*first).clone(), None, false)));
            };
            let value = serde_json::from_slice::<serde_json::Value>(&ctx.body)
                .ok()
                .and_then(|body| body.get(name).cloned())
                .and_then(value_key);
            match value {
                Some(value) => {
                    let key = cache_key(first, identity, "body", name, &value);
                    cached(core, first, candidates, key, ttl_secs).await
                }
                None => Ok(Some(((*first).clone(), None, false))),
            }
        }
        SurfaceAffinity::BearerToken { namespace } => {
            let token = bearer_value(ctx).ok_or(CoreError::Unauthorized)?;
            let key = super::pin::token_key(first.provider.id, namespace, token);
            let binding = core
                .host
                .cache()
                .get(&key)
                .await?
                .and_then(super::pin::decode_token)
                .ok_or(CoreError::Unauthorized)?;
            if binding.identity.user_id != identity.user_id
                || binding.identity.user_key_id != identity.user_key_id
            {
                return Err(CoreError::Unauthorized);
            }
            let target = candidates
                .iter()
                .find(|target| target.credential == binding.credential)
                .map(|target| (*target).clone())
                .ok_or(CoreError::NoCredentials)?;
            Ok(Some((target, None, true)))
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
            Ok(Some((target, None, true)))
        }
    }
}

fn bearer_value(ctx: &RequestCtx) -> Option<&str> {
    ctx.headers
        .get(http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            let (scheme, token) = value.split_once(' ')?;
            scheme.eq_ignore_ascii_case("bearer").then_some(token)
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            ctx.query.as_deref()?.split('&').find_map(|pair| {
                let (name, value) = pair.split_once('=')?;
                matches!(name, "access_token" | "token" | "key")
                    .then_some(value)
                    .filter(|value| !value.is_empty())
            })
        })
}
