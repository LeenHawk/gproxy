import { api, query } from "@/api/client"
export type UpdateReport = { current: string; latest: string; available: boolean; channel: string; source: string; target: string; notes_url: string | null; notes: string | null; restart: string; rollback_available: boolean; checked_at_ms: number }
export type UpdateSchedule = { last_check: UpdateReport | null; last_error: string | null; interval_secs: number | null; automatic: boolean; channel: string; source: string }
export type AppliedUpdate = { version: string | null; changed: boolean; restart: string }
export const updateSchedule = () => api<UpdateSchedule>("/admin/api/update")
export type UpdateSelection = { channel: string; source: string }
export const checkUpdate = ({ channel, source }: UpdateSelection) => api<UpdateReport>(`/admin/api/update/check${query({ channel, source })}`, { method: "POST" })
export const applyUpdate = ({ channel, source }: UpdateSelection) => api<AppliedUpdate>(`/admin/api/update/apply${query({ channel, source })}`, { method: "POST" })
export const rollbackUpdate = () => api<AppliedUpdate>("/admin/api/update/rollback", { method: "POST" })

export type UpdateProgress = { phase: "downloading" | "verifying" | "installing"; downloaded_bytes: number; total_bytes: number }
export const updateProgress = () => api<UpdateProgress | null>("/admin/api/update/progress")
