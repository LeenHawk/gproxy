//! **What the server says this caller may do.** One module, and the only one
//! that knows.
//!
//! v3 shipped two applications — `surfaces/admin-surface.tsx` and
//! `surfaces/portal-surface.tsx` — and decided between them from the URL. That
//! made "operator" and "user" two different programs with two shells, two
//! navigations and two copies of every shared page. v4 has one console whose
//! sections are rendered from a capability set.
//!
//! # The contract
//!
//! [`ConsoleContext`] is what the rest of the console reads: the scopes this
//! caller may act as, the one they are acting as now, and the set of
//! capability names the server says to render. Nothing outside this file asks
//! what anybody's *role* is — a role is a fact about a row, a capability is an
//! answer about a request, and only the server can turn the first into the
//! second. The route table decides a minimum scope per family; a console that
//! re-derived that from a role string would be a second, divergent copy of an
//! authorization decision.
//!
//! The backend also exposes `/admin/api/context` for scoped administration.
//! This console still uses the portal context for its session and exposes
//! instance-admin management pages through the adapter below.

import type { PortalContextDto } from "@/generated/app"
import { SETTINGS_ACCESS } from "@/api/settings"
import { PROVIDERS_READ } from "@/api/configuration"

/**
 * One scope a caller may administer in.
 *
 * The console's mirror of `gproxy-app`'s `AdminScope`: `Instance`,
 * `Organization(id)` or `Team(id)`, one per request. `name` is carried
 * alongside so a scope can be named to a person without a second lookup.
 */
export type AdminScope =
  | { kind: "instance" }
  | { kind: "organization"; id: string; name: string }
  | { kind: "team"; id: string; name: string }

export function scopeKey(scope: AdminScope) {
  return scope.kind === "instance" ? "instance" : `${scope.kind}:${scope.id}`
}

/**
 * A capability name, as the server spells it.
 *
 * Deliberately an opaque string rather than a union: the authoritative list
 * lives in the route table, and a closed union here would have to be edited
 * every time a family is added — which is exactly the drift this seam exists
 * to avoid. The names the console's navigation asks for are
 * `identity.<family>` and `self.<verb>`, which are the family path segments
 * `/admin/api` already uses and the four self-service facts the portal
 * context already reports.
 */
export type Capability = string

/** The identity families, by their `/admin/api` path segment. */
export const IDENTITY_FAMILIES = [
  "users",
  "api-keys",
  "organizations",
  "teams",
  "permissions",
  "rate-limits",
  "plans",
  "subscriptions",
  "pools",
  "oauth-clients",
  "sessions",
  "audit",
] as const

export const SELF_READ = "self.read"
export const SELF_KEYS_WRITE = "self.keys.write"
export const SELF_PASSWORD_CHANGE = "self.password.change"
export const SELF_LOGS_READ = "self.logs.read"

/** Everything the application is allowed to branch on. */
export type ConsoleContext = {
  userId: string
  userName: string
  hasPassword: boolean
  organizations: PortalContextDto["organizations"]
  teams: PortalContextDto["teams"]
  subscription: PortalContextDto["subscription"]
  /** Every scope this caller may act as, widest first. Empty is ordinary. */
  scopes: Array<AdminScope>
  /** The scope in force. `null` when the caller administers nothing. */
  scope: AdminScope | null
  /** What to render. The server's answer, not a local inference. */
  capabilities: ReadonlySet<Capability>
  has: (capability: Capability) => boolean
}

/**
 * The adapter, and the only place a role string is read.
 *
 * `PortalContextDto` carries three kinds of fact and this maps each one:
 *
 * - `user.role === "admin"` — an instance operator, who by today's route table
 *   may reach the identity families and provider configuration;
 * - a membership whose per-scope `role` is `admin` becomes an [`AdminScope`].
 *   Scoped management is not exposed by this adapter; only instance operators
 *   receive the management navigation here;
 * - `features.*` — three facts the host already computes about this caller,
 *   two of them about an OAuth-grant caller being refused the writing half of
 *   the account it was lent, and one an instance setting.
 *
 * Replacing this with a parse of `/admin/api/context` is the whole of the
 * migration.
 */
function fromPortalContext(context: PortalContextDto) {
  const capabilities = new Set<Capability>([SELF_READ])
  const scopes: Array<AdminScope> = []

  if (context.user.role === "admin") {
    scopes.push({ kind: "instance" })
    capabilities.add(PROVIDERS_READ)
    capabilities.add(SETTINGS_ACCESS)
    for (const family of IDENTITY_FAMILIES) capabilities.add(`identity.${family}`)
  }
  for (const entry of context.organizations) {
    if (entry.role === "admin") scopes.push({ kind: "organization", id: entry.id, name: entry.name })
  }
  for (const entry of context.teams) {
    if (entry.role === "admin") scopes.push({ kind: "team", id: entry.id, name: entry.name })
  }

  if (context.features.canCreateKeys) capabilities.add(SELF_KEYS_WRITE)
  if (context.features.canChangePassword) capabilities.add(SELF_PASSWORD_CHANGE)
  if (context.features.canSeeLogs) capabilities.add(SELF_LOGS_READ)

  return { capabilities, scopes }
}

/**
 * The context the shell runs on.
 *
 * The current scope defaults to the first one the caller has, which for an
 * instance operator is `instance`. A scope *switcher* is deliberately not
 * built here: choosing a scope means sending it with each request, and that
 * header is defined by the same change that adds the endpoint above. The shape
 * is ready for one — `scopes` is a list and `scope` is a single value — and
 * nothing else has to move when it arrives.
 */
export function consoleContext(context: PortalContextDto): ConsoleContext {
  const { capabilities, scopes } = fromPortalContext(context)
  return {
    userId: context.user.id,
    userName: context.user.name,
    hasPassword: context.user.hasPassword,
    organizations: context.organizations,
    teams: context.teams,
    subscription: context.subscription,
    scopes,
    scope: scopes[0] ?? null,
    capabilities,
    has: (capability) => capabilities.has(capability),
  }
}
