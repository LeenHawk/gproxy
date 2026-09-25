//! Model name to execution plan.
//!
//! Core does not route: it executes against a provider and a credential set a
//! caller already named. This module owns the other half — exposed model
//! names, the routes they select, the `channel/model` and `provider/model`
//! prefixes, the narrowing an application layer applies, and the order the
//! resulting targets are tried in. The rows live here rather than in
//! `CoreData` because nothing in the engine reads them.
//!
//! # The grammar
//!
//! A model name is resolved by the first rule that matches:
//!
//! | Name | Resolves to |
//! |---|---|
//! | absent | every enabled provider, with no upstream model |
//! | an exposed model name | that route's enabled members |
//! | `channel/model` | the providers of that channel, preferring the ones whose catalog lists `model` |
//! | `provider/model` | that one provider |
//! | anything else | [`SdkError::UnknownModel`] |
//!
//! Exact exposed names are matched before the prefix forms, so an operator can
//! expose the literal name `openai/gpt-5` and have it mean their route. Within
//! the prefix forms **a channel id wins over a provider of the same name**: a
//! channel id is a fixed part of this build and cannot be renamed out of the
//! way, while a provider can always be renamed, so the collision is resolved
//! in favour of the name the operator cannot change.

mod balance;
mod table;

use std::{
    collections::{BTreeSet, HashSet},
    num::NonZeroU32,
    sync::Arc,
};

use gproxy_core::{
    CoreData, CredentialBlocks, CredentialData, CredentialStatus, ProviderData, keys,
};
use gproxy_protocol::OperationKey;
use gproxy_store::entity::routing::route::RouteStrategy;

pub use balance::RotationCounters;
pub use table::{DEFAULT_MAX_ATTEMPTS, MemberSeed, RouteEntry, RoutingTable};

use crate::{SdkError, SdkResult, handle::Gproxy, rt::now_ms};

/// What to resolve, and what the caller is allowed to reach.
///
/// The three narrowing fields are how an application layer keeps callers
/// apart: `allowed_providers` and `allowed_credentials` are intersections, so
/// `None` means "no restriction from this dimension" and an empty set means
/// "nothing is allowed", which resolves to [`SdkError::NoTarget`].
#[derive(Clone, Copy, Debug)]
pub struct ResolveRequest<'a> {
    /// The model name the caller asked for. `None` for operations that name
    /// no model, such as listing models.
    pub model: Option<&'a str>,
    pub operation: OperationKey,
    /// Provider ids this caller may reach, if it is restricted to some.
    pub allowed_providers: Option<&'a BTreeSet<String>>,
    /// Credential ids this caller may spend, if it is restricted to some.
    pub allowed_credentials: Option<&'a BTreeSet<String>>,
    /// Restrict to one channel id, as an ingress mount does.
    pub channel: Option<&'a str>,
    /// A stable session key. When the provider pins credentials to sessions,
    /// its credentials are ordered by a stable hash of this key, so the same
    /// session keeps offering core the same first choice.
    pub affinity_key: Option<&'a str>,
}

impl<'a> ResolveRequest<'a> {
    pub fn new(operation: OperationKey) -> Self {
        Self {
            model: None,
            operation,
            allowed_providers: None,
            allowed_credentials: None,
            channel: None,
            affinity_key: None,
        }
    }

    pub fn model(mut self, model: &'a str) -> Self {
        self.model = Some(model);
        self
    }
}

/// Where a request may be sent, in the order it should be tried.
#[derive(Debug)]
pub struct Plan {
    /// What the name resolved to: the exposed name for a route, the part
    /// after the prefix for `channel/model` and `provider/model`, `None` when
    /// no model was named. It is not a per-target upstream model — a route's
    /// members may each name their own.
    pub resolved_model: Option<String>,
    /// The attempt budget for the whole plan, shared across its targets: the
    /// route's, or the instance-wide default for every other form.
    pub max_attempts: NonZeroU32,
    /// Never empty: an empty result is reported as an error instead.
    pub targets: Vec<Target>,
}

