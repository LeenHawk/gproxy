import type { AutostartStatusDto } from "@/generated/AutostartStatusDto"
import type { AutostartUpdateRequest } from "@/generated/AutostartUpdateRequest"
import type { UpdateAppliedDto } from "@/generated/UpdateAppliedDto"
import type { UpdateStatusDto } from "@/generated/UpdateStatusDto"
import { api, json } from "@/api/client"

export const autostartStatus = () => api<AutostartStatusDto>("/admin/native/autostart")
export const setAutostart = (value: AutostartUpdateRequest) =>
  api<AutostartStatusDto>("/admin/native/autostart", json("PUT", value))
export const updateStatus = () => api<UpdateStatusDto>("/admin/native/update")
export const applyUpdate = () => api<UpdateAppliedDto>("/admin/native/update/apply", json("POST", {}))
export const rollbackUpdate = () => api<UpdateAppliedDto>("/admin/native/update/rollback", json("POST", {}))
