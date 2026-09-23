//! `/admin/api` — the instance operator's surface.
//!
//! `gproxy-host-axum/src/admin.rs` builds most of this surface from a
//! `family!` macro: five routes with the same five shapes for every identity
//! family. This file is the same macro on the other side, as a factory — so a
//! family that gains a route in Rust gains it here by being spelled out, and a
//! family that does not is five lines.
//!
//! # Request shapes are `Partial`
//!
//! `ts-rs` renders `Option<T>` as `T | null` rather than `field?: T`, which is
//! right for a response (the field is always there) and wrong for a filter
//! (serde treats absent and null alike). Query objects are therefore taken as
//! `Partial<…>` and [`query`] drops whatever the caller left out.

import type {
  ApiKeyCreated,
  ApiKeyDto,
  ApiKeyPatch,
  ApiKeySecretDto,
  ApiKeyWrite,
  AuditEventDto,
  AuditQuery,
  ListQuery,
  MemberPatch,
  MemberWrite,
  OAuthClientDto,
  OAuthClientPatch,
  OAuthClientWrite,
  OrganizationDto,
  OrganizationMemberDto,
  OrganizationPatch,
  OrganizationWrite,
  Page,
  PermissionDto,
  PermissionPatch,
  PermissionWrite,
  RateLimitDto,
  RateLimitPatch,
  RateLimitWrite,
  TeamDto,
  TeamMemberDto,
  TeamPatch,
  TeamWrite,
  UserDto,
  UserPatch,
  UserSessionDto,
  UserWrite,
} from "@/generated/app"
import { api, json, query } from "@/api/client"

const BASE = "/admin/api"

export type ListFilter = Partial<ListQuery>
export type AuditFilter = Partial<AuditQuery>

/** The five routes `family!` declares, for one family. */
export type Family<D, W, P> = {
  path: string
  list: (filter: ListFilter) => Promise<Page<D>>
  get: (id: string) => Promise<D>
  create: (write: W) => Promise<D>
  update: (id: string, patch: P) => Promise<D>
  remove: (id: string) => Promise<void>
}

export function family<D, W, P>(path: string): Family<D, W, P> {
  const item = (id: string) => `${BASE}${path}/${encodeURIComponent(id)}`
  return {
    path,
    list: (filter) => api<Page<D>>(`${BASE}${path}${query({ ...filter })}`),
    get: (id) => api<D>(item(id)),
    create: (write) => api<D>(`${BASE}${path}`, json("POST", write)),
    update: (id, patch) => api<D>(item(id), json("PATCH", patch)),
    remove: (id) => api<void>(item(id), { method: "DELETE" }),
  }
}

export const users = {
  ...family<UserDto, UserWrite, UserPatch>("/users"),
  setPassword: (id: string, password: string) =>
    api<UserDto>(`${BASE}/users/${encodeURIComponent(id)}/password`, json("POST", { password })),
  clearPassword: (id: string) =>
    api<UserDto>(`${BASE}/users/${encodeURIComponent(id)}/password`, { method: "DELETE" }),
  /** `null` clears the per-user list and falls back to the level above. */
  setAllowlist: (id: string, clients: Array<string> | null) =>
    api<UserDto>(`${BASE}/users/${encodeURIComponent(id)}/allowlist`, json("PUT", { clients })),
  sessions: (id: string) => api<Array<UserSessionDto>>(`${BASE}/users/${encodeURIComponent(id)}/sessions`),
  revokeSessions: (id: string) =>
    api<{ revoked: number }>(`${BASE}/users/${encodeURIComponent(id)}/sessions`, { method: "DELETE" }),
}

export const apiKeys = {
  ...family<ApiKeyDto, ApiKeyWrite, ApiKeyPatch>("/api-keys"),
  rotate: (id: string) => api<ApiKeyCreated>(`${BASE}/api-keys/${encodeURIComponent(id)}/rotate`, { method: "POST" }),
  reveal: (id: string) => api<ApiKeySecretDto>(`${BASE}/api-keys/${encodeURIComponent(id)}/secret`),
}

/** A mint answers with the plaintext once; a plain create never does. */
export function createApiKey(write: ApiKeyWrite) {
  return api<ApiKeyCreated>(`${BASE}/api-keys`, json("POST", write))
}

export const organizations = family<OrganizationDto, OrganizationWrite, OrganizationPatch>("/organizations")
export const teams = family<TeamDto, TeamWrite, TeamPatch>("/teams")
export const permissions = family<PermissionDto, PermissionWrite, PermissionPatch>("/permissions")
export const rateLimits = family<RateLimitDto, RateLimitWrite, RateLimitPatch>("/rate-limits")

/**
 * An OAuth client is **retired**, not deleted: the grants it issued still name
 * it, so the row survives and stops being usable. `/admin/api/oauth-clients/{id}`
 * has no `DELETE` at all, and `remove` says so rather than sending a request
 * that would come back 405.
 */
export const oauthClients = {
  ...family<OAuthClientDto, OAuthClientWrite, OAuthClientPatch>("/oauth-clients"),
  remove: (id: string): Promise<void> =>
    Promise.reject(new Error(`oauth client ${id} is retired, not deleted`)),
  retire: (id: string) =>
    api<OAuthClientDto>(`${BASE}/oauth-clients/${encodeURIComponent(id)}/retire`, { method: "POST" }),
}

/** Membership is a composite key, so it is addressed by both halves. */
function membership<D>(scope: string, key: "organizationId" | "teamId") {
  const at = (id: string, userId: string) =>
    `${BASE}${scope}/${encodeURIComponent(id)}/members/${encodeURIComponent(userId)}`
  return {
    list: (id: string, filter: ListFilter = {}) =>
      // The path names the scope; the query parameter cannot point elsewhere,
      // because the host overwrites it. Sent anyway so the two agree.
      api<Page<D>>(`${BASE}${scope}/${encodeURIComponent(id)}/members${query({ ...filter, [key]: id })}`),
    add: (id: string, write: MemberWrite) =>
      api<D>(`${BASE}${scope}/${encodeURIComponent(id)}/members`, json("POST", write)),
    setRole: (id: string, userId: string, patch: MemberPatch) => api<D>(at(id, userId), json("PUT", patch)),
    remove: (id: string, userId: string) => api<void>(at(id, userId), { method: "DELETE" }),
  }
}

export const organizationMembers = membership<OrganizationMemberDto>("/organizations", "organizationId")
export const teamMembers = membership<TeamMemberDto>("/teams", "teamId")

export const sessions = {
  list: (filter: ListFilter) => api<Page<UserSessionDto>>(`${BASE}/sessions${query({ ...filter })}`),
  revoke: (id: string) => api<void>(`${BASE}/sessions/${encodeURIComponent(id)}`, { method: "DELETE" }),
}

export const audit = {
  list: (filter: AuditFilter) => api<Page<AuditEventDto>>(`${BASE}/audit${query({ ...filter })}`),
}
