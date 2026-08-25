import type { AuthResponse } from "@/generated/AuthResponse"
import type { LoginRequest } from "@/generated/LoginRequest"
import type { SessionStatusDto } from "@/generated/SessionStatusDto"
import type { SetupRequest } from "@/generated/SetupRequest"
import { api, json } from "@/api/client"

export const session = () => api<SessionStatusDto>("/admin/session")
export const setup = (request: SetupRequest) =>
  api<AuthResponse>("/admin/setup", json("POST", request))
export const login = (request: LoginRequest) =>
  api<AuthResponse>("/admin/login", json("POST", request))
export const logout = () => api<void>("/admin/logout", { method: "POST" })
