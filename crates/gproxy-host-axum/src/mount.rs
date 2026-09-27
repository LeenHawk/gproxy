//! The mount grammar: which slice of the URL names *this* gateway, and which
//! slice belongs to the API being forwarded.
//!
//! One instance answers the same upstream API at three mounts:
//!
//! | Path | Mount | What it narrows resolution to |
//! |---|---|---|
//! | `/v1/messages` | [`Mount::Aggregated`] | nothing — every provider the caller may reach |
//! | `/acme/v1/messages` | [`Mount::Namespace`] | the exposed model names under `acme/` |
//! | `/openai-prod/v1/messages` | [`Mount::Provider`] | that one provider |
//!
//! A **namespace** is the first segment of a slash-bearing exposed model name:
//! exposing `acme/fast` creates the namespace `acme`. A **provider** is a
//! `providers.name` — the operator's label, not the row id, because the id is
//! machine-minted and nobody types it into a client's base URL.
//!
//! # The rule that makes this safe
//!
//! **A prefix is only stripped when what is left also matches a declared
//! ingress surface.** This is v3's `normalize_path_with`, ported with its
//! tests, and the reason it is worth porting rather than rewriting is that
//! every plausible simplification is wrong:
//!
//! - strip any known name unconditionally, and a provider called `backend-api`
//!   silently eats `/backend-api/codex/responses` — a real Codex path — turning
//!   a data-plane call into a mount that does not exist;
//! - do not check the remainder at all, and a typo in a client's base URL
//!   becomes a 404 from an upstream instead of from here.
//!
//! So an ambiguous path resolves **towards the aggregated mount**: when the
//! first segment names something but the rest is not a surface this gateway
//! serves, the path is left whole and treated as aggregated. Losing a mount is
//! a 404 the operator can see; taking one that was not meant is a request sent
//! to the wrong upstream.

use std::collections::{BTreeMap, BTreeSet};

use gproxy_core::CoreData;
use gproxy_sdk::RoutingTable;

/// Which of the three surfaces a request arrived on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mount {
    /// The instance root. Resolution sees every provider the caller may reach.
    Aggregated,
    /// `/{namespace}/…`: the first segment of a slash-bearing exposed model
    /// name.
    Namespace(String),
    /// `/{provider}/…`: one provider, by its `providers.name`.
    Provider(String),
}

impl Mount {
    /// The path prefix this mount occupies, `""` for the aggregated one. What
    /// an absolute URL back to the same mount is built from.
    pub fn prefix(&self) -> String {
        match self {
            Self::Aggregated => String::new(),
            Self::Namespace(name) | Self::Provider(name) => format!("/{name}"),
        }
    }

    /// The `providers.name` a provider mount named, for the surfaces that
    /// need one row rather than a set: a vendor service call, a credential
    /// view. A namespace names no single provider.
    pub fn provider_name(&self) -> Option<&str> {
        match self {
            Self::Provider(name) => Some(name),
            _ => None,
        }
    }

    /// Split a request path into the mount it names and the path the upstream
    /// API sees.
    ///
    /// `is_surface` decides whether a remainder is something this gateway
    /// serves; see the module note for why the prefix is only stripped when it
    /// answers yes.
    pub fn parse(
        path: &str,
        index: &MountIndex,
        is_surface: impl Fn(&str) -> bool,
    ) -> (Self, String) {
        normalize_path_with(path, |name| index.target(name), is_surface)
    }
}

/// The names that can appear as a mount, as of one snapshot.
///
/// Built from the two published snapshots — the routing table for exposed
/// names, the engine's `CoreData` for providers — once per pair, and replaced
/// when either is, so a mount that stops existing stops being a mount at the
/// next revision rather than at the next restart.
#[derive(Debug, Default)]
pub struct MountIndex {
    namespaces: BTreeSet<String>,
    /// `providers.name` to the provider's row id. The name is what a client
    /// puts in its base URL; the id is what everything below this crate takes.
    providers: BTreeMap<String, String>,
}

impl MountIndex {
    pub fn build(routing: &RoutingTable, core: &CoreData) -> Self {
        let namespaces = routing
            .names
            .keys()
            .filter_map(|name| name.split_once('/'))
            .map(|(namespace, _)| namespace.to_owned())
            .filter(|namespace| !namespace.is_empty())
            .collect();
        let providers = core
            .providers
            .iter()
            .map(|(id, provider)| (provider.entity.name.clone(), id.clone()))
            .collect();
        Self {
            namespaces,
            providers,
        }
    }

