import { infiniteQueryOptions, queryOptions } from "@tanstack/react-query";
import { api } from "./http";

export interface Usage {
  id: number;
  request_id: string;
  at: number;
  route_name: string | null;
  provider_id: number | null;
  credential_id: number | null;
  org_id: number | null;
  team_id: number | null;
  user_id: number | null;
  user_key_id: number | null;
  operation: string;
  kind: string;
  model: string | null;
  input_tokens: number;
  output_tokens: number;
  cache_read_tokens: number;
  cache_creation_5m_tokens: number;
  cache_creation_1h_tokens: number;
  cost: string;
  latency_ms: number;
  usage_source: string;
  ended: string;
}

export interface UsageRollup {
  id: number;
  granularity: string;
  bucket_start: number;
  provider_id: number | null;
  org_id: number | null;
  team_id: number | null;
  user_id: number | null;
  route_name: string | null;
  model: string | null;
  requests: number;
  input_tokens: number;
  output_tokens: number;
  cost: string;
}

export interface DownstreamRequest {
  id: number;
  request_id: string;
  at: number;
  method: string;
  path: string;
  query: string | null;
  status: number;
  headers_json: unknown;
  body: string | null;
  response_body: string | null;
}

export interface UpstreamRequest {
  id: number;
  request_id: string;
  at: number;
  provider_id: number | null;
  credential_id: number | null;
  url: string;
  method: string;
  status: number;
  latency_ms: number;
  headers_json: unknown;
  body: string | null;
  response_body: string | null;
}

export interface AuditLog {
  id: number;
  at: number;
  actor_id: number | null;
  actor_name: string | null;
  action: string;
  target: string;
  status: number;
  source_ip: string | null;
}

export interface CredentialStatus {
  id: number;
  credential_id: number;
  channel: string;
  health_kind: string;
  health_json: { state?: string; open_until?: number; reason?: string } | null;
  checked_at: number | null;
  last_error: string | null;
}

export interface UsageFilter {
  at_from?: number;
  at_to?: number;
  provider_id?: number;
  user_id?: number;
  route_name?: string;
  model?: string;
  before_id?: number;
  limit?: number;
}

function usageQs(f: UsageFilter): string {
  const p = new URLSearchParams();
  if (f.limit != null) p.set("limit", String(f.limit));
  if (f.before_id != null) p.set("before_id", String(f.before_id));
  if (f.at_from != null) p.set("at_from", String(f.at_from));
  if (f.at_to != null) p.set("at_to", String(f.at_to));
  if (f.provider_id != null) p.set("provider_id", String(f.provider_id));
  if (f.user_id != null) p.set("user_id", String(f.user_id));
  if (f.route_name) p.set("route_name", f.route_name);
  if (f.model) p.set("model", f.model);
  const s = p.toString();
  return s ? `?${s}` : "";
}

export const usageQuery = (f: UsageFilter) =>
  queryOptions({
    queryKey: ["usage", f],
    queryFn: () => api<Usage[]>(`/admin/usage${usageQs(f)}`),
  });

export const PAGE = 50;

// Infinite (keyset) variant for the usage explorer.
// Each page is Usage[] DESC by id; cursor = last row id; undefined when page underfills.
export const usageInfiniteQuery = (f: Omit<UsageFilter, "before_id" | "limit">) =>
  infiniteQueryOptions({
    queryKey: ["usage", "infinite", f],
    queryFn: ({ pageParam }) =>
      api<Usage[]>(`/admin/usage${usageQs({ ...f, before_id: pageParam ?? undefined, limit: PAGE })}`),
    initialPageParam: undefined as number | undefined,
    getNextPageParam: (last: Usage[]) =>
      last.length >= PAGE ? last[last.length - 1].id : undefined,
  });

/** Recent downstream request logs (id desc, keyset-paginated). */
export const logsInfiniteQuery = () =>
  infiniteQueryOptions({
    queryKey: ["logs", "infinite"],
    queryFn: ({ pageParam }) =>
      api<DownstreamRequest[]>(`/admin/logs?limit=${PAGE}${pageParam != null ? `&before_id=${pageParam}` : ""}`),
    initialPageParam: undefined as number | undefined,
    getNextPageParam: (last: DownstreamRequest[]) =>
      last.length >= PAGE ? last[last.length - 1].id : undefined,
  });

export const rollupsQuery = (granularity: string, from: number, to: number) =>
  queryOptions({
    queryKey: ["usage-rollups", granularity, from, to],
    queryFn: () =>
      api<UsageRollup[]>(
        `/admin/usage-rollups?granularity=${granularity}&from=${from}&to=${to}`,
      ),
  });

export const downstreamLogsQuery = (rid: string) =>
  queryOptions({
    queryKey: ["logs", rid, "downstream"],
    queryFn: () => api<DownstreamRequest[]>(`/admin/logs/${rid}/downstream`),
  });

export const upstreamLogsQuery = (rid: string) =>
  queryOptions({
    queryKey: ["logs", rid, "upstream"],
    queryFn: () => api<UpstreamRequest[]>(`/admin/logs/${rid}/upstream`),
  });

export const auditQuery = (limit = 100) =>
  queryOptions({
    queryKey: ["audit", limit],
    queryFn: () => api<AuditLog[]>(`/admin/audit?limit=${limit}`),
  });

export const credentialStatusesQuery = queryOptions({
  queryKey: ["credential-statuses"],
  queryFn: () => api<CredentialStatus[]>("/admin/credential-statuses"),
  staleTime: 30_000,
});
