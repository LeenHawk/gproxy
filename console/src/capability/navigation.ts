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
  BookOpenText, Boxes, Building2, ChartLine, CircleUserRound, Fingerprint, Gauge,
  Info, KeyRound, LayoutDashboard, ListChecks, MonitorSmartphone, ScrollText, Settings2, ShieldCheck, UsersRound, Waypoints,
  type LucideIcon,
} from "lucide-react"

import { USAGE_READ, LOGS_READ } from "@/api/observation"
import { SETTINGS_ACCESS } from "@/api/settings"
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
  standalone?: boolean
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
    { id: "requests", route: "/requests", needs: SELF_LOGS_READ, icon: ScrollText },
    { id: "account", route: "/account", needs: SELF_READ, icon: Settings2 },
    { id: "about", route: "/about", needs: SELF_READ, icon: Info },
  ],
}

const PROVIDERS: NavSection = {
  id: "providers",
  icon: Waypoints,
  standalone: true,
  items: [{ id: "providers", route: "/providers", needs: PROVIDERS_READ, icon: Waypoints }],
}

const MODEL_CATALOG: NavSection = { id: "model-catalog", icon: Boxes, standalone: true, items: [{ id: "model-catalog", route: "/model-catalog", needs: SETTINGS_ACCESS, icon: Boxes }] }

const RULES: NavSection = { id: "rules", icon: ListChecks, items: [{ id: "rule-sets", route: "/rule-sets", needs: PROVIDERS_READ, icon: ListChecks }] }

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

const OBSERVATION: NavSection = { id: "observation", icon: ChartLine, items: [
  { id: "globalUsage", route: "/observation/usage", needs: USAGE_READ, icon: ChartLine },
  { id: "downstreamLogs", route: "/observation/downstream", needs: LOGS_READ, icon: ScrollText },
  { id: "upstreamLogs", route: "/observation/upstream", needs: LOGS_READ, icon: ScrollText },
] }

const SYSTEM: NavSection = { id: "system", icon: Settings2, items: [{ id: "settings", route: "/settings", needs: SETTINGS_ACCESS, icon: Settings2 }, { id: "tokenizer", route: "/tokenizer", needs: SETTINGS_ACCESS, icon: BookOpenText }, { id: "update", route: "/update", needs: SETTINGS_ACCESS, icon: Settings2 }, { id: "connection-profiles", route: "/clients", needs: "configuration.connection-profiles", icon: Settings2 }] }

const MANAGEMENT: NavSection = { id: "management", icon: Waypoints, standalone: true, items: [
  { id: "routes", route: "/model-routes", needs: "configuration.routes", icon: Waypoints },
] }
const SECTIONS: ReadonlyArray<NavSection> = [SELF, PROVIDERS, MODEL_CATALOG, RULES, MANAGEMENT, PEOPLE, ACCESS, OBSERVATION, SYSTEM]

/** Tenant pages reuse existing object routes and their scoped APIs, never identity CRUD. */
function sectionsForScope(context: ConsoleContext): ReadonlyArray<NavSection> {
  const tenant = context.scope?.kind === "organization" || context.scope?.kind === "team"
  if (!tenant) return SECTIONS
  return SECTIONS.map(section => {
    if (section.id === "providers") return { ...section, items: section.items.map(item => ({ ...item, needs: "configuration.credentials" })) }
    if (section.id === "people") return { ...section, items: section.items
      .filter(item => context.has("configuration.credentials") && (item.id === "teams" || (item.id === "organizations" && context.scope?.kind === "organization")))
      .map(item => ({ ...item, needs: "configuration.quotas" })) }
    return section
  })
}

/** The sections this caller sees, with the items they may reach. */
export function sectionsFor(context: ConsoleContext): Array<NavSection> {
  return sectionsForScope(context)
    .map(section => ({ ...section, items: section.items.filter(item => context.has(item.needs)) }))
    .filter(section => section.items.length > 0)
}

/** Known but unavailable routes remain forbidden, including the other tenant scopes. */
export function mayEnter(context: ConsoleContext, route: string) {
  if (route === "/transfer" || route === "/settings/transfer") return context.has(SETTINGS_ACCESS) && context.has("configuration.transfer")
  if (route.startsWith("/providers/") && !context.has(PROVIDERS_READ) && !/^\/providers\/[^/]+(?:\/credentials)?$/.test(route)) return false
  const target = route.startsWith("/providers/") ? "/providers" : route
  if (!SECTIONS.some(section => section.items.some(item => item.route === target))) return true
  return sectionsFor(context).some(section => section.items.some(item => item.route === target))
}
