//! Signing in, asking who you are, and signing out.
//!
//! The session is a cookie the host sets on `POST /portal/api/login`; the
//! token is also in the body, for a client with no cookie jar, and the console
//! deliberately ignores that half — storing a bearer token in
//! `localStorage` would trade an `HttpOnly` cookie for something a script on
//! the page can read.
//!
//! Sign-in and sign-out are on the **portal** surface, not the admin one: they
//! run before there is a caller, so they cannot sit behind a guard that
//! demands one. `DELETE /admin/api/session` is the same operation behind the
//! other front door, and the console uses the portal's so that an ordinary
//! account can sign out too.

import type { AdminContextDto, PortalContextDto } from "@/generated/app"
import { api, json, ApiError, getAdminScope, setAdminScope, UNAUTHORIZED_EVENT } from "@/api/client"

/**
 * `GET /admin/api/session`. Not a DTO and so not generated: it is declared
 * inline in `gproxy-host-axum/src/admin.rs`. Reaching it at all is the
 * interesting part — the surface's guard requires an instance administrator,
 * so a 403 here is the answer, not a failure.
 */
export type SessionStatus = { userId: string; userRole: string; apiKeyId: string | null }

/** What a sign-in returns. The console uses only `expiresAtMs`. */
export type IssuedSession = { sessionId: string; token: string; expiresAtMs: number }

export function signIn(name: string, password: string) {
  return api<IssuedSession>("/portal/api/login", json("POST", { name, password }))
}

export async function signOut() {
  const result = await api<{ endedSession: boolean }>("/portal/api/logout", { method: "POST" })
  setAdminScope(null)
  window.dispatchEvent(new Event(UNAUTHORIZED_EVENT))
  return result
}

/**
 * Everything the shell renders itself from, in one call. A 401 here is how
 * the application learns it has no session; [`api`] turns it into the
 * unauthorized event the shell listens for.
 */
export type SessionContext = PortalContextDto & { admin: AdminContextDto | null }
export async function context(): Promise<SessionContext> {
  const portal = await api<PortalContextDto>("/portal/api/context")
  const requested = getAdminScope()
  let admin: AdminContextDto | null = null
  try {
    admin = await api<AdminContextDto>("/admin/api/context")
  } catch (error) {
    if (!(error instanceof ApiError) || error.status !== 403) throw error
    // A revoked selected scope may coexist with another valid membership.
    if (requested !== null) {
      setAdminScope(null)
      try { admin = await api<AdminContextDto>("/admin/api/context") }
      catch (retry) { if (!(retry instanceof ApiError) || retry.status !== 403) throw retry }
    }
  }
  setAdminScope(admin?.scope?.selector ?? null, admin?.scopeHeader)
  return { ...portal, admin }
}
