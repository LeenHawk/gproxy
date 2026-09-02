import type { AuditEventDto } from "@/generated/AuditEventDto"
import type { ChannelDto } from "@/generated/ChannelDto"
import type { CredentialQuotaCycleDto } from "@/generated/CredentialQuotaCycleDto"
import type { QuotaWindowDto } from "@/generated/QuotaWindowDto"
import type { TlsPresetDto } from "@/generated/TlsPresetDto"
import type { UsageQueryDto } from "@/generated/UsageQueryDto"
import type { UsageStatisticsDto } from "@/generated/UsageStatisticsDto"
import type { UsageTrendPointDto } from "@/generated/UsageTrendPointDto"
import type { UsageTrendQueryDto } from "@/generated/UsageTrendQueryDto"
import type { LogDetailDto } from "@/generated/LogDetailDto"
import type { LogPageDto } from "@/generated/LogPageDto"
import type { LogQueryDto } from "@/generated/LogQueryDto"
import { api } from "@/api/client"

const queryString = (entries: object) => {
  const query = new URLSearchParams()
  for (const [key, value] of Object.entries(entries)) {
    if (value != null && value !== "") query.set(key, String(value))
  }
  return query.toString()
}

export const channels = () => api<Array<ChannelDto>>("/admin/api/channels")
export const tlsPresets = () => api<Array<TlsPresetDto>>("/admin/api/tls-presets")
export const usage = (value: UsageQueryDto) =>
  api<Array<UsageStatisticsDto>>(`/admin/api/usage?${queryString(value)}`)
export const usageTrend = (value: UsageTrendQueryDto) =>
  api<Array<UsageTrendPointDto>>(`/admin/api/usage-trend?${queryString(value)}`)
export const quotaWindows = (subjectKind?: string, subjectId?: number) =>
  api<Array<QuotaWindowDto>>(
    `/admin/api/quota-windows?${queryString({ subject_kind: subjectKind ?? "", subject_id: subjectId ?? null })}`,
  )
export const credentialCycles = (from: number, to: number, credentialId?: number) =>
  api<Array<CredentialQuotaCycleDto>>(
    `/admin/api/credential-cycles?${queryString({ from, to, credential_id: credentialId ?? null })}`,
  )
export const audit = (limit = 100) => api<Array<AuditEventDto>>(`/admin/api/audit?limit=${limit}`)
export const logs = (value: LogQueryDto) => api<LogPageDto>(`/admin/api/logs?${queryString(value)}`)
export const logDetail = (requestId: string) => api<LogDetailDto>(`/admin/api/logs/${encodeURIComponent(requestId)}`)
