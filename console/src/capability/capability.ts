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
//! Portal features describe self-service; management sections and scope
//! selectors come from the authenticated administrative context.

import type { AdminContextDto, AdminScopeDto, PortalContextDto } from "@/generated/app"

/**
 * One scope a caller may administer in.
 *
 * The console's mirror of `gproxy-app`'s `AdminScope`: `Instance`,
 * `Organization(id)` or `Team(id)`, one per request. `name` is carried
 * alongside so a scope can be named to a person without a second lookup.
 */
export type AdminScope = AdminScopeDto

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
  switchScope?: (selector: string) => Promise<void>
  userId: string
  userName: string
  hasPassword: boolean
  organizations: PortalContextDto["organizations"]
  teams: PortalContextDto["teams"]
  /** Every scope this caller may act as, widest first. Empty is ordinary. */
  scopes: Array<AdminScope>
  /** The scope in force. `null` when the caller administers nothing. */
  scope: AdminScope | null
  /** What to render. The server's answer, not a local inference. */
  capabilities: ReadonlySet<Capability>
  has: (capability: Capability) => boolean
}

/** Combine portal features with the selected scope's server-granted sections. */
export function consoleContext(context: PortalContextDto & { admin?: AdminContextDto | null }): ConsoleContext {
  const capabilities = new Set<Capability>([SELF_READ])
  if (context.features.canCreateKeys) capabilities.add(SELF_KEYS_WRITE)
  if (context.features.canChangePassword) capabilities.add(SELF_PASSWORD_CHANGE)
  if (context.features.canSeeLogs) capabilities.add(SELF_LOGS_READ)
  for (const section of context.admin?.sections ?? []) {
    if (!section.capabilities.includes("read")) continue
    const prefix = (IDENTITY_FAMILIES as readonly string[]).includes(section.id) ? "identity" : "configuration"
    capabilities.add(`${prefix}.${section.id}`)
    if (section.capabilities.includes("write")) capabilities.add(`${prefix}.${section.id}.write`)
  }
  return {
    userId: context.user.id, userName: context.user.name, hasPassword: context.user.hasPassword,
    organizations: context.organizations, teams: context.teams,
    scopes: context.admin?.scopes ?? [], scope: context.admin?.scope ?? null,
    capabilities, has: capability => capabilities.has(capability),
  }
}
