import { api, query } from "@/api/client"
export type UpdateReport = { current: string; latest: string; available: boolean; channel: string; target: string; notes_url: string | null; notes: string | null; restart: string; rollback_available: boolean; checked_at_ms: number }
export type UpdateSchedule = { last_check: UpdateReport | null; last_error: string | null; interval_secs: number | null; automatic: boolean }
export type AppliedUpdate = { version: string | null; changed: boolean; restart: string }
export const updateSchedule = () => api<UpdateSchedule>("/admin/api/update")
export const checkUpdate = (channel: string) => api<UpdateReport>(`/admin/api/update/check${query({ channel })}`, { method: "POST" })
export const applyUpdate = (channel: string) => api<AppliedUpdate>(`/admin/api/update/apply${query({ channel })}`, { method: "POST" })
export const rollbackUpdate = () => api<AppliedUpdate>("/admin/api/update/rollback", { method: "POST" })
