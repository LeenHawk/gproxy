//! Which model names this caller may call.
//!
//! Two kinds of name are listed, and they are the two a client can actually
//! type:
//!
//! 1. every **exposed model** — the names an operator published through the
//!    routing table;
//! 2. every **`channel/model`** form — a registered channel id, a slash, and a
//!    model some live provider of that channel lists in its catalogue.
//!
//! A third form resolves and is deliberately not listed:
//! `providerName/model`. It is an operator convenience whose left half is a
//! renameable row, so a portal that printed it would be handing users a name
//! that stops working when somebody edits a provider. The channel form is
//! stable for the life of the build.
//!
//! # Nothing is omitted; `permitted` says whether you may call it
//!
//! A name the caller's rules do not reach stays in the list with `permitted:
//! false`. v3 dropped such rows; this does not, for two reasons. A list that
//! silently omits makes "this model 404s" and "you are not allowed this model"
//! the same observation, which is a support ticket rather than an answer. And
//! there is nothing to protect: an exposed name and a `channel/model` form are
//! instance configuration — the same strings the operator publishes — not
//! another tenant's data. What *is* withheld is the provider ids behind them;
//! the DTO reports a count and a channel, which say how redundant a name is
//! without naming the machinery.
//!
//! # The answer is the real decision, not a second model of it
//!
//! Permission is evaluated by
//! [`admission::permission::allowed_providers`](crate::admission::permission::allowed_providers),
//! the same function the request funnel calls, against the same snapshot. A
//! refusal there — no provider permitted — is an empty set here rather than an
//! error, because "you may call none of these" is a perfectly good answer to a
//! catalogue request and a 403 would be a strange one.
//!
//! The operation evaluated against is `GenerateContent`: the portal's question
//! is "what can I send a prompt to". A caller allowed to list a model but not
//! to generate with it would show as not permitted, which is the honest answer
//! to the question actually being asked.

use std::collections::{BTreeMap, BTreeSet};

use gproxy_protocol::Operation;

use super::Portal;
use crate::{AppError, Result, admission::permission, dto::PortalModelDto};

/// How many names one listing may carry.
///
/// A catalogue is providers × models and an operator who imports a large
/// default catalogue onto several providers can reach thousands of names, none
/// of which a person scrolls. The list is sorted first, so the cut is stable
/// rather than arbitrary, and a portal that hits it should be offering a
/// search instead of a list.
pub const MAX_PORTAL_MODELS: usize = 2_000;

/// One addressable name, with the providers behind it collected before any
/// permission is evaluated.
#[derive(Default)]
struct Backing {
    provider_ids: BTreeSet<String>,
    channel_ids: BTreeSet<String>,
}

impl<C> Portal<'_, C> {
    /// The model names this instance answers to, each marked with whether the
    /// caller may call it.
    ///
    /// Sorted by name, so two calls in the same revision produce the same list
    /// and a console can diff them.
    pub fn models(&self) -> Result<Vec<PortalModelDto>> {
        let core = self.writer().gproxy().core().snapshot();
        let routing = self.writer().gproxy().routing();

        let mut names: BTreeMap<String, Backing> = BTreeMap::new();

        // The published names first: a route member of a provider the engine
        // has not loaded contributes nothing, which is the same silence
        // resolution would answer with.
        for (name, route_id) in &routing.names {
            let entry = names.entry(name.clone()).or_default();
            let Some(route) = routing.routes.get(route_id) else {
                continue;
            };
            for member in &route.members {
                let Some(provider) = core.providers.get(&member.provider_id) else {
                    continue;
                };
                entry.provider_ids.insert(provider.entity.id.clone());
                entry.channel_ids.insert(provider.entity.channel.clone());
            }
        }

        // Then the channel forms, straight off the catalogues. Generating them
        // from a live provider's own rows is what makes every name here one
        // that resolves.
        for provider in core.providers.values() {
            let channel = &provider.entity.channel;
            for model in &provider.models {
                for name in model.exposed_names() {
                    let entry = names.entry(format!("{channel}/{name}")).or_default();
                    entry.provider_ids.insert(provider.entity.id.clone());
                    entry.channel_ids.insert(channel.clone());
                }
            }
        }

        let all_providers: BTreeSet<String> = core.providers.keys().cloned().collect();
        names
            .into_iter()
            .take(MAX_PORTAL_MODELS)
            .map(|(name, backing)| {
                let allowed = self.permitted_providers(&name, &all_providers)?;
                Ok(PortalModelDto {
                    permitted: backing.provider_ids.iter().any(|id| allowed.contains(id)),
                    provider_count: backing.provider_ids.len() as u64,
                    channel_ids: backing.channel_ids.into_iter().collect(),
                    name,
                })
            })
            .collect()
    }

    /// The providers the caller's rules allow for one name, with a refusal
    /// read as the empty set.
    fn permitted_providers(
        &self,
        model: &str,
        all_providers: &BTreeSet<String>,
    ) -> Result<BTreeSet<String>> {
        match permission::allowed_providers(
            self.data(),
            self.caller(),
            Some(model),
            Operation::GenerateContent,
            all_providers,
            &self.config().oauth.cli_client_ids,
        ) {
            Ok(allowed) => Ok(allowed),
            // "No provider is permitted" and "this client may not generate at
            // all" are both `permitted: false` for every name, not a failed
            // catalogue request.
            Err(AppError::Forbidden(_)) => Ok(BTreeSet::new()),
            Err(error) => Err(error),
        }
    }
}