    /// What `name` mounts, if anything.
    ///
    /// A namespace wins over a provider of the same name, for the reason the
    /// sdk's resolver gives for preferring a channel id over a provider name:
    /// the exposed name is what an operator deliberately published, and a
    /// provider can always be renamed out of the collision.
    pub fn target(&self, name: &str) -> Option<Mount> {
        if self.namespaces.contains(name) {
            return Some(Mount::Namespace(name.to_owned()));
        }
        self.providers
            .contains_key(name)
            .then(|| Mount::Provider(name.to_owned()))
    }

    /// The row id behind a provider mount.
    pub fn provider_id(&self, name: &str) -> Option<&str> {
        self.providers.get(name).map(String::as_str)
    }
}

/// v3's `normalize_path_with`, with `RoutingMode` replaced by [`Mount`]. The
/// structure is deliberately unchanged: one split, two predicates, and the
/// aggregated answer for everything that is not unambiguously a mount.
fn normalize_path_with(
    path: &str,
    target: impl FnOnce(&str) -> Option<Mount>,
    is_surface: impl FnOnce(&str) -> bool,
) -> (Mount, String) {
    let Some((name, remainder)) = path.strip_prefix('/').and_then(|path| path.split_once('/'))
    else {
        return (Mount::Aggregated, path.to_owned());
    };
    let remainder = format!("/{remainder}");
    if name.is_empty() {
        return (Mount::Aggregated, path.to_owned());
    }
    let Some(mount) = target(name) else {
        return (Mount::Aggregated, path.to_owned());
    };
    if !is_surface(&remainder) {
        return (Mount::Aggregated, path.to_owned());
    }
    (mount, remainder)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_prefix_accepts_every_declared_remainder() {
        for (path, remainder) in [
            ("/codex/v1/responses", "/v1/responses"),
            (
                "/codex/backend-api/codex/responses",
                "/backend-api/codex/responses",
            ),
            ("/codex/backend-api/wham/usage", "/backend-api/wham/usage"),
            ("/codex/oauth/token", "/oauth/token"),
        ] {
            assert_eq!(
                normalize_path_with(
                    path,
                    |name| (name == "codex").then(|| Mount::Provider(name.to_owned())),
                    |value| value == remainder
                ),
                (Mount::Provider("codex".into()), remainder.into())
            );
        }
    }

    #[test]
    fn unknown_prefix_or_remainder_stays_aggregated() {
        for path in [
            // The first segment is not a mount, and `/codex/responses` is not
            // a surface either: the whole path goes to the aggregated mount.
            "/backend-api/codex/responses",
            "/missing/v1/responses",
            "/codex/not-a-surface",
        ] {
            assert_eq!(
                normalize_path_with(
                    path,
                    |name| (name == "codex").then(|| Mount::Provider(name.to_owned())),
                    |value| { value == "/v1/responses" }
                ),
                (Mount::Aggregated, path.into())
            );
        }
    }

    #[test]
    fn a_path_with_no_second_segment_is_never_a_mount() {
        // There is no remainder to serve, so there is nothing to strip.
        for path in ["/", "/codex", "/healthz"] {
            assert_eq!(
                normalize_path_with(path, |_| Some(Mount::Provider("codex".into())), |_| true),
                (Mount::Aggregated, path.into())
            );
        }
    }

    #[test]
    fn a_namespace_wins_over_a_provider_of_the_same_name() {
        let index = MountIndex {
            namespaces: ["acme".to_owned()].into_iter().collect(),
            providers: [("acme".to_owned(), "p-7".to_owned())]
                .into_iter()
                .collect(),
        };
        assert_eq!(index.target("acme"), Some(Mount::Namespace("acme".into())));
        assert_eq!(index.target("nobody"), None);

        let providers = MountIndex {
            namespaces: BTreeSet::new(),
            providers: [("acme".to_owned(), "p-7".to_owned())]
                .into_iter()
                .collect(),
        };
        assert_eq!(
            providers.target("acme"),
            Some(Mount::Provider("acme".into()))
        );
        assert_eq!(providers.provider_id("acme"), Some("p-7"));
    }

    #[test]
    fn a_mounts_prefix_is_what_an_absolute_url_back_to_it_starts_with() {
        assert_eq!(Mount::Aggregated.prefix(), "");
        assert_eq!(Mount::Namespace("acme".into()).prefix(), "/acme");
        assert_eq!(Mount::Provider("p1".into()).prefix(), "/p1");
        assert_eq!(Mount::Provider("p1".into()).provider_name(), Some("p1"));
        assert_eq!(Mount::Namespace("acme".into()).provider_name(), None);
    }
}
