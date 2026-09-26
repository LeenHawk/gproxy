//! The consent half of the gateway's own OAuth issuer.
//!
//! Two flows end on a page of this console. A client that sends a browser to
//! `{issuer}/oauth/authorize` has the host redirect it to `/console/authorize`
//! with the request's query intact; a device that started the device flow
//! shows its user a code and `/console/device`. The page answers with the
//! session cookie; the host also takes a management API key there, and never
//! an OAuth token or an ordinary key.
//!
//! The authorization endpoint answers in RFC 6749's error shape rather than
//! the product envelope — `error` is a string code there — so it has its own
//! request helper instead of [`api`].

import type { AuthorizeDetails, AuthorizeOutcome, ConsentDecision, DeviceDecided, DeviceDetails } from "@/generated/app"
import { api, ApiError, json } from "@/api/client"
import { send } from "@/lib/transport"

/** The issuer's authorization endpoint on the aggregated mount. */
const AUTHORIZE = "/v1/oauth/authorize"

type OAuthEnvelope = { error: string; error_description?: string }

async function authorizeRequest<T>(search: string, init?: RequestInit): Promise<T> {
  const headers = new Headers(init?.headers)
  headers.set("accept", "application/json")
  const response = await send(`${AUTHORIZE}${search}`, { ...init, credentials: "same-origin", headers })
  const body: unknown = await response.json().catch(() => null)
  if (!response.ok) {
    const envelope = typeof body === "object" && body !== null && typeof (body as OAuthEnvelope).error === "string"
      ? body as OAuthEnvelope
      : null
    throw new ApiError(response.status, envelope?.error ?? "http", envelope?.error_description ?? response.statusText)
  }
  return body as T
}

/** What the consent page shows for the request in `search`, validated as an approval would be. */
export function authorizeDetails(search: string) {
  return authorizeRequest<AuthorizeDetails>(search)
}

/** The person's answer, and where the browser goes next: back to the client either way. */
export function authorizeDecide(search: string, decision: ConsentDecision) {
  return authorizeRequest<{ location: string; outcome: AuthorizeOutcome }>(search, json("POST", { decision }))
}

/** The pending device authorization behind a code the person typed. */
export function deviceDetails(userCode: string) {
  return api<DeviceDetails>(`/portal/api/oauth/device?userCode=${encodeURIComponent(userCode)}`)
}

export function deviceDecide(userCode: string, decision: ConsentDecision) {
  return api<DeviceDecided>("/portal/api/oauth/device", json("POST", { userCode, decision }))
}
