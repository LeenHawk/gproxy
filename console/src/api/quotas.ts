import { configFamily } from "@/api/config-family"
import { api, json, query } from "@/api/client"
import type { BudgetStatusDto, QuotaDto, QuotaPatch, QuotaWrite } from "@/generated/sdk"
export const quotas = configFamily<QuotaDto, Partial<QuotaWrite>, Partial<QuotaPatch>>("/quotas")
export const budgetStatus = (ownerKind: string, ownerId: string) => api<BudgetStatusDto[]>(`/admin/api/quotas/status${query({ owners: `${ownerKind}:${ownerId}` })}`)
export const resetQuota = (row: QuotaDto) => api(`/admin/api/quotas/${encodeURIComponent(row.id)}/${["provider", "credential"].includes(row.ownerKind) ? "limit-reset" : "reset"}`, json("POST", {}))
