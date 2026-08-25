import type { ErrorEnvelope } from "@/generated/ErrorEnvelope"
import type { PortalContextDto } from "@/generated/PortalContextDto"
import type { PortalModelDto } from "@/generated/PortalModelDto"
import type { PortalQuotaWindowDto } from "@/generated/PortalQuotaWindowDto"
import type { PortalRecentQueryDto } from "@/generated/PortalRecentQueryDto"
import type { PortalRecentRequestDto } from "@/generated/PortalRecentRequestDto"
import type { PortalSettingsDto } from "@/generated/PortalSettingsDto"
import type { PortalUsageDto } from "@/generated/PortalUsageDto"
import type { PortalUsageQueryDto } from "@/generated/PortalUsageQueryDto"
import { ApiError, api, json } from "@/api/client"

async function portalApi<T>(path: string, key: string, signal?: AbortSignal): Promise<T> {
  const response = await fetch(path, {
    cache: "no-store",
    credentials: "omit",
    signal,
    headers: {
      accept: "application/json",
      authorization: `Bearer ${key}`,
    },
  })
  if (!response.ok) {
    const body = await response.json().catch(() => null) as ErrorEnvelope | null
    throw new ApiError(response.status, body?.error.message ?? response.statusText)
  }
  return response.json() as Promise<T>
}

export const portalContext = (key: string) =>
  portalApi<PortalContextDto>("/portal/api/context", key)

export const portalModels = (key: string, signal?: AbortSignal) =>
  portalApi<Array<PortalModelDto>>("/portal/api/models", key, signal)

export function portalUsage(key: string, query: PortalUsageQueryDto, signal?: AbortSignal) {
  const params = new URLSearchParams({ from: String(query.from), to: String(query.to) })
  return portalApi<PortalUsageDto>(`/portal/api/usage?${params}`, key, signal)
}

export const portalQuotaWindows = (key: string, signal?: AbortSignal) =>
  portalApi<Array<PortalQuotaWindowDto>>("/portal/api/quota-windows", key, signal)

export function portalRecentRequests(key: string, query: PortalRecentQueryDto, signal?: AbortSignal) {
  const params = new URLSearchParams()
  if (query.limit != null) params.set("limit", String(query.limit))
  const suffix = params.size === 0 ? "" : `?${params}`
  return portalApi<Array<PortalRecentRequestDto>>(`/portal/api/recent-requests${suffix}`, key, signal)
}

export const portalSettings = () =>
  api<PortalSettingsDto>("/admin/portal-settings")

export const savePortalSettings = (value: PortalSettingsDto) =>
  api<PortalSettingsDto>("/admin/portal-settings", json("PATCH", value))
