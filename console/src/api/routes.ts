import { queryOptions } from "@tanstack/react-query";
import { api } from "./http";

export const ROUTE_STRATEGIES = ["failover", "round_robin", "weighted", "least_latency"] as const;
export type RouteStrategy = (typeof ROUTE_STRATEGIES)[number];

export interface Route {
  id: number;
  name: string;
  strategy: string;
  enabled: boolean;
  description: string | null;
  settings_json: unknown;
  created_at: number;
  updated_at: number;
}

export interface RouteInput {
  id?: number | null;
  name: string;
  strategy: string;
  enabled: boolean;
  description?: string | null;
  /** OMIT when none — JSON null round-trips as Some(Value::Null) server-side. */
  settings_json?: unknown;
}

export interface RouteMember {
  id: number;
  route_id: number;
  provider_id: number;
  upstream_model_id: string;
  weight: number;
  tier: number;
  enabled: boolean;
  created_at: number;
  updated_at: number;
}

export interface RouteMemberInput {
  id?: number | null;
  route_id: number;
  provider_id: number;
  upstream_model_id: string;
  weight: number;
  tier: number;
  enabled: boolean;
}

export const routesQuery = queryOptions({
  queryKey: ["routes"],
  queryFn: () => api<Route[]>("/admin/routes"),
});

export const routeQuery = (id: number) =>
  queryOptions({ queryKey: ["routes", id], queryFn: () => api<Route>(`/admin/routes/${id}`) });

export const routeMembersQuery = (routeId: number) =>
  queryOptions({
    queryKey: ["routes", routeId, "members"],
    queryFn: () => api<RouteMember[]>(`/admin/routes/${routeId}/members`),
  });

export function upsertRoute(input: RouteInput): Promise<Route> {
  return api<Route>("/admin/routes", { method: "POST", body: JSON.stringify(input) });
}

export function deleteRoute(id: number): Promise<void> {
  return api<void>(`/admin/routes/${id}`, { method: "DELETE" });
}

export function upsertRouteMember(routeId: number, input: RouteMemberInput): Promise<RouteMember> {
  return api<RouteMember>(`/admin/routes/${routeId}/members`, { method: "POST", body: JSON.stringify(input) });
}

export function deleteRouteMember(id: number): Promise<void> {
  return api<void>(`/admin/route-members/${id}`, { method: "DELETE" });
}