/// One provider, one upstream model and the credentials of that provider this
/// caller may spend. This is exactly what core's `ExecutionTarget` needs, plus
/// the balancing facts the caller may want to log.
#[derive(Clone)]
pub struct Target {
    pub requested_model: Option<String>,
    pub provider: Arc<ProviderData>,
    pub upstream_model: Option<String>,
    /// Non-empty, and every one belongs to `provider`.
    pub credentials: Vec<Arc<CredentialData>>,
    pub tier: u32,
    pub weight: u32,
    /// The route member this target came from, absent for the forms that do
    /// not go through a route.
    pub member_id: Option<String>,
}

impl std::fmt::Debug for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Target")
            .field("provider", &self.provider.entity.id)
            .field("upstream_model", &self.upstream_model)
            .field(
                "credentials",
                &self
                    .credentials
                    .iter()
                    .map(|credential| &credential.id)
                    .collect::<Vec<_>>(),
            )
            .field("tier", &self.tier)
            .field("weight", &self.weight)
            .field("member_id", &self.member_id)
            .finish()
    }
}

/// A candidate before narrowing: which provider, under which upstream model
/// and with which balancing facts.
struct Candidate {
    provider: Arc<ProviderData>,
    upstream_model: Option<String>,
    tier: u32,
    weight: u32,
    member_id: Option<String>,
}

/// What the name matched, before anything is narrowed away.
struct Matched {
    candidates: Vec<Candidate>,
    strategy: RouteStrategy,
    /// The rotation pool these candidates rotate in.
    balance_key: String,
    max_attempts: u32,
    resolved_model: Option<String>,
}

impl<C> Gproxy<C> {
    /// Resolve a model name against the published routing table and the active
    /// execution snapshot.
    ///
    /// Both are pinned for the call, so a reload in the middle cannot produce a
    /// plan that mixes revisions. [`Gproxy::call`] resolves against the same
    /// snapshot it then executes with.
    pub async fn resolve(&self, request: ResolveRequest<'_>) -> SdkResult<Plan> {
        let snapshot = self.0.core.snapshot();
        let routing = self.0.routing.load_full();
        self.resolve_with(&snapshot, &routing, request).await
    }

    pub(crate) async fn resolve_with(
        &self,
        snapshot: &CoreData,
        routing: &RoutingTable,
        request: ResolveRequest<'_>,
    ) -> SdkResult<Plan> {
        let matched = self.match_name(snapshot, routing, request.model)?;
        let named = matched.candidates.len();
        let now = now_ms();
        let mut ranked = Vec::with_capacity(named);
        for mut candidate in matched.candidates {
            let requested_model = candidate.upstream_model.clone();
            if let Some(name) = candidate.upstream_model.as_deref()
                && let Some(model) = candidate
                    .provider
                    .models
                    .iter()
                    .find(|m| m.enabled && m.variant_names().contains(&name))
            {
                candidate.upstream_model = Some(model.upstream_name.clone());
            }
            if let Some(channel) = request.channel
                && candidate.provider.entity.channel != channel
            {
                continue;
            }
            if let Some(allowed) = request.allowed_providers
                && !allowed.contains(&candidate.provider.entity.id)
            {
                continue;
            }
            let Some((credentials, health)) = self
                .usable_credentials(snapshot, &candidate, &request, now)
                .await
            else {
                continue;
            };
            ranked.push(balance::Ranked {
                target: Target {
                    requested_model,
                    provider: candidate.provider,
                    upstream_model: candidate.upstream_model,
                    credentials,
                    tier: candidate.tier,
                    weight: candidate.weight,
                    member_id: candidate.member_id,
                },
                health,
            });
        }
        if ranked.is_empty() {
            return Err(SdkError::NoTarget(request.model.unwrap_or("*").to_owned()));
        }
        Ok(Plan {
            resolved_model: matched.resolved_model,
            max_attempts: NonZeroU32::new(matched.max_attempts).unwrap_or(NonZeroU32::MIN),
            targets: balance::order(
                ranked,
                matched.strategy,
                &matched.balance_key,
                &self.0.rotation,
            ),
        })
    }

