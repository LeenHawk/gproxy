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

import {
  Boxes, Building2, ChartLine, CircleUserRound, CreditCard, Fingerprint, Gauge,
  KeyRound, LayoutDashboard, Layers, ListChecks, MonitorSmartphone, Network,
  ReceiptText, ScrollText, Settings2, ShieldCheck, SlidersHorizontal, UsersRound, Waypoints,
  type LucideIcon,
} from "lucide-react"

import { PROVIDERS_READ } from "@/api/configuration"

import { SELF_LOGS_READ, SELF_READ, type Capability, type ConsoleContext } from "@/capability/capability"

export type NavItem = {
  /** Also the i18n key under `nav.` and the route's identity. */
  id: string
  label?: string
  route: string
  needs: Capability
  icon: LucideIcon
}

export type NavSection = {
  /** Also the i18n key under `section.`. */
  id: string
  icon: LucideIcon
  items: Array<NavItem>
}

/**
 * The self-service surface. Every signed-in caller has it, which is why the
 * console's root is here and not on an operator dashboard: most callers are
 * not operators.
 */
const SELF: NavSection = {
  id: "self",
  icon: CircleUserRound,
  items: [
    { id: "overview", route: "/", needs: SELF_READ, icon: LayoutDashboard },
    { id: "keys", route: "/keys", needs: SELF_READ, icon: KeyRound },
    { id: "models", route: "/models", needs: SELF_READ, icon: Boxes },
    { id: "usage", route: "/usage", needs: SELF_READ, icon: ChartLine },
    { id: "quota", route: "/quota", needs: SELF_READ, icon: Gauge },
    { id: "requests", route: "/requests", needs: SELF_LOGS_READ, icon: ScrollText },
    { id: "account", route: "/account", needs: SELF_READ, icon: Settings2 },
  ],
}

const PROVIDERS: NavSection = {
  id: "providers",
  icon: Waypoints,
  items: [{ id: "providers", route: "/providers", needs: PROVIDERS_READ, icon: Waypoints }],
}

const PEOPLE: NavSection = {
  id: "people",
  icon: UsersRound,
  items: [
    { id: "users", route: "/identity/users", needs: "identity.users", icon: UsersRound },
    { id: "organizations", route: "/identity/organizations", needs: "identity.organizations", icon: Building2 },
    { id: "teams", route: "/identity/teams", needs: "identity.teams", icon: UsersRound },
  ],
}

const ACCESS: NavSection = {
  id: "access",
  icon: ShieldCheck,
  items: [
    { id: "api-keys", route: "/identity/api-keys", needs: "identity.api-keys", icon: KeyRound },
    { id: "permissions", route: "/identity/permissions", needs: "identity.permissions", icon: ShieldCheck },
    { id: "rate-limits", route: "/identity/rate-limits", needs: "identity.rate-limits", icon: Gauge },
    { id: "oauth-clients", route: "/identity/oauth-clients", needs: "identity.oauth-clients", icon: Fingerprint },
    { id: "sessions", route: "/identity/sessions", needs: "identity.sessions", icon: MonitorSmartphone },
    { id: "audit", route: "/identity/audit", needs: "identity.audit", icon: ListChecks },
  ],
}

const BILLING: NavSection = {
  id: "billing",
  icon: CreditCard,
  items: [
    { id: "plans", route: "/identity/plans", needs: "identity.plans", icon: CreditCard },
    { id: "plan-limits", route: "/identity/plan-limits", needs: "identity.plans", icon: SlidersHorizontal },
    { id: "subscriptions", route: "/identity/subscriptions", needs: "identity.subscriptions", icon: ReceiptText },
    { id: "pools", route: "/identity/pools", needs: "identity.pools", icon: Layers },
    { id: "pool-members", route: "/identity/pool-members", needs: "identity.pools", icon: Network },
  ],
}

const SECTIONS: ReadonlyArray<NavSection> = [SELF, PROVIDERS, PEOPLE, ACCESS, BILLING]

/** The sections this caller sees, with the items they may reach. */
export function sectionsFor(context: ConsoleContext): Array<NavSection> {
  return SECTIONS
    .map((section) => ({ ...section, items: section.items.filter((item) => context.has(item.needs)) }))
    .filter((section) => section.items.length > 0)
}

/** Whether this caller may enter a route, by the same table the sidebar uses. */
export function mayEnter(context: ConsoleContext, route: string) {
  const target = route.startsWith("/providers/") ? "/providers" : route
  const item = SECTIONS.flatMap((section) => section.items).find((entry) => entry.route === target)
  return item ? context.has(item.needs) : true
}
