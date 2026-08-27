import type { ErrorEnvelope } from "@/generated/ErrorEnvelope"

export class ApiError extends Error {
  readonly status: number

  constructor(status: number, message: string) {
    super(message)
    this.name = "ApiError"
    this.status = status
  }
}

export async function api<T>(path: string, init?: RequestInit): Promise<T> {
  const headers = new Headers(init?.headers)
  if (!headers.has("accept")) headers.set("accept", "application/json")
  const response = await fetch(path, {
    ...init,
    credentials: "same-origin",
    headers,
  })
  if (!response.ok) {
    if (response.status === 401) window.dispatchEvent(new Event("gproxy:unauthorized"))
    const body = await response.json().catch(() => null) as ErrorEnvelope | null
    throw new ApiError(response.status, body?.error.message ?? response.statusText)
  }
  if (response.status === 204) return undefined as T
  return response.json() as Promise<T>
}

export function json(method: "POST" | "PATCH" | "DELETE", value: unknown): RequestInit {
  return {
    method,
    headers: { "content-type": "application/json" },
    body: JSON.stringify(value),
  }
}