    /// Step one: what does this name mean? Narrowing and availability are not
    /// considered here, so a name that means something but has nothing behind
    /// it is reported as `NoTarget` rather than `UnknownModel`.
    fn match_name(
        &self,
        snapshot: &CoreData,
        routing: &RoutingTable,
        model: Option<&str>,
    ) -> SdkResult<Matched> {
        let Some(model) = model else {
            let mut candidates: Vec<Candidate> = snapshot
                .providers
                .values()
                .map(|provider| Candidate {
                    provider: provider.clone(),
                    upstream_model: None,
                    tier: 0,
                    weight: 100,
                    member_id: None,
                })
                .collect();
            candidates.sort_by(|a, b| a.provider.entity.id.cmp(&b.provider.entity.id));
            return Ok(Matched {
                candidates,
                strategy: RouteStrategy::RoundRobin,
                balance_key: "@all".into(),
                max_attempts: routing.default_max_attempts,
                resolved_model: None,
            });
        };

        if let Some((route_id, route)) = routing.route_for(model) {
            let candidates = route
                .members
                .iter()
                .filter_map(|member| {
                    Some(Candidate {
                        // A member of a disabled or deleted provider is not a
                        // candidate: the snapshot only holds enabled ones.
                        provider: snapshot.providers.get(&member.provider_id)?.clone(),
                        upstream_model: Some(member.upstream_model.clone()),
                        tier: member.tier,
                        weight: member.weight,
                        member_id: Some(member.member_id.clone()),
                    })
                })
                .collect();
            return Ok(Matched {
                candidates,
                strategy: route.strategy,
                balance_key: route_id.to_owned(),
                max_attempts: route.max_attempts,
                resolved_model: Some(model.to_owned()),
            });
        }

        let (head, tail) = model
            .split_once('/')
            .filter(|(head, tail)| !head.is_empty() && !tail.is_empty())
            .ok_or_else(|| SdkError::UnknownModel(model.to_owned()))?;

        // A channel id before a provider name: channel ids are fixed by this
        // build and an operator cannot rename one out of the way.
        if self.0.core.channels().get(head).is_some() {
            let of_channel: Vec<&Arc<ProviderData>> = snapshot
                .providers
                .values()
                .filter(|provider| provider.entity.channel == head)
                .collect();
            // Prefer the providers that say they serve this model. When none
            // of them has a catalog entry for it, the catalog is simply not
            // maintained and every provider of the channel is a candidate.
            let listed: Vec<&Arc<ProviderData>> = of_channel
                .iter()
                .copied()
                .filter(|provider| {
                    provider.models.iter().any(|model| {
                        model.enabled
                            && (model.upstream_name == tail
                                || model.variant_names().contains(&tail))
                    })
                })
                .collect();
            let chosen = if listed.is_empty() {
                of_channel
            } else {
                listed
            };
            let mut candidates: Vec<Candidate> = chosen
                .into_iter()
                .map(|provider| Candidate {
                    provider: provider.clone(),
                    upstream_model: Some(tail.to_owned()),
                    tier: 0,
                    weight: 100,
                    member_id: None,
                })
                .collect();
            candidates.sort_by(|a, b| a.provider.entity.id.cmp(&b.provider.entity.id));
            return Ok(Matched {
                candidates,
                strategy: RouteStrategy::RoundRobin,
                balance_key: format!("@channel:{head}"),
                max_attempts: routing.default_max_attempts,
                resolved_model: Some(tail.to_owned()),
            });
        }

        let provider = snapshot
            .providers
            .values()
            .find(|provider| provider.entity.name == head)
            .ok_or_else(|| SdkError::UnknownModel(model.to_owned()))?;
        Ok(Matched {
            candidates: vec![Candidate {
                provider: provider.clone(),
                upstream_model: Some(tail.to_owned()),
                tier: 0,
                weight: 100,
                member_id: None,
            }],
            strategy: RouteStrategy::Failover,
            balance_key: format!("@provider:{}", provider.entity.id),
            max_attempts: routing.default_max_attempts,
            resolved_model: Some(tail.to_owned()),
        })
    }

