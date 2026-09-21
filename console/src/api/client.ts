//! One `fetch` wrapper, and the error envelope every v4 surface answers with.
//!
//! `gproxy-host-axum/src/error.rs` renders exactly one product envelope:
//!
//! ```json
//! { "error": { "code": "forbidden", "message": "…" } }
//! ```
//!
//! The envelope has no `ts-rs` declaration because it is the host's shape, not
//! a DTO — so it is written out here rather than imported from
//! `@/generated`, and [`isEnvelope`] is what keeps this file honest about the
//! fact that a body may be anything at all (a proxy's HTML, an empty 502).
//!
//! `code` is the machine-readable half and the only part worth branching on;
//! `message` is already safe to show, because the host replaces the text of
//! any 5xx with a generic line before it leaves the process.

/** The product error envelope. Not generated: the host owns this shape. */
export type ErrorEnvelope = { error: { code: string; message: string } }

function isEnvelope(body: unknown): body is ErrorEnvelope {
  if (typeof body !== "object" || body === null || !("error" in body)) return false
  const error = (body as { error: unknown }).error
  return typeof error === "object" && error !== null && "code" in error && "message" in error
}

export class ApiError extends Error {
  readonly status: number
  /** [`AppError::code`], or `http` when the response carried no envelope. */
  readonly code: string

  constructor(status: number, code: string, message: string) {
    super(message)
    this.name = "ApiError"
    this.status = status
    this.code = code
  }
}

/**
 * Raised once per response that ends a session, so the shell can drop back to
 * the sign-in page from wherever it was. An event rather than a callback
 * because every query and every mutation can be the one that finds out.
 */
export const UNAUTHORIZED_EVENT = "gproxy:unauthorized"

export async function api<T>(path: string, init?: RequestInit): Promise<T> {
  const headers = new Headers(init?.headers)
  if (!headers.has("accept")) headers.set("accept", "application/json")
  const response = await fetch(path, { ...init, credentials: "same-origin", headers })
  if (!response.ok) {
    const body: unknown = await response.json().catch(() => null)
    const envelope = isEnvelope(body) ? body.error : null
    // 401 is the session ending; 403 is a live session being told no, and
    // must not sign anybody out.
    if (response.status === 401) window.dispatchEvent(new Event(UNAUTHORIZED_EVENT))
    throw new ApiError(response.status, envelope?.code ?? "http", envelope?.message ?? response.statusText)
  }
  // Every delete answers 204 with no body; asking for JSON there throws.
  if (response.status === 204) return undefined as T
  return response.json() as Promise<T>
}

/** A JSON request body with its method. */
export function json(method: "POST" | "PUT" | "PATCH" | "DELETE", value: unknown): RequestInit {
  return { method, headers: { "content-type": "application/json" }, body: JSON.stringify(value) }
}

/** A query string built from the fields a filter object actually set. */
export function query(params: Record<string, string | number | boolean | null | undefined>) {
  const search = new URLSearchParams()
  for (const [key, value] of Object.entries(params)) {
    if (value === null || value === undefined || value === "") continue
    search.set(key, String(value))
  }
  const rendered = search.toString()
  return rendered ? `?${rendered}` : ""
}
