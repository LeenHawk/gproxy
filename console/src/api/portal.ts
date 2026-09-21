//! `/portal/api` — the surface every authenticated caller has.
//!
//! Read `gproxy-host-axum/src/portal.rs` next to this file: there is one
//! function here per route there, in the same order, and nothing here that is
//! not routed there. The portal's guard asks for an authenticated caller and
//! nothing more, because every operation behind it is scoped to that caller by
//! construction — which is why a function in this file never takes a user id.

import type {
  PortalKeyCreate,
  PortalKeyCreated,
  PortalKeyDto,
  PortalKeySecretDto,
  PortalModelDto,
  PortalOAuthSessionDto,
  PortalPasswordChange,
  PortalQuotaWindowDto,
  PortalRequestDto,
  PortalUsageDto,
  PortalUsageQuery,
  UserSessionDto,
} from "@/generated/app"
import { api, json, query } from "@/api/client"

const BASE = "/portal/api"

export function models() {
  return api<Array<PortalModelDto>>(`${BASE}/models`)
}

/**
 * The caller's own usage. `Partial` because `ts-rs` renders `Option<T>` as
 * `T | null` rather than `field?: T`, which is right for a response and wrong
 * for a filter: serde treats an absent field and a null one alike, and
 * [`query`] drops whatever the caller left out.
 */
export function usage(filter: Partial<PortalUsageQuery>) {
  return api<PortalUsageDto>(`${BASE}/usage${query({ ...filter })}`)
}

export function quota() {
  return api<Array<PortalQuotaWindowDto>>(`${BASE}/quota`)
}

/**
 * The caller's own recent requests. Empty when the instance has
 * `portal_recent_requests_enabled` off, which `features.canSeeLogs` reports
 * ahead of time so the page can say so rather than look broken.
 */
export function requests() {
  return api<Array<PortalRequestDto>>(`${BASE}/requests`)
}

/** The caller's live console/portal sign-ins, including the current one. */
export function sessions() {
  return api<Array<UserSessionDto>>(`${BASE}/sessions`)
}

export const keys = {
  list: () => api<Array<PortalKeyDto>>(`${BASE}/keys`),
  create: (write: PortalKeyCreate) => api<PortalKeyCreated>(`${BASE}/keys`, json("POST", write)),
  remove: (id: string) => api<void>(`${BASE}/keys/${encodeURIComponent(id)}`, { method: "DELETE" }),
  rotate: (id: string) => api<PortalKeyCreated>(`${BASE}/keys/${encodeURIComponent(id)}/rotate`, { method: "POST" }),
  reveal: (id: string) => api<PortalKeySecretDto>(`${BASE}/keys/${encodeURIComponent(id)}/secret`),
}

export const oauthSessions = {
  list: () => api<Array<PortalOAuthSessionDto>>(`${BASE}/oauth-sessions`),
  revoke: (id: string) => api<void>(`${BASE}/oauth-sessions/${encodeURIComponent(id)}`, { method: "DELETE" }),
}

export function changePassword(change: PortalPasswordChange) {
  return api<void>(`${BASE}/password`, json("POST", change))
}
