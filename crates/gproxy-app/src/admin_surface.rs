//! The management surface as a table: every section a console can render, the
//! minimum [`AdminScope`] that reaches it, and what may be done in it.
//!
//! # One declaration, two readers
//!
//! This table is the *only* statement of which families an organization
//! administrator may reach. Both readers derive from it and neither repeats
//! it:
//!
//! - a host's route table names a section per route and gates on
//!   [`require_section`], so a route cannot be open by accident;
//! - `GET /admin/api/context` renders the sections the caller's scope reaches,
//!   and the console builds its navigation from that answer and from nothing
//!   else.
//!
//! Putting it here rather than in a host keeps the two hosts (axum, edge) and
//! the console answering the same question the same way, and makes the whole
//! policy readable in one screen.
//!
//! # Default closed
//!
//! [`require_section`] refuses a section it does not recognise. A route that
//! names a section that is not in this table is therefore unreachable outside
//! the instance scope rather than silently open, and
//! [`every_section_is_declared_once`](self) keeps the table itself honest.
//! Combined with the host macros — whose section argument is mandatory, so a
//! family that declares nothing does not compile — a new route is closed
//! unless somebody writes down that it should not be.
//!
//! # What is instance machinery, and why
//!
//! Everything that configures the *gateway* rather than a tenant: providers,
//! models, routes, rewrite rules, endpoints, prices, connection profiles,
//! settings, transfer, connectivity, the tokenizer and the static catalogues.
//! An organization administrator does not run the upstreams; they spend them.
//!
//! The catalogues (`/channels`, `/tls-presets`, `/rule-presets`,
//! `/default-model-catalog`) are static data and leak nothing — they are
//! instance-only anyway, because the only thing a console does with them is
//! render a provider form, and an organization administrator cannot create a
//! provider.
//!
//! The identity families are instance machinery **for now**: they are the
//! largest part of the scope model and are opened in their own step. See the
//! crate README.

use crate::{AdminScope, AppError, Result};

/// The minimum scope a section is reachable from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SectionScope {
    /// Only [`AdminScope::Instance`].
    Instance,
    /// Any scope. The family narrows its own rows; see
    /// [`AdminScope::narrow`].
    Scoped,
}

/// One navigable family of the management surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdminSection {
    /// Stable identifier, also the audit trail's family name. The console
    /// keys its navigation and its translations on this.
    pub id: &'static str,
    /// The collection path under `/admin/api`, for a console that builds its
    /// requests from the table rather than hard-coding them.
    pub path: &'static str,
    pub minimum: SectionScope,
    /// What the section supports: `read`, `write`, or both. A section with no
    /// `write` is one a console renders without its create and delete
    /// affordances.
    pub capabilities: &'static [&'static str],
}

impl AdminSection {
    /// Whether `scope` reaches this section at all.
    pub fn reachable(&self, scope: &AdminScope) -> bool {
        match self.minimum {
            SectionScope::Instance => scope.is_instance(),
            SectionScope::Scoped => true,
        }
    }
}

const READ: &[&str] = &["read"];
const READ_WRITE: &[&str] = &["read", "write"];

