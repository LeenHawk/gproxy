import type { AutostartStatusDto } from "@/generated/AutostartStatusDto"
import type { AutostartUpdateRequest } from "@/generated/AutostartUpdateRequest"
import { api, json } from "@/api/client"

export const autostartStatus = () => api<AutostartStatusDto>("/admin/native/autostart")
export const setAutostart = (value: AutostartUpdateRequest) =>
  api<AutostartStatusDto>("/admin/native/autostart", json("PUT", value))
