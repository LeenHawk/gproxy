import type { AuditEventDto } from "@/generated/AuditEventDto"
import type { ChannelDto } from "@/generated/ChannelDto"
import type { CredentialQuotaCycleDto } from "@/generated/CredentialQuotaCycleDto"
import type { QuotaWindowDto } from "@/generated/QuotaWindowDto"
import type { TlsPresetDto } from "@/generated/TlsPresetDto"
import type { UsageAggregateDto } from "@/generated/UsageAggregateDto"
import type { UsageQueryDto } from "@/generated/UsageQueryDto"
import type { LogDetailDto } from "@/generated/LogDetailDto"
import type { LogPageDto } from "@/generated/LogPageDto"
import type { LogQueryDto } from "@/generated/LogQueryDto"
import type { LogSettingsDto } from "@/generated/LogSettingsDto"
import type { LogSettingsUpdateDto } from "@/generated/LogSettingsUpdateDto"
import { api, json } from "@/api/client"

const queryString = (entries: object) => {
  const query = new URLSearchParams()
  for (const [key, value] of Object.entries(entries)) {
    if (value != null && value !== "") query.set(key, String(value))
  }
  return query.toString()
}

export const channels = () => api<Array<ChannelDto>>("/admin/channels")
export const tlsPresets = () => api<Array<TlsPresetDto>>("/admin/tls-presets")
export const usage = (value: UsageQueryDto) =>
  api<Array<UsageAggregateDto>>(`/admin/usage?${queryString(value)}`)
export const quotaWindows = (subjectKind?: string, subjectId?: number) =>
  api<Array<QuotaWindowDto>>(
    `/admin/quota-windows?${queryString({ subject_kind: subjectKind ?? "", subject_id: subjectId ?? null })}`,
  )
export const credentialCycles = (from: number, to: number, credentialId?: number) =>
  api<Array<CredentialQuotaCycleDto>>(
    `/admin/credential-cycles?${queryString({ from, to, credential_id: credentialId ?? null })}`,
  )
export const audit = (limit = 100) => api<Array<AuditEventDto>>(`/admin/audit?limit=${limit}`)
export const logs = (value: LogQueryDto) => api<LogPageDto>(`/admin/logs?${queryString(value)}`)
export const logDetail = (requestId: string) => api<LogDetailDto>(`/admin/logs/${encodeURIComponent(requestId)}`)
export const logSettings = () => api<LogSettingsDto>("/admin/log-settings")
export const saveLogSettings = (value: LogSettingsUpdateDto) =>
  api<LogSettingsDto>("/admin/log-settings", json("PATCH", value))
