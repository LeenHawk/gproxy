//! The navigation, as a function of the capability set.
//!
//! Every entry names the capability the server has to have granted for it to
//! appear, so nothing renders a link to a page whose API would answer 403. The
//! list is data rather than JSX because the same declaration answers three
//! questions: what the sidebar shows, whether a route may be entered, and what
//! to fall back to when it may not.
//!
//! An identity item's capability is `identity.<family>`, where the family is
//! the `/admin/api` path segment it calls. One nav item, one family, one
//! route-table entry: when the scoped administration API gives an organization
//! administrator `identity.teams` and not `identity.users`, this table needs
//! no edit at all — the item simply appears for them.

import { SELF_LOGS_READ, SELF_READ, type Capability, type ConsoleContext } from "@/capability/capability"

export type NavItem = {
  /** Also the i18n key under `nav.` and the route's identity. */
  id: string
  route: string
  needs: Capability
}

export type NavSection = {
  /** Also the i18n key under `section.`. */
  id: string
  items: Array<NavItem>
}

/**
 * The self-service surface. Every signed-in caller has it, which is why the
 * console's root is here and not on an operator dashboard: most callers are
 * not operators.
 */
const SELF: NavSection = {
  id: "self",
  items: [
    { id: "overview", route: "/", needs: SELF_READ },
    { id: "keys", route: "/keys", needs: SELF_READ },
    { id: "models", route: "/models", needs: SELF_READ },
    { id: "usage", route: "/usage", needs: SELF_READ },
    { id: "quota", route: "/quota", needs: SELF_READ },
    { id: "requests", route: "/requests", needs: SELF_LOGS_READ },
    { id: "account", route: "/account", needs: SELF_READ },
  ],
}

const IDENTITY: NavSection = {
  id: "identity",
  items: [
    { id: "users", route: "/identity/users", needs: "identity.users" },
    { id: "api-keys", route: "/identity/api-keys", needs: "identity.api-keys" },
    { id: "organizations", route: "/identity/organizations", needs: "identity.organizations" },
    { id: "teams", route: "/identity/teams", needs: "identity.teams" },
    { id: "permissions", route: "/identity/permissions", needs: "identity.permissions" },
    { id: "rate-limits", route: "/identity/rate-limits", needs: "identity.rate-limits" },
    { id: "plans", route: "/identity/plans", needs: "identity.plans" },
    { id: "plan-limits", route: "/identity/plan-limits", needs: "identity.plans" },
    { id: "subscriptions", route: "/identity/subscriptions", needs: "identity.subscriptions" },
    { id: "pools", route: "/identity/pools", needs: "identity.pools" },
    { id: "pool-members", route: "/identity/pool-members", needs: "identity.pools" },
    { id: "oauth-clients", route: "/identity/oauth-clients", needs: "identity.oauth-clients" },
    { id: "sessions", route: "/identity/sessions", needs: "identity.sessions" },
    { id: "audit", route: "/identity/audit", needs: "identity.audit" },
  ],
}

const SECTIONS: ReadonlyArray<NavSection> = [SELF, IDENTITY]

/** The sections this caller sees, with the items they may reach. */
export function sectionsFor(context: ConsoleContext): Array<NavSection> {
  return SECTIONS
    .map((section) => ({ ...section, items: section.items.filter((item) => context.has(item.needs)) }))
    .filter((section) => section.items.length > 0)
}

/** Whether this caller may enter a route, by the same table the sidebar uses. */
export function mayEnter(context: ConsoleContext, route: string) {
  const item = SECTIONS.flatMap((section) => section.items).find((entry) => entry.route === route)
  return item ? context.has(item.needs) : true
}
