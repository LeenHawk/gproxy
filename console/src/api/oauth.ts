import type { IdResponse } from "@/generated/IdResponse"
import type { OAuthAuthorizationRequest } from "@/generated/OAuthAuthorizationRequest"
import type { OAuthAuthorizeDecision } from "@/generated/OAuthAuthorizeDecision"
import type { OAuthClientDto } from "@/generated/OAuthClientDto"
import type { OAuthClientWriteRequest } from "@/generated/OAuthClientWriteRequest"
import type { OAuthConsentDto } from "@/generated/OAuthConsentDto"
import type { OAuthDeviceDecision } from "@/generated/OAuthDeviceDecision"
import type { OAuthErrorDto } from "@/generated/OAuthErrorDto"
import type { OAuthRedirectDto } from "@/generated/OAuthRedirectDto"
import { ApiError, api, json } from "@/api/client"

export const oauthClients = (signal?: AbortSignal) => api<Array<OAuthClientDto>>("/admin/api/oauth-clients", { signal })
export const createOAuthClient = (value: OAuthClientWriteRequest) => api<IdResponse>("/admin/api/oauth-clients", json("POST", value))
export const updateOAuthClient = (id: number, value: OAuthClientWriteRequest) => api<void>(`/admin/api/oauth-clients/${id}`, json("PATCH", value))
export const deleteOAuthClient = (id: number) => api<void>(`/admin/api/oauth-clients/${id}`, { method: "DELETE" })

async function oauthApi<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(path, { ...init, credentials: "same-origin", cache: "no-store" })
  if (!response.ok) {
    const error = await response.json().catch(() => null) as OAuthErrorDto | null
    throw new ApiError(response.status, error?.error ?? response.statusText)
  }
  return response.json() as Promise<T>
}

export function oauthAuthorization(query: string): OAuthAuthorizationRequest {
  const params = new URLSearchParams(query)
  return {
    response_type: params.get("response_type") ?? "",
    client_id: params.get("client_id") ?? "",
    redirect_uri: params.get("redirect_uri") ?? "",
    scope: params.get("scope") ?? "",
    code_challenge: params.get("code_challenge") ?? "",
    code_challenge_method: params.get("code_challenge_method") ?? "",
    state: params.get("state") ?? "",
  }
}

export const oauthConsent = (query: string, signal?: AbortSignal) => oauthApi<OAuthConsentDto>(`/oauth/authorize/details?${query}`, { signal })
export const decideOAuthAuthorization = (value: OAuthAuthorizeDecision) => oauthApi<OAuthRedirectDto>("/oauth/authorize", json("POST", value))
export const oauthDeviceConsent = (userCode: string, signal?: AbortSignal) => oauthApi<OAuthConsentDto>(`/oauth/device/details?${new URLSearchParams({ user_code: userCode })}`, { signal })
export const decideOAuthDevice = (value: OAuthDeviceDecision) => oauthApi<void>("/oauth/device/decision", json("POST", value))
