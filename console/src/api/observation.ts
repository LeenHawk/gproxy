import { api, query } from "@/api/client"
import type { PortalUsageDto, PortalUsageQuery } from "@/generated/app"
import type { CaptureDetailDto, LogDetailDto, LogPageDto, LogQuery, Page, UsageRecordDto, UsageRecordQuery } from "@/generated/sdk"
const BASE = "/admin/api"
export const USAGE_READ = "configuration.usage"
export const LOGS_READ = "configuration.logs"
export type LogSide = "downstream" | "upstream"
export const usage = (filter: Partial<PortalUsageQuery>) => api<PortalUsageDto>(`${BASE}/usage${query({ ...filter })}`)
export const records = (filter: Partial<UsageRecordQuery>) => api<Page<UsageRecordDto>>(`${BASE}/usage/records${query({ ...filter })}`)
export const logs = (side: LogSide, filter: Partial<LogQuery>) => api<LogPageDto>(`${BASE}/logs/${side}${query({ ...filter })}`)
export const detail = (id: string) => api<LogDetailDto>(`${BASE}/logs/downstream/${encodeURIComponent(id)}`)
export const capture = (id: string) => api<CaptureDetailDto>(`${BASE}/logs/captures/${encodeURIComponent(id)}`)