/// Every section of `/admin/api`, in the order a console would list them.
///
/// The order is the navigation's: the tenant's own things first — which is
/// what an organization administrator sees — then the gateway's, then
/// identity, then the operator's tools.
pub const ADMIN_SECTIONS: &[AdminSection] = &[
    // -- reachable from every scope ----------------------------------------
    AdminSection {
        id: "context",
        path: "/context",
        minimum: SectionScope::Scoped,
        capabilities: READ,
    },
    AdminSection {
        // The caller's own session: who am I, and sign me out. Self-referential
        // and therefore safe in any scope, exactly as the portal's is.
        id: "session",
        path: "/session",
        minimum: SectionScope::Scoped,
        capabilities: READ_WRITE,
    },
    AdminSection {
        id: "credentials",
        path: "/credentials",
        minimum: SectionScope::Scoped,
        capabilities: READ_WRITE,
    },
    AdminSection {
        id: "quotas",
        path: "/quotas",
        minimum: SectionScope::Scoped,
        capabilities: READ_WRITE,
    },
    // -- the gateway's own configuration -----------------------------------
    AdminSection {
        id: "providers",
        path: "/providers",
        minimum: SectionScope::Instance,
        capabilities: READ_WRITE,
    },
    AdminSection {
        id: "models",
        path: "/models",
        minimum: SectionScope::Instance,
        capabilities: READ_WRITE,
    },
    AdminSection {
        id: "provider-models",
        path: "/provider-models",
        minimum: SectionScope::Instance,
        capabilities: READ_WRITE,
    },
    AdminSection {
        id: "routes",
        path: "/routes",
        minimum: SectionScope::Instance,
        capabilities: READ_WRITE,
    },
    AdminSection {
        id: "route-members",
        path: "/route-members",
        minimum: SectionScope::Instance,
        capabilities: READ_WRITE,
    },
    AdminSection {
        id: "exposed-models",
        path: "/exposed-models",
        minimum: SectionScope::Instance,
        capabilities: READ_WRITE,
    },
    AdminSection {
        id: "connection-profiles",
        path: "/connection-profiles",
        minimum: SectionScope::Instance,
        capabilities: READ_WRITE,
    },
    AdminSection {
        id: "settings",
        path: "/settings",
        minimum: SectionScope::Instance,
        capabilities: READ_WRITE,
    },
    AdminSection {
        id: "rule-sets",
        path: "/rule-sets",
        minimum: SectionScope::Instance,
        capabilities: READ_WRITE,
    },
    AdminSection {
        id: "rules",
        path: "/rules",
        minimum: SectionScope::Instance,
        capabilities: READ_WRITE,
    },
    AdminSection {
        id: "provider-rule-sets",
        path: "/provider-rule-sets",
        minimum: SectionScope::Instance,
        capabilities: READ_WRITE,
    },
    AdminSection {
        id: "operation-rules",
        path: "/operation-rules",
        minimum: SectionScope::Instance,
        capabilities: READ_WRITE,
    },
    AdminSection {
        id: "operation-endpoints",
        path: "/operation-endpoints",
        minimum: SectionScope::Instance,
        capabilities: READ_WRITE,
    },
    AdminSection {
        id: "price-rules",
        path: "/price-rules",
        minimum: SectionScope::Instance,
        capabilities: READ_WRITE,
    },
    AdminSection {
        id: "price-rates",
        path: "/price-rates",
        minimum: SectionScope::Instance,
        capabilities: READ_WRITE,
    },
    AdminSection {
        id: "price-tiers",
        path: "/price-tiers",
        minimum: SectionScope::Instance,
        capabilities: READ_WRITE,
    },
    // -- identity ----------------------------------------------------------
    AdminSection {
        id: "users",
        path: "/users",
        minimum: SectionScope::Instance,
        capabilities: READ_WRITE,
    },
    AdminSection {
        id: "api-keys",
        path: "/api-keys",
        minimum: SectionScope::Instance,
        capabilities: READ_WRITE,
    },
    AdminSection {
        id: "organizations",
        path: "/organizations",
        minimum: SectionScope::Instance,
        capabilities: READ_WRITE,
    },
    AdminSection {
        id: "teams",
        path: "/teams",
        minimum: SectionScope::Instance,
        capabilities: READ_WRITE,
    },
    AdminSection {
        id: "members",
        path: "/organizations/{id}/members",
        minimum: SectionScope::Instance,
        capabilities: READ_WRITE,
    },
    AdminSection {
        id: "team-members",
        path: "/teams/{id}/members",
        minimum: SectionScope::Instance,
        capabilities: READ_WRITE,
    },
    AdminSection {
        id: "permissions",
        path: "/permissions",
        minimum: SectionScope::Instance,
        capabilities: READ_WRITE,
    },
    AdminSection {
        id: "rate-limits",
        path: "/rate-limits",
        minimum: SectionScope::Instance,
        capabilities: READ_WRITE,
    },
    AdminSection {
        id: "subscriptions",
        path: "/subscriptions",
        minimum: SectionScope::Instance,
        capabilities: READ_WRITE,
    },
    AdminSection {
        id: "pools",
        path: "/pools",
        minimum: SectionScope::Instance,
        capabilities: READ_WRITE,
    },
    AdminSection {
        id: "pool-members",
        path: "/pool-members",
        minimum: SectionScope::Instance,
        capabilities: READ_WRITE,
    },
    AdminSection {
        id: "plans",
        path: "/plans",
        minimum: SectionScope::Instance,
        capabilities: READ_WRITE,
    },
    AdminSection {
        id: "plan-limits",
        path: "/plan-limits",
        minimum: SectionScope::Instance,
        capabilities: READ_WRITE,
    },
    AdminSection {
        id: "oauth-clients",
        path: "/oauth-clients",
        minimum: SectionScope::Instance,
        capabilities: READ_WRITE,
    },
    AdminSection {
        id: "sessions",
        path: "/sessions",
        minimum: SectionScope::Instance,
        capabilities: READ_WRITE,
    },
    AdminSection {
        id: "audit",
        path: "/audit",
        minimum: SectionScope::Instance,
        capabilities: READ,
    },
    // -- the operator's tools ----------------------------------------------
    AdminSection {
        id: "transfer",
        path: "/export",
        minimum: SectionScope::Instance,
        capabilities: READ_WRITE,
    },
    AdminSection {
        id: "connectivity",
        path: "/connectivity/test",
        minimum: SectionScope::Instance,
        capabilities: READ_WRITE,
    },
    AdminSection {
        id: "catalog",
        path: "/channels",
        minimum: SectionScope::Instance,
        capabilities: READ,
    },
    AdminSection {
        id: "tokenizer",
        path: "/tokenizer-vocabs",
        minimum: SectionScope::Instance,
        capabilities: READ_WRITE,
    },
];