    /// The credentials of one candidate this caller may spend, and how healthy
    /// they are. `None` drops the candidate entirely.
    ///
    /// Disabled, retired and `Dead` credentials are gone for good — waiting
    /// does not bring them back — so they are removed outright. A block is
    /// temporary, so blocked credentials are removed only while the target has
    /// an unblocked one left; a target whose credentials are *all* blocked is
    /// kept with health rank 1 and ordered behind every healthy target, which
    /// is what makes a rate-limited provider a last resort rather than an
    /// outage.
    async fn usable_credentials(
        &self,
        snapshot: &CoreData,
        candidate: &Candidate,
        request: &ResolveRequest<'_>,
        now_ms: i64,
    ) -> Option<(Vec<Arc<CredentialData>>, u8)> {
        if matches!(
            gproxy_core::convert::route(&candidate.provider, request.operation),
            Ok(gproxy_core::convert::Route::Local)
        ) && !(candidate
            .provider
            .channel
            .local_operations()
            .contains(&request.operation.operation)
            && candidate
                .provider
                .channel
                .native_dialects(
                    gproxy_channel::channel::ProviderView {
                        id: &candidate.provider.entity.id,
                        channel: &candidate.provider.entity.channel,
                        base_url: candidate.provider.entity.base_url.as_deref(),
                        config: &candidate.provider.entity.config,
                    },
                    request.operation.operation,
                )
                .contains(&request.operation.dialect))
        {
            return Some((Vec::new(), 0));
        }
        let model = candidate.upstream_model.as_deref();
        let operation = request.operation.operation;
        let mut usable = Vec::new();
        let mut blocked = Vec::new();
        for id in &candidate.provider.credential_ids {
            if let Some(allowed) = request.allowed_credentials
                && !allowed.contains(id)
            {
                continue;
            }
            let Some(credential) = snapshot.credentials.get(id) else {
                continue;
            };
            if !credential.enabled || credential.state.is_retired() {
                continue;
            }
            if credential.state.load().status == CredentialStatus::Dead {
                continue;
            }
            if self
                .blocks(&credential.provider_id, &credential.id)
                .await
                .blocked_by(model, operation, now_ms)
                .is_some()
            {
                blocked.push(credential.clone());
            } else {
                usable.push(credential.clone());
            }
        }
        let (mut credentials, health) = if !usable.is_empty() {
            (usable, 0)
        } else if !blocked.is_empty() {
            (blocked, 1)
        } else {
            return None;
        };
        credentials.sort_by(|a, b| a.id.cmp(&b.id));
        self.apply_affinity(candidate, request.affinity_key, &mut credentials);
        Some((credentials, health))
    }

    /// Core reads the same blocks before every attempt, so a stale answer here
    /// only costs a wasted position in the order. A cache that cannot answer
    /// is therefore treated as "nothing is blocked": refusing to route because
    /// the cache is down would turn a soft signal into an outage.
    async fn blocks(&self, provider_id: &str, credential_id: &str) -> CredentialBlocks {
        match self
            .0
            .cache
            .get(&keys::credential_blocks(provider_id, credential_id))
            .await
        {
            Ok(entry) => entry
                .and_then(|entry| serde_json::from_slice(&entry.value).ok())
                .unwrap_or_default(),
            Err(error) => {
                tracing::warn!(
                    %provider_id,
                    %credential_id,
                    %error,
                    "credential blocks unreadable; treating the credential as unblocked"
                );
                CredentialBlocks::default()
            }
        }
    }

    /// Offer a session-pinned provider its credentials in an order that
    /// depends only on the session and the provider, so every instance
    /// proposes the same first choice. Core still owns the actual pin — it
    /// reads the durable affinity from the cache and only falls back to the
    /// order it was handed.
    fn apply_affinity(
        &self,
        candidate: &Candidate,
        affinity_key: Option<&str>,
        credentials: &mut [Arc<CredentialData>],
    ) {
        let Some(key) = affinity_key else { return };
        if !candidate.provider.session_affinity {
            return;
        }
        if credentials.len() < 2 {
            return;
        }
        let slot = balance::stable_slot(key, &candidate.provider.entity.id);
        let offset = usize::try_from(slot % credentials.len() as u64).unwrap_or(0);
        credentials.rotate_left(offset);
    }
}

impl Plan {
    /// The distinct provider ids this plan reaches, in plan order, for a
    /// caller that wants to log the fan-out without holding on to the targets.
    pub fn provider_ids(&self) -> Vec<&str> {
        let mut seen = HashSet::new();
        self.targets
            .iter()
            .map(|target| target.provider.entity.id.as_str())
            .filter(|id| seen.insert(*id))
            .collect()
    }
}
