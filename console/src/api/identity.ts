import type { OrganizationDto } from "@/generated/OrganizationDto"
import type { OrganizationWriteRequest } from "@/generated/OrganizationWriteRequest"
import type { PermissionDto } from "@/generated/PermissionDto"
import type { PermissionWriteRequest } from "@/generated/PermissionWriteRequest"
import type { QuotaDto } from "@/generated/QuotaDto"
import type { QuotaWriteRequest } from "@/generated/QuotaWriteRequest"
import type { RateLimitDto } from "@/generated/RateLimitDto"
import type { RateLimitWriteRequest } from "@/generated/RateLimitWriteRequest"
import type { TeamDto } from "@/generated/TeamDto"
import type { TeamWriteRequest } from "@/generated/TeamWriteRequest"
import type { UserDto } from "@/generated/UserDto"
import type { UserKeyCreateRequest } from "@/generated/UserKeyCreateRequest"
import type { UserKeyCreateResponse } from "@/generated/UserKeyCreateResponse"
import type { UserKeyDto } from "@/generated/UserKeyDto"
import type { UserKeyRevealResponse } from "@/generated/UserKeyRevealResponse"
import type { UserKeyUpdateRequest } from "@/generated/UserKeyUpdateRequest"
import type { UserWriteRequest } from "@/generated/UserWriteRequest"
import { api, json } from "@/api/client"
import { deleteEntity } from "@/api/control"

const save = <T>(path: string, value: T, id?: number) =>
  api(id == null ? path : `${path}/${id}`, json(id == null ? "POST" : "PATCH", value))

export const organizations = () => api<Array<OrganizationDto>>("/admin/organizations")
export const saveOrganization = (value: OrganizationWriteRequest, id?: number) =>
  save("/admin/organizations", value, id)
export const teams = () => api<Array<TeamDto>>("/admin/teams")
export const saveTeam = (value: TeamWriteRequest, id?: number) =>
  save("/admin/teams", value, id)
export const users = () => api<Array<UserDto>>("/admin/users")
export const saveUser = (value: UserWriteRequest, id?: number) =>
  save("/admin/users", value, id)
export const userKeys = () => api<Array<UserKeyDto>>("/admin/user-keys")
export const createUserKey = (value: UserKeyCreateRequest) =>
  api<UserKeyCreateResponse>("/admin/user-keys", json("POST", value))
export const updateUserKey = (id: number, value: UserKeyUpdateRequest) =>
  api(`/admin/user-keys/${id}`, json("PATCH", value))
export const revealUserKey = (id: number) =>
  api<UserKeyRevealResponse>(`/admin/user-keys/${id}/reveal`, { method: "POST" })
export const permissions = () => api<Array<PermissionDto>>("/admin/permissions")
export const savePermission = (value: PermissionWriteRequest, id?: number) =>
  save("/admin/permissions", value, id)
export const rateLimits = () => api<Array<RateLimitDto>>("/admin/rate-limits")
export const saveRateLimit = (value: RateLimitWriteRequest, id?: number) =>
  save("/admin/rate-limits", value, id)
export const quotas = () => api<Array<QuotaDto>>("/admin/quotas")
export const saveQuota = (value: QuotaWriteRequest, id?: number) =>
  save("/admin/quotas", value, id)
export const removeIdentityRule = (kind: "permissions" | "rate-limits" | "quotas", id: number) =>
  deleteEntity(kind, id)