/// The declared section, if there is one.
pub fn section(id: &str) -> Option<&'static AdminSection> {
    ADMIN_SECTIONS.iter().find(|section| section.id == id)
}

/// The sections `scope` reaches, in table order.
pub fn sections_for(scope: &AdminScope) -> impl Iterator<Item = &'static AdminSection> + '_ {
    ADMIN_SECTIONS
        .iter()
        .filter(move |section| section.reachable(scope))
}

/// The gate a host's route table calls.
///
/// An unknown section is refused, which is what makes the table default
/// closed: a route that names a section nobody declared is reachable by the
/// instance scope only.
pub fn require_section(scope: &AdminScope, id: &str) -> Result<()> {
    match section(id) {
        Some(section) if section.reachable(scope) => Ok(()),
        _ if scope.is_instance() => Ok(()),
        _ => Err(AppError::forbidden(format!(
            "`{id}` is instance machinery and is not reachable from an organization or team scope"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_section_is_declared_once() {
        let mut ids: Vec<&str> = ADMIN_SECTIONS.iter().map(|section| section.id).collect();
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count, "a section id is declared twice");
        for section in ADMIN_SECTIONS {
            assert!(section.path.starts_with('/'), "{}", section.id);
            assert!(!section.capabilities.is_empty(), "{}", section.id);
        }
    }

    #[test]
    fn only_the_four_scoped_sections_are_open() {
        // The list an organization administrator may reach. A family joining
        // it is a deliberate act, and this test is where it is written down.
        let open: Vec<&str> = ADMIN_SECTIONS
            .iter()
            .filter(|section| section.minimum == SectionScope::Scoped)
            .map(|section| section.id)
            .collect();
        assert_eq!(open, ["context", "session", "credentials", "quotas"]);
    }

    #[test]
    fn an_unknown_section_is_closed_to_everything_but_the_instance() {
        let organization = AdminScope::Organization("acme".into());
        assert!(require_section(&AdminScope::Instance, "not-a-section").is_ok());
        let error = require_section(&organization, "not-a-section").unwrap_err();
        assert_eq!(error.status_code(), 403);
    }

    #[test]
    fn a_scope_reaches_exactly_the_sections_it_declares() {
        let team = AdminScope::Team("core".into());
        let reached: Vec<&str> = sections_for(&team).map(|section| section.id).collect();
        assert_eq!(reached, ["context", "session", "credentials", "quotas"]);
        assert_eq!(
            sections_for(&AdminScope::Instance).count(),
            ADMIN_SECTIONS.len()
        );

        assert!(require_section(&team, "credentials").is_ok());
        assert!(require_section(&team, "quotas").is_ok());
        for closed in ["providers", "settings", "users", "transfer", "catalog"] {
            assert_eq!(
                require_section(&team, closed).unwrap_err().status_code(),
                403,
                "{closed}"
            );
            assert!(require_section(&AdminScope::Instance, closed).is_ok());
        }
    }
}
