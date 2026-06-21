import { infiniteQueryOptions, queryOptions } from "@tanstack/react-query";
import { api } from "./http";
import type { Usage, UsageRollup } from "./usage";
import type { RoutePermission, RateLimit, Quota } from "./authz";
import type { UserKeyView } from "./identity";

export interface UserMe { id: number; name: string; is_admin: boolean; org_id: number; org_name?: string | null; team_id: number | null; team_name?: string | null; }
// F7a returns Effective<T> = { source, ...flattened rule fields } — mirror as intersections:
export type EffPermission = RoutePermission & { source: "user" | "team" | "org" };
export type EffRateLimit = RateLimit & { source: "user" | "team" | "org" };
export type EffQuota = Quota & { source: "user" | "team" | "org" };

// distinct queryKey — NOT ["session"] (main.tsx 401-bounce is scoped to that)
export const portalSessionQuery = queryOptions({
  queryKey: ["portal-session"],
  queryFn: () => api<UserMe>("/user/me"),
  retry: false,
  staleTime: 60_000,
});

// keys
export const myKeysQuery = queryOptions({ queryKey: ["my-keys"], queryFn: () => api<UserKeyView[]>("/user/keys") });
export const createMyKey = (label: string | null) =>
  api<UserKeyView>("/user/keys", { method: "POST", body: JSON.stringify({ label }) });
export const updateMyKey = (id: number, body: { label?: string | null; enabled: boolean }) =>
  api<UserKeyView>(`/user/keys/${id}`, { method: "PATCH", body: JSON.stringify(body) });
export const deleteMyKey = (id: number) => api<void>(`/user/keys/${id}`, { method: "DELETE" });

// usage — mirror api/usage.ts but /user/* and NO user_id
const PAGE = 50;
function myUsageQs(f: { at_from?: number; at_to?: number; route_name?: string; model?: string; before_id?: number; limit?: number }): string {
  const p = new URLSearchParams();
  if (f.limit != null) p.set("limit", String(f.limit));
  if (f.before_id != null) p.set("before_id", String(f.before_id));
  if (f.at_from != null) p.set("at_from", String(f.at_from));
  if (f.at_to != null) p.set("at_to", String(f.at_to));
  if (f.route_name) p.set("route_name", f.route_name);
  if (f.model) p.set("model", f.model);
  const s = p.toString();
  return s ? `?${s}` : "";
}
export type MyUsageFilter = { at_from?: number; at_to?: number; route_name?: string; model?: string };
export const myUsageInfiniteQuery = (f: MyUsageFilter) =>
  infiniteQueryOptions({
    queryKey: ["my-usage", "infinite", f],
    queryFn: ({ pageParam }) =>
      api<Usage[]>(`/user/usage${myUsageQs({ ...f, before_id: pageParam ?? undefined, limit: PAGE })}`),
    initialPageParam: undefined as number | undefined,
    getNextPageParam: (last: Usage[]) =>
      last.length >= PAGE ? last[last.length - 1].id : undefined,
  });
export const myRollupsQuery = (granularity: string, from: number, to: number) =>
  queryOptions({
    queryKey: ["my-rollups", granularity, from, to],
    queryFn: () => api<UsageRollup[]>(`/user/usage-rollups?granularity=${granularity}&from=${from}&to=${to}`),
  });

// authz (effective, read-only)
export const myPermissionsQuery = queryOptions({ queryKey: ["my-permissions"], queryFn: () => api<EffPermission[]>("/user/route-permissions") });
export const myRateLimitsQuery = queryOptions({ queryKey: ["my-rate-limits"], queryFn: () => api<EffRateLimit[]>("/user/rate-limits") });
export const myQuotaQuery = queryOptions({ queryKey: ["my-quota"], queryFn: () => api<EffQuota[]>("/user/quota") });

// account
export const changePassword = (current: string, next: string) =>
  api<void>("/user/change-password", { method: "POST", body: JSON.stringify({ current, new: next }) });
