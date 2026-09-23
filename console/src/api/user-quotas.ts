import { api, json, query } from "@/api/client"
import type { BudgetStatusDto, Page, QuotaDto, QuotaPatch, QuotaWrite } from "@/generated/sdk"

export const userQuotaKey = (userId: string) => ["admin", "user-quotas", userId] as const
export async function userQuotas(userId: string) {
  const rows: QuotaDto[] = []
  for (let page = 1; ; page += 1) {
    const result = await api<Page<QuotaDto>>(`/admin/api/quotas${query({ ownerKind: "user", ownerId: userId, page, pageSize: 500 })}`)
    rows.push(...result.items)
    if (rows.length >= result.total || result.items.length === 0) return rows
  }
}
export const userBudgetStatus = (userId: string) =>
  api<BudgetStatusDto[]>(`/admin/api/quotas/status${query({ owners: `user:${userId}` })}`)
export const createUserQuota = (userId: string, body: Partial<QuotaWrite>) =>
  api<QuotaDto>("/admin/api/quotas", json("POST", { ...body, ownerKind: "user", ownerId: userId, metric: "cost", unit: "USD" }))
export const updateUserQuota = (id: string, body: Partial<QuotaPatch>) =>
  api<QuotaDto>(`/admin/api/quotas/${encodeURIComponent(id)}`, json("PATCH", body))
export const deleteUserQuota = (id: string) =>
  api<void>(`/admin/api/quotas/${encodeURIComponent(id)}`, { method: "DELETE" })
export const resetUserQuota = (id: string) =>
  api<unknown>(`/admin/api/quotas/${encodeURIComponent(id)}/reset`, { method: "POST" })
