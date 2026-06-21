import { queryOptions } from "@tanstack/react-query";
import { api, ApiError } from "./http";

export type Scope = "org" | "team" | "user";

export interface RoutePermission { id: number; scope: Scope; scope_id: number; route_pattern: string; created_at: number; updated_at: number; }
export interface RoutePermissionInput { id?: number | null; scope: Scope; scope_id: number; route_pattern: string; }
export interface RateLimit { id: number; scope: Scope; scope_id: number; route_pattern: string; rpm: number | null; rpd: number | null; total_tokens: number | null; created_at: number; updated_at: number; }
export interface RateLimitInput { id?: number | null; scope: Scope; scope_id: number; route_pattern: string; rpm?: number | null; rpd?: number | null; total_tokens?: number | null; }
export interface Quota { id: number; scope: Scope; scope_id: number; quota_total: string; cost_used: string; created_at: number; updated_at: number; }
export interface QuotaInput { id?: number | null; scope: Scope; scope_id: number; quota_total: string; cost_used: string; }

const q = (scope: Scope, scopeId: number) => `?scope=${scope}&scope_id=${scopeId}`;

export const permissionsQuery = (scope: Scope, scopeId: number) => queryOptions({
  queryKey: ["route-permissions", scope, scopeId],
  queryFn: () => api<RoutePermission[]>(`/admin/route-permissions${q(scope, scopeId)}`),
});
export const rateLimitsQuery = (scope: Scope, scopeId: number) => queryOptions({
  queryKey: ["rate-limits", scope, scopeId],
  queryFn: () => api<RateLimit[]>(`/admin/rate-limits${q(scope, scopeId)}`),
});
// quota GET returns the single quota or 404 → swallow 404 into null
export const quotaQuery = (scope: Scope, scopeId: number) => queryOptions({
  queryKey: ["quotas", scope, scopeId],
  // 404 (no quota set for this scope) is a normal empty state — swallow it to null
  // here, so only genuine transient failures surface and inherit the global retry:1.
  queryFn: async () => {
    try { return await api<Quota>(`/admin/quotas${q(scope, scopeId)}`); }
    catch (e) { if (e instanceof ApiError && e.status === 404) return null; throw e; }
  },
});

export const upsertPermission = (i: RoutePermissionInput) => api<RoutePermission>("/admin/route-permissions", { method: "POST", body: JSON.stringify(i) });
export const deletePermission = (id: number) => api<void>(`/admin/route-permissions/${id}`, { method: "DELETE" });
export const upsertRateLimit = (i: RateLimitInput) => api<RateLimit>("/admin/rate-limits", { method: "POST", body: JSON.stringify(i) });
export const deleteRateLimit = (id: number) => api<void>(`/admin/rate-limits/${id}`, { method: "DELETE" });
export const upsertQuota = (i: QuotaInput) => api<Quota>("/admin/quotas", { method: "POST", body: JSON.stringify(i) });
export const deleteQuota = (id: number) => api<void>(`/admin/quotas/${id}`, { method: "DELETE" });
